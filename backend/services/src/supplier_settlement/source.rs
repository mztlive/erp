//! W27 不可变来源证据录入与 D32 正式事实派生。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{
    AccessControlExt, NoTransaction, SupplierFulfillmentExt, SupplierSettlementExt, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{SupplierFulfillmentOrderId, SupplierRefundFactId};
use entities::money::{line_amounts, Amount};
use entities::supplier_fulfillment::{AllocationAction, CancelStatus, FulfillmentStatus};
use entities::supplier_settlement::{
    SettlementSourceFactType, SupplierSettlementSourceEvidence, SupplierSettlementSourceEvidenceData,
    SupplierSettlementSourceEvidenceLine,
};
use id_generator::next_id;
use validator::Validate;

use super::{
    digest_parts, RecordSettlementSourceEvidenceRequest, SupplierSettlementService,
    SupplierSettlementSourceEvidenceQuery,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::supplier_settlement::SupplierSettlementSourceEvidenceView;

impl SupplierSettlementService {
    /// 查询供应商与周期下最新的完整来源证据批次。
    ///
    /// 该读模型供创建表单取得服务端冻结的期间策略，不返回逐行金额。
    pub async fn latest_source_evidence(
        &self,
        query: &SupplierSettlementSourceEvidenceQuery,
    ) -> Result<SupplierSettlementSourceEvidenceView> {
        query.validate()?;
        let period_start = parse_business_date(&query.period_start, "结算期间开始")?;
        let period_end = parse_business_date(&query.period_end, "结算期间结束")?;
        if period_end < period_start {
            return Err(Error::ValidationError("结算期间结束不得早于开始".to_string()));
        }
        self.db
            .supplier_settlement_source_evidence()
            .latest_for_scope(&query.supplier_id, period_start, period_end, &mut NoTransaction)
            .await?
            .map(Into::into)
            .ok_or_else(|| {
                Error::NotFound("SOURCE_EVIDENCE_MISSING: 当前供应商与期间没有完整来源证据批次".to_string())
            })
    }

    /// 录入一个经服务端逐行核验、不可变且可幂等恢复的来源证据批次。
    ///
    /// 订单成本和退款三元组只从 D32 正式事实派生；客户端只能补充仓库尚无模型的
    /// 运费、服务费、取消时间与供应商账单行，并必须携带正式证据引用。
    ///
    /// # 错误
    /// 策略/周期非法、订单与行不精确配对、事实不在周期内、金额恒等失败或幂等键
    /// 被不同命令复用时 fail-closed。
    pub async fn record_source_evidence(
        &self,
        req: RecordSettlementSourceEvidenceRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementSourceEvidenceView> {
        req.validate()?;
        let request_hash = source_request_hash(&req);
        if let Some(existing) = self
            .db
            .supplier_settlement_source_evidence()
            .find_by_request_id(&req.request_id, &mut NoTransaction)
            .await?
        {
            if existing.request_hash != request_hash {
                return Err(Error::ConflictError("来源证据请求ID已用于不同命令".to_string()));
            }
            return Ok(existing.into());
        }
        let evidence = self
            .build_source_evidence(&req, actor, request_hash.clone())
            .await?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.source_evidence.record",
            "supplier_settlement_source_evidence",
            evidence.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let evidence_for_tx = evidence.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_source_evidence()
                        .create(&evidence_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(existing) = self
                .db
                .supplier_settlement_source_evidence()
                .find_by_request_id(&req.request_id, &mut NoTransaction)
                .await?
            {
                if existing.request_hash != request_hash {
                    return Err(Error::ConflictError("来源证据请求ID已用于不同命令".to_string()));
                }
                return Ok(existing.into());
            }
            return Err(error);
        }
        Ok(evidence.into())
    }

    async fn build_source_evidence(
        &self,
        req: &RecordSettlementSourceEvidenceRequest,
        actor: &AuditActor,
        request_hash: String,
    ) -> Result<SupplierSettlementSourceEvidence> {
        let period_start = parse_business_date(&req.period_start, "结算期间开始")?;
        let period_end = parse_business_date(&req.period_end, "结算期间结束")?;
        if period_end < period_start {
            return Err(Error::ValidationError("结算期间结束不得早于开始".to_string()));
        }
        if req.timezone.trim() != "Asia/Shanghai" {
            return Err(Error::ValidationError(
                "当前结算期间策略只支持 Asia/Shanghai 时区".to_string(),
            ));
        }
        ensure_distinct_source_lines(&req.lines)?;
        if let Some(latest) = self
            .db
            .supplier_settlement_source_evidence()
            .latest_for_period(
                &req.supplier_id,
                period_start,
                period_end,
                req.period_policy_id.trim(),
                req.period_policy_version.trim(),
                &mut NoTransaction,
            )
            .await?
        {
            if req.source_version <= latest.source_version {
                return Err(Error::ConflictError(format!(
                    "来源版本必须高于当前版本 {}",
                    latest.source_version
                )));
            }
        }
        let item_ids = req
            .lines
            .iter()
            .map(|line| line.supplier_fulfillment_item_id.to_string())
            .collect::<HashSet<_>>();
        let orders = self
            .db
            .supplier_fulfillment_orders()
            .find_many(
                mongodb::bson::doc! {
                    "supplier_id": req.supplier_id.to_string(),
                    "deleted_at": 0_i64,
                },
                &mut NoTransaction,
            )
            .await?;
        let order_ids = orders
            .iter()
            .map(|order| order.base.id.clone())
            .collect::<HashSet<_>>();
        if order_ids.is_empty() {
            return Err(Error::NotFound(
                "当前供应商没有可核验的供应商履约订单".to_string(),
            ));
        }
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_many(
                mongodb::bson::doc! {
                    "supplier_fulfillment_order_id": {
                        "$in": order_ids.iter().cloned().collect::<Vec<_>>()
                    },
                    "deleted_at": 0_i64,
                },
                &mut NoTransaction,
            )
            .await?;
        let order_map = orders
            .into_iter()
            .map(|value| (value.base.id.clone(), value))
            .collect::<HashMap<_, _>>();
        let item_map = items
            .into_iter()
            .map(|value| (value.base.id.clone(), value))
            .collect::<HashMap<_, _>>();
        let typed_order_ids = order_ids
            .iter()
            .map(SupplierFulfillmentOrderId::new)
            .collect::<Vec<_>>();
        let refund_facts = self
            .db
            .supplier_refund_facts()
            .find_refund_facts_by_order_ids(&typed_order_ids, &mut NoTransaction)
            .await?;
        let refund_fact_ids = refund_facts
            .iter()
            .map(|fact| SupplierRefundFactId::new(fact.base.id.as_str()))
            .collect::<Vec<_>>();
        let refund_allocations = self
            .db
            .supplier_refund_allocations()
            .find_allocations_by_fact_ids(&refund_fact_ids, &mut NoTransaction)
            .await?;
        let refund_fact_map = refund_facts
            .iter()
            .map(|fact| (fact.base.id.as_str(), fact))
            .collect::<HashMap<_, _>>();
        ensure_complete_source_scope(CompleteSourceScope {
            inputs: &req.lines,
            input_item_ids: &item_ids,
            order_map: &order_map,
            item_map: &item_map,
            refund_allocations: &refund_allocations,
            refund_fact_map: &refund_fact_map,
            period_start,
            period_end,
        })?;
        let mut lines = Vec::with_capacity(req.lines.len());
        for input in &req.lines {
            let order = order_map
                .get(input.supplier_fulfillment_order_id.as_ref())
                .ok_or_else(|| Error::NotFound("供应商订单不存在".to_string()))?;
            let item = item_map
                .get(input.supplier_fulfillment_item_id.as_ref())
                .ok_or_else(|| Error::NotFound("供应商履约明细不存在".to_string()))?;
            if item.supplier_fulfillment_order_id != input.supplier_fulfillment_order_id {
                return Err(Error::BusinessLogicError(format!(
                    "履约明细 {} 不属于订单 {}",
                    input.supplier_fulfillment_item_id, input.supplier_fulfillment_order_id
                )));
            }
            if order.supplier_id != req.supplier_id {
                return Err(Error::BusinessLogicError(
                    "来源证据包含其他供应商的订单".to_string(),
                ));
            }
            lines.push(build_source_line(
                input,
                order,
                item,
                &refund_allocations,
                &refund_fact_map,
                period_start,
                period_end,
            )?);
        }
        let source_hash = source_evidence_hash(req, &lines);
        SupplierSettlementSourceEvidence::new(
            next_id(),
            SupplierSettlementSourceEvidenceData {
                request_id: req.request_id.clone(),
                supplier_id: req.supplier_id.clone(),
                period_start,
                period_end,
                period_policy_id: req.period_policy_id.clone(),
                period_policy_version: req.period_policy_version.clone(),
                timezone: req.timezone.clone(),
                source_version: req.source_version,
                external_bill_no: req.external_bill_no.clone(),
                external_bill_version: req.external_bill_version.clone(),
                external_bill_evidence_reference_id: req.external_bill_evidence_reference_id.clone(),
                lines,
                source_as_of: Instant::now(),
                recorded_by: actor.id().to_string(),
                source_hash,
                request_hash,
            },
        )
        .map_err(Into::into)
    }
}

fn build_source_line(
    input: &super::RecordSettlementSourceEvidenceLineRequest,
    order: &entities::supplier_fulfillment::SupplierFulfillmentOrder,
    item: &entities::supplier_fulfillment::SupplierFulfillmentItem,
    refund_allocations: &[entities::supplier_fulfillment::SupplierRefundAllocation],
    refund_fact_map: &HashMap<&str, &entities::supplier_fulfillment::SupplierRefundFact>,
    period_start: BusinessDate,
    period_end: BusinessDate,
) -> Result<SupplierSettlementSourceEvidenceLine> {
    let completion_in_period = order
        .completed_at
        .filter(|value| date_in_period(*value, period_start, period_end));
    if completion_in_period.is_some() && order.fulfillment_status != FulfillmentStatus::Completed {
        return Err(Error::BusinessLogicError(
            "履约完成时间与正式订单状态不一致".to_string(),
        ));
    }
    let cancel_at = paired_cancel_evidence(input, order, period_start, period_end)?;
    let mut source_fact_types = Vec::new();
    let mut evidence_reference_ids = input.evidence_reference_ids.clone();
    let (order_gross, order_net, order_tax) = if let Some(completed_at) = completion_in_period {
        source_fact_types.push(SettlementSourceFactType::FulfillmentCompleted);
        evidence_reference_ids.push(format!(
            "supplier-fulfillment://{}/{}/completed/{}",
            order.base.id,
            item.base.id,
            completed_at.unix_secs()
        ));
        line_amounts(item.unit_cost_snapshot_gross, item.quantity, item.input_tax_rate)
    } else {
        (zero(), zero(), zero())
    };
    if let Some((cancel_at, reference)) = cancel_at {
        source_fact_types.push(SettlementSourceFactType::CancelConfirmed);
        evidence_reference_ids.push(reference);
        evidence_reference_ids.push(format!(
            "supplier-cancel://{}/{}/{}",
            order.base.id,
            item.base.id,
            cancel_at.unix_secs()
        ));
    }
    let mut refund_gross = zero();
    let mut refund_net = zero();
    let mut refund_tax = zero();
    for allocation in refund_allocations
        .iter()
        .filter(|allocation| allocation.supplier_fulfillment_item_id == input.supplier_fulfillment_item_id)
    {
        let fact = refund_fact_map
            .get(allocation.supplier_refund_fact_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("退款分配缺少正式退款头".to_string()))?;
        if !date_in_period(fact.refunded_at, period_start, period_end) {
            continue;
        }
        match allocation.allocation_action {
            AllocationAction::Apply => {
                refund_gross = refund_gross.checked_add(allocation.gross_amount);
                refund_net = refund_net.checked_add(allocation.net_amount);
                refund_tax = refund_tax.checked_add(allocation.tax_amount);
            }
            AllocationAction::Reverse => {
                refund_gross = refund_gross.checked_sub(allocation.gross_amount);
                refund_net = refund_net.checked_sub(allocation.net_amount);
                refund_tax = refund_tax.checked_sub(allocation.tax_amount);
            }
        }
        evidence_reference_ids.push(format!(
            "supplier-refund://{}/allocation/{}",
            fact.base.id, allocation.base.id
        ));
    }
    if refund_gross != zero() || refund_net != zero() || refund_tax != zero() {
        source_fact_types.push(SettlementSourceFactType::RefundConfirmed);
    }
    if source_fact_types.is_empty() {
        return Err(Error::BusinessLogicError(format!(
            "履约明细 {} 在结算周期内没有完成、取消或退款正式事实",
            item.base.id
        )));
    }
    let erp_gross = order_gross
        .checked_add(input.freight_gross)
        .checked_add(input.service_fee_gross)
        .checked_sub(refund_gross);
    let erp_net = order_net
        .checked_add(input.freight_net)
        .checked_add(input.service_fee_net)
        .checked_sub(refund_net);
    let erp_tax = order_tax
        .checked_add(input.freight_tax)
        .checked_add(input.service_fee_tax)
        .checked_sub(refund_tax);
    let mut line = SupplierSettlementSourceEvidenceLine {
        supplier_fulfillment_order_id: input.supplier_fulfillment_order_id.clone(),
        supplier_fulfillment_item_id: input.supplier_fulfillment_item_id.clone(),
        quantity: item.quantity,
        source_fact_types,
        evidence_reference_ids,
        order_gross,
        order_net,
        order_tax,
        freight_gross: input.freight_gross,
        freight_net: input.freight_net,
        freight_tax: input.freight_tax,
        service_fee_gross: input.service_fee_gross,
        service_fee_net: input.service_fee_net,
        service_fee_tax: input.service_fee_tax,
        refund_gross,
        refund_net,
        refund_tax,
        erp_gross,
        erp_net,
        erp_tax,
        supplier_billed_gross: input.supplier_billed_gross,
        supplier_billed_net: input.supplier_billed_net,
        supplier_billed_tax: input.supplier_billed_tax,
    };
    line.validate()?;
    Ok(line)
}

fn paired_cancel_evidence(
    input: &super::RecordSettlementSourceEvidenceLineRequest,
    order: &entities::supplier_fulfillment::SupplierFulfillmentOrder,
    period_start: BusinessDate,
    period_end: BusinessDate,
) -> Result<Option<(Instant, String)>> {
    match (
        input.cancel_occurred_at,
        input.cancel_evidence_reference_id.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(at), Some(reference)) => {
            if order.cancel_status != CancelStatus::Canceled {
                return Err(Error::BusinessLogicError(
                    "取消补证与供应商订单取消终态不一致".to_string(),
                ));
            }
            let at = Instant::from_unix_secs(at);
            if !date_in_period(at, period_start, period_end) {
                return Err(Error::ValidationError("取消补证发生时间不在结算期间".to_string()));
            }
            let reference = reference.trim();
            if reference.is_empty() {
                return Err(Error::ValidationError("取消证据引用不能为空".to_string()));
            }
            Ok(Some((at, reference.to_string())))
        }
        _ => Err(Error::ValidationError(
            "取消发生时间与取消证据引用必须同时提供或同时省略".to_string(),
        )),
    }
}

fn ensure_distinct_source_lines(lines: &[super::RecordSettlementSourceEvidenceLineRequest]) -> Result<()> {
    let mut item_ids = HashSet::with_capacity(lines.len());
    for line in lines {
        if !item_ids.insert(line.supplier_fulfillment_item_id.to_string()) {
            return Err(Error::ValidationError(
                "来源证据不得重复同一供应商履约明细".to_string(),
            ));
        }
    }
    Ok(())
}

/// 校验服务端可枚举的完成与退款事实没有被来源命令漏行。
///
/// 取消历史尚无可关联正式表，只能由带正式引用的补证行加入；完成时间与退款分配
/// 已可从 D32 枚举，因此任一对应履约明细缺失都拒绝整批入库。
struct CompleteSourceScope<'a> {
    inputs: &'a [super::RecordSettlementSourceEvidenceLineRequest],
    input_item_ids: &'a HashSet<String>,
    order_map: &'a HashMap<String, entities::supplier_fulfillment::SupplierFulfillmentOrder>,
    item_map: &'a HashMap<String, entities::supplier_fulfillment::SupplierFulfillmentItem>,
    refund_allocations: &'a [entities::supplier_fulfillment::SupplierRefundAllocation],
    refund_fact_map: &'a HashMap<&'a str, &'a entities::supplier_fulfillment::SupplierRefundFact>,
    period_start: BusinessDate,
    period_end: BusinessDate,
}

