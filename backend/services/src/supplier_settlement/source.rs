//! W27 不可变来源证据录入与 D32 正式事实派生。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{AccessControlExt, NoTransaction, SupplierSettlementExt, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::money::{line_amounts, Amount};
use entities::supplier_fulfillment::AllocationAction;
use entities::supplier_settlement::{
    SettlementAmountComponents, SettlementCancelEvidence, SettlementPeriod, SettlementSourceFactType,
    SupplierSettlementSourceEvidence, SupplierSettlementSourceEvidenceData,
    SupplierSettlementSourceEvidenceLine, SupplierSettlementSourceEvidenceLineData, SETTLEMENT_TIMEZONE,
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
    ///
    /// # 参数
    /// * `query` - 供应商与结算期间查询条件
    ///
    /// # 返回
    /// 返回最新不可变来源证据批次的概要视图。
    ///
    /// # 错误
    /// 日期或期间非法、仓储查询失败或当前范围没有来源证据时返回错误。
    pub async fn latest_source_evidence(
        &self,
        query: &SupplierSettlementSourceEvidenceQuery,
    ) -> Result<SupplierSettlementSourceEvidenceView> {
        query.validate()?;
        let period = SettlementPeriod::new(
            parse_business_date(&query.period_start, "结算期间开始")?,
            parse_business_date(&query.period_end, "结算期间结束")?,
            SETTLEMENT_TIMEZONE,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        self.db
            .supplier_settlement_source_evidence()
            .latest_for_scope(
                &query.supplier_id,
                period.start(),
                period.end(),
                &mut NoTransaction,
            )
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
    /// # 参数
    /// * `req` - 客户端来源证据命令
    /// * `actor` - 已鉴权记录人
    ///
    /// # 返回
    /// 返回新建或幂等恢复的不可变来源证据概要。
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
            if !existing.matches_request_hash(&request_hash) {
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
                if !existing.matches_request_hash(&request_hash) {
                    return Err(Error::ConflictError("来源证据请求ID已用于不同命令".to_string()));
                }
                return Ok(existing.into());
            }
            return Err(error);
        }
        Ok(evidence.into())
    }

    /// 从已验证命令和正式履约/退款事实构建不可变来源证据批次。
    ///
    /// # 参数
    /// * `req` - 来源证据命令
    /// * `actor` - 已鉴权记录人
    /// * `request_hash` - 当前命令的稳定幂等指纹
    ///
    /// # 返回
    /// 返回可在事务中直接持久化的完整来源证据实体。
    ///
    /// # 错误
    /// 周期/版本非法、跨域事实缺失、范围不完整、关系不一致或金额构造失败时返回错误。
    async fn build_source_evidence(
        &self,
        req: &RecordSettlementSourceEvidenceRequest,
        actor: &AuditActor,
        request_hash: String,
    ) -> Result<SupplierSettlementSourceEvidence> {
        let period = SettlementPeriod::new(
            parse_business_date(&req.period_start, "结算期间开始")?,
            parse_business_date(&req.period_end, "结算期间结束")?,
            &req.timezone,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        let input_item_ids = req
            .lines
            .iter()
            .map(|line| line.supplier_fulfillment_item_id.clone())
            .collect::<Vec<_>>();
        SupplierSettlementSourceEvidence::ensure_unique_item_ids(&input_item_ids)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        if let Some(latest) = self
            .db
            .supplier_settlement_source_evidence()
            .latest_for_period(
                &req.supplier_id,
                period.start(),
                period.end(),
                req.period_policy_id.trim(),
                req.period_policy_version.trim(),
                &mut NoTransaction,
            )
            .await?
        {
            latest
                .ensure_newer_source_version(req.source_version)
                .map_err(|error| Error::ConflictError(error.to_string()))?;
        }
        let item_ids = req
            .lines
            .iter()
            .map(|line| line.supplier_fulfillment_item_id.to_string())
            .collect::<HashSet<_>>();
        let scope = self
            .db
            .supplier_settlement()
            .settlement_source_scope(
                &req.supplier_id,
                period.start(),
                period.end(),
                &req.lines
                    .iter()
                    .map(|line| line.supplier_fulfillment_order_id.clone())
                    .collect::<Vec<_>>(),
                &input_item_ids,
                &mut NoTransaction,
            )
            .await?;
        if scope.orders.is_empty() {
            return Err(Error::NotFound(
                "当前供应商没有可核验的供应商履约订单".to_string(),
            ));
        }
        let order_map = scope
            .orders
            .into_iter()
            .map(|value| (value.base.id.clone(), value))
            .collect::<HashMap<_, _>>();
        let item_map = scope
            .items
            .into_iter()
            .map(|value| (value.base.id.clone(), value))
            .collect::<HashMap<_, _>>();
        let refund_allocations = scope.refund_allocations;
        let refund_fact_map = scope
            .refund_facts
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
            period,
        })?;
        let mut lines = Vec::with_capacity(req.lines.len());
        for input in &req.lines {
            let order = order_map
                .get(input.supplier_fulfillment_order_id.as_ref())
                .ok_or_else(|| Error::NotFound("供应商订单不存在".to_string()))?;
            let item = item_map
                .get(input.supplier_fulfillment_item_id.as_ref())
                .ok_or_else(|| Error::NotFound("供应商履约明细不存在".to_string()))?;
            if !item.belongs_to_order(&input.supplier_fulfillment_order_id) {
                return Err(Error::BusinessLogicError(format!(
                    "履约明细 {} 不属于订单 {}",
                    input.supplier_fulfillment_item_id, input.supplier_fulfillment_order_id
                )));
            }
            if !order.belongs_to_supplier(&req.supplier_id) {
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
                period,
            )?);
        }
        let mut data = SupplierSettlementSourceEvidenceData {
            request_id: req.request_id.clone(),
            supplier_id: req.supplier_id.clone(),
            period_start: period.start(),
            period_end: period.end(),
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
            source_hash: String::new(),
            request_hash,
        };
        data.source_hash = data.canonical_source_hash();
        SupplierSettlementSourceEvidence::new(next_id(), data).map_err(Into::into)
    }
}