fn ensure_complete_source_scope(scope: CompleteSourceScope<'_>) -> Result<()> {
    let CompleteSourceScope {
        inputs,
        input_item_ids,
        order_map,
        item_map,
        refund_allocations,
        refund_fact_map,
        period_start,
        period_end,
    } = scope;
    let mut required_item_ids = HashSet::new();
    for item in item_map.values() {
        let order = order_map
            .get(item.supplier_fulfillment_order_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("履约明细缺少供应商订单头".to_string()))?;
        if order
            .completed_at
            .is_some_and(|at| date_in_period(at, period_start, period_end))
        {
            required_item_ids.insert(item.base.id.clone());
        }
    }
    for allocation in refund_allocations {
        let fact = refund_fact_map
            .get(allocation.supplier_refund_fact_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("退款分配缺少正式退款头".to_string()))?;
        if date_in_period(fact.refunded_at, period_start, period_end) {
            required_item_ids.insert(allocation.supplier_fulfillment_item_id.to_string());
        }
    }
    for input in inputs.iter().filter(|input| input.cancel_occurred_at.is_some()) {
        required_item_ids.insert(input.supplier_fulfillment_item_id.to_string());
    }
    let mut missing = required_item_ids
        .difference(input_item_ids)
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    if !missing.is_empty() {
        return Err(Error::BusinessLogicError(format!(
            "SOURCE_EVIDENCE_INCOMPLETE: 来源证据遗漏周期内完成或退款明细 {}",
            missing.into_iter().take(20).collect::<Vec<_>>().join(",")
        )));
    }
    Ok(())
}

fn date_in_period(value: Instant, period_start: BusinessDate, period_end: BusinessDate) -> bool {
    let offset = chrono::FixedOffset::east_opt(8 * 60 * 60).expect("上海时区偏移合法");
    let date = value.as_utc().with_timezone(&offset).date_naive();
    date >= period_start.as_naive_date() && date <= period_end.as_naive_date()
}

fn parse_business_date(value: &str, field: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("{field}不是合法业务日期")))
}

fn zero() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

fn source_request_hash(req: &RecordSettlementSourceEvidenceRequest) -> String {
    let mut parts = vec![
        "supplier-settlement-source-evidence-command-v1".to_string(),
        req.request_id.clone(),
        req.idempotency_key.clone(),
        req.supplier_id.to_string(),
        req.period_start.trim().to_string(),
        req.period_end.trim().to_string(),
        req.period_policy_id.trim().to_string(),
        req.period_policy_version.trim().to_string(),
        req.timezone.trim().to_string(),
        req.source_version.to_string(),
        req.external_bill_no.trim().to_string(),
        req.external_bill_version.trim().to_string(),
        req.external_bill_evidence_reference_id.trim().to_string(),
    ];
    let mut lines = req.lines.iter().collect::<Vec<_>>();
    lines.sort_by(|left, right| {
        left.supplier_fulfillment_item_id
            .as_ref()
            .cmp(right.supplier_fulfillment_item_id.as_ref())
    });
    for line in lines {
        parts.extend([
            line.supplier_fulfillment_order_id.to_string(),
            line.supplier_fulfillment_item_id.to_string(),
            line.cancel_occurred_at
                .map(|value| value.to_string())
                .unwrap_or_default(),
            line.cancel_evidence_reference_id.clone().unwrap_or_default(),
            line.freight_gross.to_string(),
            line.freight_net.to_string(),
            line.freight_tax.to_string(),
            line.service_fee_gross.to_string(),
            line.service_fee_net.to_string(),
            line.service_fee_tax.to_string(),
            line.supplier_billed_gross.to_string(),
            line.supplier_billed_net.to_string(),
            line.supplier_billed_tax.to_string(),
        ]);
        let mut references = line.evidence_reference_ids.clone();
        references.sort();
        parts.push(references.join(","));
    }
    digest_parts(&parts)
}