/// 由一条命令输入及已查询的正式事实构建冻结来源行。
///
/// # 参数
/// * `input` - 当前来源命令行
/// * `order` - 已加载的供应商履约订单
/// * `item` - 已加载的供应商履约明细
/// * `refund_allocations` - 供应商订单范围内的退款分配
/// * `refund_fact_map` - 退款事实头索引
/// * `period` - 当前结算期间
///
/// # 返回
/// 返回由实体工厂派生 ERP 金额并规范化证据的冻结来源行。
///
/// # 错误
/// 订单状态、取消证据、退款关系、期间归属或金额恒等不一致时返回错误。
fn build_source_line(
    input: &super::RecordSettlementSourceEvidenceLineRequest,
    order: &entities::supplier_fulfillment::SupplierFulfillmentOrder,
    item: &entities::supplier_fulfillment::SupplierFulfillmentItem,
    refund_allocations: &[entities::supplier_fulfillment::SupplierRefundAllocation],
    refund_fact_map: &HashMap<&str, &entities::supplier_fulfillment::SupplierRefundFact>,
    period: SettlementPeriod,
) -> Result<SupplierSettlementSourceEvidenceLine> {
    let completion_in_period = order
        .confirmed_completed_at()?
        .filter(|completed_at| period.contains(*completed_at));
    let cancel_evidence = SettlementCancelEvidence::from_optional(
        input.cancel_occurred_at.map(Instant::from_unix_secs),
        input.cancel_evidence_reference_id.clone(),
        period,
    )?;
    if cancel_evidence.is_some() {
        order.ensure_canceled()?;
    }
    let mut source_fact_types = Vec::new();
    let mut evidence_reference_ids = input.evidence_reference_ids.clone();
    let order_amounts = if let Some(completed_at) = completion_in_period {
        source_fact_types.push(SettlementSourceFactType::FulfillmentCompleted);
        evidence_reference_ids.push(format!(
            "supplier-fulfillment://{}/{}/completed/{}",
            order.base.id,
            item.base.id,
            completed_at.unix_secs()
        ));
        let (gross, net, tax) =
            line_amounts(item.unit_cost_snapshot_gross, item.quantity, item.input_tax_rate);
        SettlementAmountComponents::new(gross, net, tax, "订单金额")?
    } else {
        SettlementAmountComponents::zero()
    };
    if let Some(cancel_evidence) = cancel_evidence {
        source_fact_types.push(SettlementSourceFactType::CancelConfirmed);
        evidence_reference_ids.push(cancel_evidence.reference_id().to_string());
        evidence_reference_ids.push(format!(
            "supplier-cancel://{}/{}/{}",
            order.base.id,
            item.base.id,
            cancel_evidence.occurred_at().unix_secs()
        ));
    }
    let refund = refund_amounts(
        input,
        refund_allocations,
        refund_fact_map,
        period,
        &mut evidence_reference_ids,
    )?;
    if refund != SettlementAmountComponents::zero() {
        source_fact_types.push(SettlementSourceFactType::RefundConfirmed);
    }
    if source_fact_types.is_empty() {
        return Err(Error::BusinessLogicError(format!(
            "履约明细 {} 在结算周期内没有完成、取消或退款正式事实",
            item.base.id
        )));
    }
    SupplierSettlementSourceEvidenceLine::from_components(SupplierSettlementSourceEvidenceLineData {
        supplier_fulfillment_order_id: input.supplier_fulfillment_order_id.clone(),
        supplier_fulfillment_item_id: input.supplier_fulfillment_item_id.clone(),
        quantity: item.quantity,
        source_fact_types,
        evidence_reference_ids,
        order: order_amounts,
        freight: SettlementAmountComponents::new(
            input.freight_gross,
            input.freight_net,
            input.freight_tax,
            "运费金额",
        )?,
        service_fee: SettlementAmountComponents::new(
            input.service_fee_gross,
            input.service_fee_net,
            input.service_fee_tax,
            "服务费金额",
        )?,
        refund,
        supplier_billed: SettlementAmountComponents::new(
            input.supplier_billed_gross,
            input.supplier_billed_net,
            input.supplier_billed_tax,
            "供应商账单金额",
        )?,
    })
    .map_err(Into::into)
}