fn source_evidence_hash(
    req: &RecordSettlementSourceEvidenceRequest,
    lines: &[SupplierSettlementSourceEvidenceLine],
) -> String {
    let mut parts = vec![
        "supplier-settlement-authoritative-source-v1".to_string(),
        req.supplier_id.to_string(),
        req.period_start.trim().to_string(),
        req.period_end.trim().to_string(),
        req.period_policy_id.trim().to_string(),
        req.period_policy_version.trim().to_string(),
        req.timezone.trim().to_string(),
        req.source_version.to_string(),
        req.external_bill_no.trim().to_string(),
        req.external_bill_version.trim().to_string(),
        req.external_bill_evidence_reference_id.trim().to_string(),
    ];
    let mut lines = lines.iter().collect::<Vec<_>>();
    lines.sort_by(|left, right| {
        left.supplier_fulfillment_item_id
            .as_ref()
            .cmp(right.supplier_fulfillment_item_id.as_ref())
    });
    for line in lines {
        parts.extend([
            line.supplier_fulfillment_order_id.to_string(),
            line.supplier_fulfillment_item_id.to_string(),
            line.quantity.to_string(),
            line.source_fact_types
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>()
                .join(","),
            line.evidence_reference_ids.join(","),
            line.order_gross.to_string(),
            line.order_net.to_string(),
            line.order_tax.to_string(),
            line.freight_gross.to_string(),
            line.freight_net.to_string(),
            line.freight_tax.to_string(),
            line.service_fee_gross.to_string(),
            line.service_fee_net.to_string(),
            line.service_fee_tax.to_string(),
            line.refund_gross.to_string(),
            line.refund_net.to_string(),
            line.refund_tax.to_string(),
            line.erp_gross.to_string(),
            line.erp_net.to_string(),
            line.erp_tax.to_string(),
            line.supplier_billed_gross.to_string(),
            line.supplier_billed_net.to_string(),
            line.supplier_billed_tax.to_string(),
        ]);
    }
    digest_parts(&parts)
}