/// 汇总当前履约明细在结算期间内的退款分配金额与证据。
///
/// # 参数
/// * `input` - 当前来源命令行
/// * `refund_allocations` - 供应商订单范围内的全部退款分配
/// * `refund_fact_map` - 退款事实头索引
/// * `period` - 当前结算期间
/// * `evidence_reference_ids` - 待追加退款证据引用的集合
///
/// # 返回
/// 返回 APPLY 减 REVERSE 后的非负退款金额三元组。
///
/// # 错误
/// 分配缺少退款头、净退款为负或金额三元组不一致时返回错误。
fn refund_amounts(
    input: &super::RecordSettlementSourceEvidenceLineRequest,
    refund_allocations: &[entities::supplier_fulfillment::SupplierRefundAllocation],
    refund_fact_map: &HashMap<&str, &entities::supplier_fulfillment::SupplierRefundFact>,
    period: SettlementPeriod,
    evidence_reference_ids: &mut Vec<String>,
) -> Result<SettlementAmountComponents> {
    let mut gross = zero();
    let mut net = zero();
    let mut tax = zero();
    for allocation in refund_allocations
        .iter()
        .filter(|allocation| allocation.supplier_fulfillment_item_id == input.supplier_fulfillment_item_id)
    {
        let fact = refund_fact_map
            .get(allocation.supplier_refund_fact_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("退款分配缺少正式退款头".to_string()))?;
        if !period.contains(fact.refunded_at) {
            continue;
        }
        match allocation.allocation_action {
            AllocationAction::Apply => {
                gross = gross.checked_add(allocation.gross_amount);
                net = net.checked_add(allocation.net_amount);
                tax = tax.checked_add(allocation.tax_amount);
            }
            AllocationAction::Reverse => {
                gross = gross.checked_sub(allocation.gross_amount);
                net = net.checked_sub(allocation.net_amount);
                tax = tax.checked_sub(allocation.tax_amount);
            }
        }
        evidence_reference_ids.push(format!(
            "supplier-refund://{}/allocation/{}",
            fact.base.id, allocation.base.id
        ));
    }
    SettlementAmountComponents::new(gross, net, tax, "退款金额").map_err(Into::into)
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
    period: SettlementPeriod,
}

/// 校验来源命令完整覆盖服务端可枚举的期间内正式事实。
///
/// # 参数
/// * `scope` - 命令输入、履约事实、退款事实与结算期间的只读集合
///
/// # 返回
/// 所有期间内完成、退款和显式取消明细均被命令覆盖时返回 `Ok(())`。
///
/// # 错误
/// 明细缺少订单头、退款分配缺少事实头或命令漏行时返回错误。
fn ensure_complete_source_scope(scope: CompleteSourceScope<'_>) -> Result<()> {
    let CompleteSourceScope {
        inputs,
        input_item_ids,
        order_map,
        item_map,
        refund_allocations,
        refund_fact_map,
        period,
    } = scope;
    let mut required_item_ids = HashSet::new();
    for item in item_map.values() {
        let order = order_map
            .get(item.supplier_fulfillment_order_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("履约明细缺少供应商订单头".to_string()))?;
        if order
            .confirmed_completed_at()?
            .is_some_and(|at| period.contains(at))
        {
            required_item_ids.insert(item.base.id.clone());
        }
    }
    for allocation in refund_allocations {
        let fact = refund_fact_map
            .get(allocation.supplier_refund_fact_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("退款分配缺少正式退款头".to_string()))?;
        if period.contains(fact.refunded_at) {
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

/// 解析客户端业务日期文本。
///
/// # 参数
/// * `value` - ISO 业务日期文本
/// * `field` - 参数校验错误使用的字段名称
///
/// # 返回
/// 返回强类型业务日期。
///
/// # 错误
/// 日期格式非法时返回 `ValidationError`。
fn parse_business_date(value: &str, field: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("{field}不是合法业务日期")))
}

/// 返回来源金额汇总使用的零金额。
///
/// # 返回
/// 返回精确到分的零金额。
fn zero() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 计算来源证据命令的稳定幂等指纹。
///
/// # 参数
/// * `req` - 客户端来源证据命令
///
/// # 返回
/// 返回对行和证据输入顺序稳定的 SHA-256 指纹。
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

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::str::FromStr;

    use entities::common::time::{BusinessDate, Instant};
    use entities::ids::{
        CostAllocationId, CostEntryId, InboxMessageId, MallOrderId, MallOrderItemId, PayableEntryId,
        SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierOfferingRevisionId, SupplierRefundAllocationId, SupplierRefundFactId,
    };
    use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
    use entities::supplier_fulfillment::{
        AllocationAction, FulfillmentStatus, SupplierFulfillmentItem, SupplierFulfillmentItemData,
        SupplierFulfillmentOrder, SupplierFulfillmentOrderData, SupplierRefundAllocation,
        SupplierRefundAllocationData, SupplierRefundFact, SupplierRefundFactData,
    };
    use entities::supplier_settlement::{
        SettlementAmountComponents, SettlementCancelEvidence, SettlementPeriod, SettlementSourceFactType,
        SETTLEMENT_TIMEZONE,
    };

    use super::{build_source_line, ensure_complete_source_scope, refund_amounts, CompleteSourceScope};
    use crate::errors::{Error, Result};
    use crate::supplier_settlement::RecordSettlementSourceEvidenceLineRequest;

    const SUPPLIER: &str = "supplier-1";
    const PERIOD_START: &str = "2026-07-01";
    const PERIOD_END: &str = "2026-07-31";

    fn instant(rfc3339: &str) -> Instant {
        Instant::from_unix_secs(chrono::DateTime::parse_from_rfc3339(rfc3339).unwrap().timestamp())
    }

    fn period() -> SettlementPeriod {
        SettlementPeriod::new(
            BusinessDate::from_str(PERIOD_START).unwrap(),
            BusinessDate::from_str(PERIOD_END).unwrap(),
            SETTLEMENT_TIMEZONE,
        )
        .unwrap()
    }

    fn completed_order(id: &str, completed_at: Instant) -> SupplierFulfillmentOrder {
        let mut order = SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new(id),
            SupplierFulfillmentOrderData::submitting(
                format!("SO-{id}"),
                MallOrderId::new(format!("mall-{id}")),
                SupplierAccountId::new(SUPPLIER),
                SupplierApiConnectionId::new("connection-1"),
                1,
                Instant::from_unix_secs(1_700_000_000),
                "encrypted-address",
                "fingerprint-address",
            ),
        )
        .expect("订单构造失败");
        order
            .advance_fulfillment(FulfillmentStatus::Accepted)
            .expect("迁移失败");
        order
            .advance_fulfillment(FulfillmentStatus::Fulfilling)
            .expect("迁移失败");
        order
            .advance_fulfillment(FulfillmentStatus::Completed)
            .expect("迁移失败");
        order.completed_at = Some(completed_at);
        order
    }

    fn fulfillment_item(id: &str, order_id: &SupplierFulfillmentOrderId) -> SupplierFulfillmentItem {
        let unit_price = UnitPrice::from_str("113.0000").unwrap();
        let quantity = Quantity::from_str("1.000000").unwrap();
        let (gross, _, _) = line_amounts(unit_price, quantity, Rate::from_str("0.130000").unwrap());
        SupplierFulfillmentItem::new(
            SupplierFulfillmentItemId::new(id),
            SupplierFulfillmentItemData {
                supplier_fulfillment_order_id: order_id.clone(),
                mall_order_item_id: MallOrderItemId::new(format!("mall-item-{id}")),
                supplier_offering_revision_id: SupplierOfferingRevisionId::new("offering-rev-1"),
                supplier_sku_code_snapshot: format!("SKU-{id}"),
                supplier_product_code_snapshot: None,
                quantity,
                unit_cost_snapshot_gross: unit_price,
                cost_snapshot_total_gross: gross,
                input_tax_rate: Rate::from_str("0.130000").unwrap(),
            },
        )
        .expect("明细构造失败")
    }

    fn refund_fact(
        id: &str,
        order_id: &SupplierFulfillmentOrderId,
        refunded_at: Instant,
    ) -> SupplierRefundFact {
        SupplierRefundFact::new(
            SupplierRefundFactId::new(id),
            SupplierRefundFactData {
                supplier_id: SupplierAccountId::new(SUPPLIER),
                connection_id: SupplierApiConnectionId::new("connection-1"),
                supplier_fulfillment_order_id: order_id.clone(),
                external_refund_no: format!("RF-{id}"),
                external_refund_version: "1".to_string(),
                refund_amount: Amount::from_str("10.00").unwrap(),
                refunded_at,
                source_event_id: format!("event-{id}"),
                inbox_message_id: InboxMessageId::new(format!("inbox-{id}")),
            },
        )
        .expect("退款事实构造失败")
    }

    #[allow(clippy::too_many_arguments)]
    fn refund_allocation(
        id: &str,
        fact_id: &SupplierRefundFactId,
        item_id: &SupplierFulfillmentItemId,
        gross: &str,
        net: &str,
        tax: &str,
        action: AllocationAction,
        reverses_allocation_id: Option<SupplierRefundAllocationId>,
    ) -> SupplierRefundAllocation {
        SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new(id),
            SupplierRefundAllocationData {
                supplier_refund_fact_id: fact_id.clone(),
                allocation_no: 1,
                supplier_fulfillment_item_id: item_id.clone(),
                original_cost_entry_id: CostEntryId::new(format!("cost-{id}")),
                original_cost_allocation_id: CostAllocationId::new(format!("cost-alloc-{id}")),
                original_payable_entry_id: PayableEntryId::new(format!("payable-{id}")),
                original_payment_allocation_id: None,
                refund_quantity: Quantity::from_str("1.000000").unwrap(),
                gross_amount: Amount::from_str(gross).unwrap(),
                net_amount: Amount::from_str(net).unwrap(),
                tax_amount: Amount::from_str(tax).unwrap(),
                payable_reduction_amount: Amount::from_str(gross).unwrap(),
                cash_refund_amount: Amount::from_str("0.00").unwrap(),
                cash_supplier_refund_id: None,
                allocation_action: action,
                reverses_allocation_id,
            },
        )
        .expect("退款分配构造失败")
    }

    fn line_request(item_id: &str, order_id: &str) -> RecordSettlementSourceEvidenceLineRequest {
        RecordSettlementSourceEvidenceLineRequest {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(order_id),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(item_id),
            cancel_occurred_at: None,
            cancel_evidence_reference_id: None,
            evidence_reference_ids: vec!["bill://line".to_string()],
            freight_gross: Amount::from_str("0.00").unwrap(),
            freight_net: Amount::from_str("0.00").unwrap(),
            freight_tax: Amount::from_str("0.00").unwrap(),
            service_fee_gross: Amount::from_str("0.00").unwrap(),
            service_fee_net: Amount::from_str("0.00").unwrap(),
            service_fee_tax: Amount::from_str("0.00").unwrap(),
            supplier_billed_gross: Amount::from_str("113.00").unwrap(),
            supplier_billed_net: Amount::from_str("98.31").unwrap(),
            supplier_billed_tax: Amount::from_str("14.69").unwrap(),
        }
    }

    fn order_map(orders: &[SupplierFulfillmentOrder]) -> HashMap<String, SupplierFulfillmentOrder> {
        orders
            .iter()
            .map(|order| (order.base.id.clone(), order.clone()))
            .collect()
    }

    fn item_map(items: &[SupplierFulfillmentItem]) -> HashMap<String, SupplierFulfillmentItem> {
        items
            .iter()
            .map(|item| (item.base.id.clone(), item.clone()))
            .collect()
    }

    fn fact_map(facts: &[SupplierRefundFact]) -> HashMap<&str, &SupplierRefundFact> {
        facts.iter().map(|fact| (fact.base.id.as_str(), fact)).collect()
    }

    fn input_item_ids(inputs: &[RecordSettlementSourceEvidenceLineRequest]) -> HashSet<String> {
        inputs
            .iter()
            .map(|line| line.supplier_fulfillment_item_id.to_string())
            .collect()
    }

    fn ensure_scope(
        inputs: Vec<RecordSettlementSourceEvidenceLineRequest>,
        orders: Vec<SupplierFulfillmentOrder>,
        items: Vec<SupplierFulfillmentItem>,
        refund_allocations: Vec<SupplierRefundAllocation>,
        refund_facts: Vec<SupplierRefundFact>,
    ) -> Result<()> {
        let input_item_ids = input_item_ids(&inputs);
        let order_map = order_map(&orders);
        let item_map = item_map(&items);
        let refund_fact_map = fact_map(&refund_facts);
        ensure_complete_source_scope(CompleteSourceScope {
            inputs: &inputs,
            input_item_ids: &input_item_ids,
            order_map: &order_map,
            item_map: &item_map,
            refund_allocations: &refund_allocations,
            refund_fact_map: &refund_fact_map,
            period: period(),
        })
    }

    fn business_logic_message(error: Error) -> String {
        match error {
            Error::BusinessLogicError(message) => message,
            other => panic!("期望 BusinessLogicError，得到 {other:?}"),
        }
    }

    #[test]
    fn complete_scope_accepts_all_in_period_facts() {
        let order = completed_order("order-1", instant("2026-07-15T12:00:00+08:00"));
        let item = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let fact = refund_fact(
            "fact-1",
            &SupplierFulfillmentOrderId::new("order-1"),
            instant("2026-07-20T10:00:00+08:00"),
        );
        let allocation = refund_allocation(
            "alloc-1",
            &SupplierRefundFactId::new("fact-1"),
            &SupplierFulfillmentItemId::new("item-1"),
            "10.00",
            "8.70",
            "1.30",
            AllocationAction::Apply,
            None,
        );
        assert!(ensure_scope(
            vec![line_request("item-1", "order-1")],
            vec![order],
            vec![item],
            vec![allocation],
            vec![fact],
        )
        .is_ok());
    }

    #[test]
    fn complete_scope_rejects_omitted_in_period_completed_item() {
        let order = completed_order("order-1", instant("2026-07-15T12:00:00+08:00"));
        let item_1 = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let item_2 = fulfillment_item("item-2", &SupplierFulfillmentOrderId::new("order-1"));
        let message = business_logic_message(
            ensure_scope(
                vec![line_request("item-1", "order-1")],
                vec![order],
                vec![item_1, item_2],
                Vec::new(),
                Vec::new(),
            )
            .expect_err("漏掉期间内完成明细必须拒绝"),
        );
        assert!(
            message.contains("SOURCE_EVIDENCE_INCOMPLETE") && message.contains("item-2"),
            "错误必须 fail-closed 并列出遗漏明细: {message}"
        );
    }

    #[test]
    fn complete_scope_rejects_omitted_in_period_refund_allocation() {
        let order = completed_order("order-1", instant("2026-07-15T12:00:00+08:00"));
        let item_1 = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let fact = refund_fact(
            "fact-1",
            &SupplierFulfillmentOrderId::new("order-1"),
            instant("2026-07-20T10:00:00+08:00"),
        );
        // 期间内退款分配指向 item-2，但命令只覆盖 item-1 → 完整性检查必须拒绝。
        let allocation = refund_allocation(
            "alloc-1",
            &SupplierRefundFactId::new("fact-1"),
            &SupplierFulfillmentItemId::new("item-2"),
            "10.00",
            "8.70",
            "1.30",
            AllocationAction::Apply,
            None,
        );
        let message = business_logic_message(
            ensure_scope(
                vec![line_request("item-1", "order-1")],
                vec![order],
                vec![item_1],
                vec![allocation],
                vec![fact],
            )
            .expect_err("漏掉期间内退款分配对应明细必须拒绝"),
        );
        assert!(
            message.contains("SOURCE_EVIDENCE_INCOMPLETE") && message.contains("item-2"),
            "错误必须 fail-closed 并列出遗漏明细: {message}"
        );
    }

    #[test]
    fn complete_scope_rejects_allocation_missing_fact_head() {
        let order = completed_order("order-1", instant("2026-07-15T12:00:00+08:00"));
        let item_1 = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let allocation = refund_allocation(
            "alloc-1",
            &SupplierRefundFactId::new("fact-missing"),
            &SupplierFulfillmentItemId::new("item-1"),
            "10.00",
            "8.70",
            "1.30",
            AllocationAction::Apply,
            None,
        );
        let message = business_logic_message(
            ensure_scope(
                vec![line_request("item-1", "order-1")],
                vec![order],
                vec![item_1],
                vec![allocation],
                Vec::new(),
            )
            .expect_err("分配缺少退款头必须拒绝"),
        );
        assert!(
            message.contains("退款分配缺少正式退款头"),
            "错误必须指向缺失的退款头: {message}"
        );
    }

    #[test]
    fn complete_scope_rejects_item_without_order_head() {
        let item = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-missing"));
        let message = business_logic_message(
            ensure_scope(
                vec![line_request("item-1", "order-missing")],
                Vec::new(),
                vec![item],
                Vec::new(),
                Vec::new(),
            )
            .expect_err("明细缺少订单头必须拒绝"),
        );
        assert!(
            message.contains("履约明细缺少供应商订单头"),
            "错误必须指向缺失的订单头: {message}"
        );
    }

    #[test]
    fn refund_amounts_net_apply_and_reverse_within_period() {
        let input = line_request("item-1", "order-1");
        let fact = refund_fact(
            "fact-1",
            &SupplierFulfillmentOrderId::new("order-1"),
            instant("2026-07-20T10:00:00+08:00"),
        );
        let allocations = vec![
            refund_allocation(
                "alloc-1",
                &SupplierRefundFactId::new("fact-1"),
                &SupplierFulfillmentItemId::new("item-1"),
                "10.00",
                "8.70",
                "1.30",
                AllocationAction::Apply,
                None,
            ),
            refund_allocation(
                "alloc-2",
                &SupplierRefundFactId::new("fact-1"),
                &SupplierFulfillmentItemId::new("item-1"),
                "4.00",
                "3.48",
                "0.52",
                AllocationAction::Reverse,
                Some(SupplierRefundAllocationId::new("alloc-1")),
            ),
        ];
        let facts = [fact];
        let fact_map = fact_map(&facts);
        let mut references = vec!["bill://line".to_string()];
        let refund = refund_amounts(&input, &allocations, &fact_map, period(), &mut references).unwrap();
        assert_eq!(refund.gross, Amount::from_str("6.00").unwrap());
        assert_eq!(refund.net, Amount::from_str("5.22").unwrap());
        assert_eq!(refund.tax, Amount::from_str("0.78").unwrap());
        assert_eq!(
            references,
            vec![
                "bill://line".to_string(),
                "supplier-refund://fact-1/allocation/alloc-1".to_string(),
                "supplier-refund://fact-1/allocation/alloc-2".to_string(),
            ],
            "退款证据引用必须按分配追加"
        );
    }

    #[test]
    fn refund_amounts_skip_out_of_period_facts_and_other_items() {
        let input = line_request("item-1", "order-1");
        let out_fact = refund_fact(
            "fact-out",
            &SupplierFulfillmentOrderId::new("order-1"),
            instant("2026-08-01T00:00:00+08:00"),
        );
        let other_fact = refund_fact(
            "fact-other",
            &SupplierFulfillmentOrderId::new("order-1"),
            instant("2026-07-20T10:00:00+08:00"),
        );
        let allocations = vec![
            refund_allocation(
                "alloc-out",
                &SupplierRefundFactId::new("fact-out"),
                &SupplierFulfillmentItemId::new("item-1"),
                "10.00",
                "8.70",
                "1.30",
                AllocationAction::Apply,
                None,
            ),
            refund_allocation(
                "alloc-other",
                &SupplierRefundFactId::new("fact-other"),
                &SupplierFulfillmentItemId::new("item-9"),
                "10.00",
                "8.70",
                "1.30",
                AllocationAction::Apply,
                None,
            ),
        ];
        let facts = [out_fact, other_fact];
        let fact_map = fact_map(&facts);
        let mut references = Vec::new();
        let refund = refund_amounts(&input, &allocations, &fact_map, period(), &mut references).unwrap();
        assert_eq!(
            refund,
            SettlementAmountComponents::zero(),
            "期外与异明细分配不计入"
        );
        assert!(references.is_empty());
    }

    #[test]
    fn refund_amounts_reject_allocation_without_fact_head() {
        let input = line_request("item-1", "order-1");
        let allocation = refund_allocation(
            "alloc-1",
            &SupplierRefundFactId::new("fact-missing"),
            &SupplierFulfillmentItemId::new("item-1"),
            "10.00",
            "8.70",
            "1.30",
            AllocationAction::Apply,
            None,
        );
        let fact_map = HashMap::new();
        let mut references = Vec::new();
        let message = business_logic_message(
            refund_amounts(&input, &[allocation], &fact_map, period(), &mut references)
                .expect_err("分配缺少退款头必须拒绝"),
        );
        assert!(message.contains("退款分配缺少正式退款头"));
    }

    #[test]
    fn build_source_line_derives_completed_order_amounts_and_references() {
        let order = completed_order("order-1", instant("2026-07-15T12:00:00+08:00"));
        let item = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let input = line_request("item-1", "order-1");
        let line = build_source_line(&input, &order, &item, &[], &HashMap::new(), period())
            .expect("期间内完成行必须构造成功");
        let (gross, net, tax) =
            line_amounts(item.unit_cost_snapshot_gross, item.quantity, item.input_tax_rate);
        assert!(line
            .source_fact_types
            .contains(&SettlementSourceFactType::FulfillmentCompleted));
        assert_eq!(line.order_gross, gross);
        assert_eq!(line.order_net, net);
        assert_eq!(line.order_tax, tax);
        assert_eq!(line.refund_gross, Amount::from_str("0.00").unwrap());
        assert_eq!(line.refund_net, Amount::from_str("0.00").unwrap());
        assert_eq!(line.refund_tax, Amount::from_str("0.00").unwrap());
        assert!(line.evidence_reference_ids.iter().any(|reference| {
            reference
                == &format!(
                    "supplier-fulfillment://order-1/item-1/completed/{}",
                    order.confirmed_completed_at().unwrap().unwrap().unix_secs()
                )
        }));
        assert_eq!(line.supplier_billed_gross, Amount::from_str("113.00").unwrap());
    }

    #[test]
    fn build_source_line_rejects_cancel_evidence_without_canceled_order() {
        let order = completed_order("order-1", instant("2026-07-15T12:00:00+08:00"));
        let item = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let mut input = line_request("item-1", "order-1");
        input.cancel_occurred_at = Some(instant("2026-07-20T10:00:00+08:00").unix_secs());
        input.cancel_evidence_reference_id = Some("cancel://proof-1".to_string());
        let error = build_source_line(&input, &order, &item, &[], &HashMap::new(), period())
            .expect_err("未取消订单携带取消补证必须拒绝");
        assert!(
            error.to_string().contains("取消"),
            "错误必须指向取消状态: {error}"
        );
    }

    #[test]
    fn build_source_line_rejects_item_without_in_period_fact() {
        let order = completed_order("order-1", instant("2026-06-01T00:00:00+08:00"));
        let item = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let input = line_request("item-1", "order-1");
        let error = build_source_line(&input, &order, &item, &[], &HashMap::new(), period())
            .expect_err("期间内没有任何正式事实的行必须拒绝");
        assert!(
            error.to_string().contains("没有完成、取消或退款正式事实"),
            "错误必须指向缺失正式事实: {error}"
        );
    }

    #[test]
    fn build_source_line_accepts_cancel_evidence_for_canceled_order() {
        let mut order = completed_order("order-1", instant("2026-06-01T00:00:00+08:00"));
        order
            .advance_cancel(entities::supplier_fulfillment::CancelStatus::CancelPending)
            .expect("迁移失败");
        order
            .advance_cancel(entities::supplier_fulfillment::CancelStatus::Canceled)
            .expect("迁移失败");
        let item = fulfillment_item("item-1", &SupplierFulfillmentOrderId::new("order-1"));
        let mut input = line_request("item-1", "order-1");
        input.cancel_occurred_at = Some(instant("2026-07-20T10:00:00+08:00").unix_secs());
        input.cancel_evidence_reference_id = Some("cancel://proof-1".to_string());
        let line = build_source_line(&input, &order, &item, &[], &HashMap::new(), period())
            .expect("已取消订单携带期间内取消补证必须构造成功");
        assert!(line
            .source_fact_types
            .contains(&SettlementSourceFactType::CancelConfirmed));
        let cancel = SettlementCancelEvidence::from_optional(
            input.cancel_occurred_at.map(Instant::from_unix_secs),
            input.cancel_evidence_reference_id.clone(),
            period(),
        )
        .unwrap()
        .unwrap();
        assert!(line
            .evidence_reference_ids
            .iter()
            .any(|reference| reference == cancel.reference_id()));
    }
}
