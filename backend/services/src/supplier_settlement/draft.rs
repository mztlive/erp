//! W27 供应商结算草稿的服务端来源快照创建与刷新。

use std::str::FromStr;

use database::{AccessControlExt, NoTransaction, SupplierSettlementExt, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
};
use entities::money::Amount;
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifference,
    SupplierSettlementDifferenceData, SupplierSettlementItem, SupplierSettlementItemData,
    SupplierSettlementSnapshotUpdate, SupplierSettlementSourceEvidence, SupplierSettlementStatement,
    SupplierSettlementStatementData,
};
use id_generator::next_id;
use validator::Validate;

use super::{
    command_audit_id, digest_parts, ensure_audit_resource, receipt_result, CreateSettlementStatementRequest,
    RefreshSettlementStatementRequest, SettlementDraftAction, SettlementDraftCommandResult,
    SupplierSettlementService, REVIEW_CUTOFF_POLICY_ID, REVIEW_CUTOFF_POLICY_VERSION,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 服务端构建的完整草稿快照。
struct DraftSnapshot {
    items: Vec<SupplierSettlementItem>,
    differences: Vec<SupplierSettlementDifference>,
    erp_amount: Amount,
    supplier_amount: Amount,
}

/// 刷新命令的持久化幂等收据。
struct RefreshReceipt {
    request_id: String,
    statement_version: u64,
    source_snapshot_hash: String,
    item_count: usize,
    difference_count: usize,
}

impl SupplierSettlementService {
    /// 从服务端最新的不可变来源批次创建结算草稿。
    ///
    /// 请求不接收金额明细。服务端逐行构造结算项与差异，并把单头、明细、差异和
    /// 幂等审计置于同一事务中；缺少来源批次时整单失败。
    pub async fn create_statement(
        &self,
        req: CreateSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        req.validate()?;
        if req.action != SettlementDraftAction::Create {
            return Err(Error::ValidationError(
                "创建结算草稿必须使用 CREATE 动作".to_string(),
            ));
        }
        let period_start = parse_business_date(&req.period_start, "结算期间开始")?;
        let period_end = parse_business_date(&req.period_end, "结算期间结束")?;
        if period_end < period_start {
            return Err(Error::ValidationError("结算期间结束不得早于开始".to_string()));
        }
        let statement_no = deterministic_statement_no(&req);
        if let Some(existing) = self
            .db
            .supplier_settlement_statements()
            .find_by_statement_no(&statement_no, &mut NoTransaction)
            .await?
        {
            validate_create_replay(&existing, &req, period_start, period_end)?;
            return self
                .draft_result(existing, req.request_id, "REPLAYED", "结算草稿创建结果已恢复")
                .await;
        }
        let source = self
            .db
            .supplier_settlement_source_evidence()
            .latest_for_scope(&req.supplier_id, period_start, period_end, &mut NoTransaction)
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(
                    "SOURCE_EVIDENCE_MISSING: 当前供应商与期间缺少完整来源证据批次".to_string(),
                )
            })?;
        let statement_id = SupplierSettlementStatementId::new(next_id());
        let snapshot = build_snapshot(&statement_id, &source)?;
        let now = Instant::now();
        let mut statement = SupplierSettlementStatement::new(
            statement_id,
            SupplierSettlementStatementData {
                statement_no: statement_no.clone(),
                supplier_id: req.supplier_id.clone(),
                period_start,
                period_end,
                period_policy_id: source.period_policy_id.clone(),
                period_policy_version: source.period_policy_version.clone(),
                period_timezone: source.timezone.clone(),
                external_bill_no: Some(source.external_bill_no.clone()),
                external_bill_version: Some(source.external_bill_version.clone()),
                erp_amount: snapshot.erp_amount,
                supplier_amount: snapshot.supplier_amount,
                subject_hash: "0".repeat(64),
                source_as_of: source.source_as_of,
                source_snapshot_at: now,
                source_snapshot_hash: source.source_hash.clone(),
                refresh_cutoff_policy_id: REVIEW_CUTOFF_POLICY_ID.to_string(),
                refresh_cutoff_policy_version: REVIEW_CUTOFF_POLICY_VERSION.to_string(),
                prepared_by: actor.id().to_string(),
            },
        )?;
        statement.refresh_snapshot(SupplierSettlementSnapshotUpdate {
            external_bill_no: source.external_bill_no.clone(),
            external_bill_version: source.external_bill_version.clone(),
            erp_amount: snapshot.erp_amount,
            supplier_amount: snapshot.supplier_amount,
            source_as_of: source.source_as_of,
            source_snapshot_at: now,
            source_snapshot_hash: source.source_hash.clone(),
            has_difference: !snapshot.differences.is_empty(),
        })?;
        statement.update_subject_hash(statement.review_subject_hash(&snapshot.differences))?;
        let fingerprint = create_fingerprint(&req);
        let audit_id = command_audit_id(
            actor.id(),
            "supplier_settlement.create",
            &statement_no,
            &req.idempotency_key,
        );
        let audit = actor.clone().resource_log_with_id(
            audit_id,
            "supplier_settlement.create",
            "supplier_settlement_statement",
            statement.base.id.clone(),
            Some(format!(
                "command_sha256={fingerprint};result={}|{}|{}|{}",
                req.request_id,
                statement.base.version,
                snapshot.items.len(),
                snapshot.differences.len(),
            )),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let statement_for_tx = statement.clone();
        let items_for_tx = snapshot.items.clone();
        let differences_for_tx = snapshot.differences.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement()
                        .create_statement_with_items(
                            &statement_for_tx,
                            &items_for_tx,
                            &differences_for_tx,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(existing) = self
                .db
                .supplier_settlement_statements()
                .find_by_statement_no(&statement_no, &mut NoTransaction)
                .await?
            {
                validate_create_replay(&existing, &req, period_start, period_end)?;
                return self
                    .draft_result(existing, req.request_id, "REPLAYED", "结算草稿创建结果已恢复")
                    .await;
            }
            return Err(error);
        }
        Ok(SettlementDraftCommandResult {
            result_status: "CREATED".to_string(),
            message: "结算草稿已从服务端权威来源快照创建".to_string(),
            request_id: req.request_id,
            statement: statement.into(),
            item_count: snapshot.items.len(),
            difference_count: snapshot.differences.len(),
        })
    }

    /// 使用同一供应商、期间和冻结策略下的最新来源批次刷新可编辑草稿。
    ///
    /// 命令要求结算单 CAS 与来源摘要同时匹配。相同来源为受审计的 no-op；新来源
    /// 则在事务内物理替换尚未提交复核的试算明细和差异。
    pub async fn refresh_statement(
        &self,
        id: &str,
        req: RefreshSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        req.validate()?;
        if req.action != SettlementDraftAction::Refresh {
            return Err(Error::ValidationError(
                "刷新结算草稿必须使用 REFRESH 动作".to_string(),
            ));
        }
        if id != req.statement_id {
            return Err(Error::ValidationError("结算单路径ID与命令载荷不一致".to_string()));
        }
        let fingerprint = refresh_fingerprint(&req);
        let audit_id = command_audit_id(
            actor.id(),
            "supplier_settlement.refresh",
            id,
            &req.idempotency_key,
        );
        if let Some(result) = self.replay_refresh(&audit_id, &fingerprint, id).await? {
            return Ok(result);
        }
        let mut statement = self.load_statement(id).await?;
        if statement.prepared_by != actor.id() {
            return Err(Error::Forbidden("只有当前结算经办人可以刷新试算".to_string()));
        }
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        if statement.source_snapshot_hash != req.expected_source_snapshot_hash {
            return Err(Error::ConflictError(
                "结算来源快照已变化，请刷新详情后重试".to_string(),
            ));
        }
        let source = self
            .db
            .supplier_settlement_source_evidence()
            .latest_for_period(
                &statement.supplier_id,
                statement.period_start,
                statement.period_end,
                &statement.period_policy_id,
                &statement.period_policy_version,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(
                    "SOURCE_EVIDENCE_MISSING: 冻结策略版本缺少完整来源证据批次".to_string(),
                )
            })?;
        let old_items = self.load_statement_items(id, &mut NoTransaction).await?;
        let old_differences = self
            .load_statement_differences(&old_items, &mut NoTransaction)
            .await?;
        if source.source_hash == statement.source_snapshot_hash {
            let receipt = RefreshReceipt {
                request_id: req.request_id.clone(),
                statement_version: statement.base.version,
                source_snapshot_hash: statement.source_snapshot_hash.clone(),
                item_count: old_items.len(),
                difference_count: old_differences.len(),
            };
            self.persist_refresh_audit(audit_id, fingerprint, &statement, &receipt, actor)
                .await?;
            return Ok(refresh_result(
                statement,
                receipt,
                "UNCHANGED",
                "当前已是最新权威来源快照",
            ));
        }
        let snapshot = build_snapshot(
            &SupplierSettlementStatementId::new(statement.base.id.clone()),
            &source,
        )?;
        statement.refresh_snapshot(SupplierSettlementSnapshotUpdate {
            external_bill_no: source.external_bill_no.clone(),
            external_bill_version: source.external_bill_version.clone(),
            erp_amount: snapshot.erp_amount,
            supplier_amount: snapshot.supplier_amount,
            source_as_of: source.source_as_of,
            source_snapshot_at: Instant::now(),
            source_snapshot_hash: source.source_hash.clone(),
            has_difference: !snapshot.differences.is_empty(),
        })?;
        statement.update_subject_hash(statement.review_subject_hash(&snapshot.differences))?;
        let old_item_ids = old_items
            .iter()
            .map(|item| item.base.id.clone())
            .collect::<Vec<_>>();
        let old_difference_ids = old_differences
            .iter()
            .map(|difference| difference.base.id.clone())
            .collect::<Vec<_>>();
        let receipt = RefreshReceipt {
            request_id: req.request_id.clone(),
            statement_version: statement.base.version + 1,
            source_snapshot_hash: statement.source_snapshot_hash.clone(),
            item_count: snapshot.items.len(),
            difference_count: snapshot.differences.len(),
        };
        let audit = refresh_audit(audit_id.clone(), &fingerprint, &statement, &receipt, actor)?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut statement_for_tx = statement.clone();
        let items_for_tx = snapshot.items.clone();
        let differences_for_tx = snapshot.differences.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let current = db
                        .supplier_settlement_statements()
                        .find_by_id(&statement_for_tx.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
                    if current.base.version != req.expected_lock_version
                        || current.source_snapshot_hash != req.expected_source_snapshot_hash
                    {
                        return Err(Error::ConflictError(
                            "结算单版本或来源快照已变化，请刷新后重试".to_string(),
                        ));
                    }
                    db.supplier_settlement()
                        .replace_draft_snapshot(
                            &mut statement_for_tx,
                            &old_item_ids,
                            &old_difference_ids,
                            &items_for_tx,
                            &differences_for_tx,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierSettlementStatement, crate::errors::Error>(statement_for_tx)
                })
            })
            .await;
        let statement = match transaction_result {
            Ok(statement) => statement,
            Err(error) => {
                if let Some(result) = self.replay_refresh(&audit_id, &fingerprint, id).await? {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(refresh_result(
            statement,
            receipt,
            "REFRESHED",
            "结算试算已刷新为最新权威来源快照",
        ))
    }

    async fn draft_result(
        &self,
        statement: SupplierSettlementStatement,
        request_id: String,
        result_status: &str,
        message: &str,
    ) -> Result<SettlementDraftCommandResult> {
        let items = self
            .load_statement_items(&statement.base.id, &mut NoTransaction)
            .await?;
        let differences = self
            .load_statement_differences(&items, &mut NoTransaction)
            .await?;
        Ok(SettlementDraftCommandResult {
            result_status: result_status.to_string(),
            message: message.to_string(),
            request_id,
            statement: statement.into(),
            item_count: items.len(),
            difference_count: differences.len(),
        })
    }

    async fn persist_refresh_audit(
        &self,
        audit_id: String,
        fingerprint: String,
        statement: &SupplierSettlementStatement,
        receipt: &RefreshReceipt,
        actor: &AuditActor,
    ) -> Result<()> {
        let audit = refresh_audit(audit_id, &fingerprint, statement, receipt, actor)?;
        match self.db.audit_logs().create(&audit, &mut NoTransaction).await {
            Ok(()) => Ok(()),
            Err(error) => {
                if self
                    .replay_refresh(&audit.base.id, &fingerprint, &statement.base.id)
                    .await?
                    .is_some()
                {
                    Ok(())
                } else {
                    Err(error.into())
                }
            }
        }
    }

    async fn replay_refresh(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, statement_id)?;
        let message = audit
            .message
            .as_deref()
            .ok_or_else(|| Error::Internal("刷新幂等收据缺少结果".to_string()))?;
        let receipt = parse_refresh_receipt(message, expected_fingerprint)?;
        let statement = self.load_statement(statement_id).await?;
        if statement.base.version != receipt.statement_version
            || statement.source_snapshot_hash != receipt.source_snapshot_hash
        {
            return Err(Error::ConflictError(
                "刷新幂等结果已被后续来源快照替代，请读取当前详情".to_string(),
            ));
        }
        Ok(Some(refresh_result(
            statement,
            receipt,
            "REPLAYED",
            "结算试算刷新结果已恢复",
        )))
    }
}

fn build_snapshot(
    statement_id: &SupplierSettlementStatementId,
    source: &SupplierSettlementSourceEvidence,
) -> Result<DraftSnapshot> {
    let mut items = Vec::with_capacity(source.lines.len());
    let mut differences = Vec::new();
    let mut erp_amount = zero();
    let mut supplier_amount = zero();
    for line in &source.lines {
        let item = SupplierSettlementItem::new(
            SupplierSettlementItemId::new(next_id()),
            SupplierSettlementItemData {
                statement_id: statement_id.clone(),
                supplier_fulfillment_order_id: line.supplier_fulfillment_order_id.clone(),
                supplier_fulfillment_item_id: line.supplier_fulfillment_item_id.clone(),
                quantity: line.quantity,
                order_amount: line.order_gross,
                freight_amount: line.freight_gross,
                service_fee_amount: line.service_fee_gross,
                refund_amount: line.refund_gross,
                erp_calculated_amount: line.erp_gross,
                erp_calculated_net_amount: line.erp_net,
                erp_calculated_tax_amount: line.erp_tax,
                supplier_billed_amount: line.supplier_billed_gross,
                supplier_billed_net_amount: line.supplier_billed_net,
                supplier_billed_tax_amount: line.supplier_billed_tax,
            },
        )?;
        erp_amount = erp_amount.checked_add(line.erp_gross);
        supplier_amount = supplier_amount.checked_add(line.supplier_billed_gross);
        let difference_amount = line.supplier_billed_gross.checked_sub(line.erp_gross);
        if difference_amount != zero() {
            differences.push(SupplierSettlementDifference::new(
                SupplierSettlementDifferenceId::new(next_id()),
                SupplierSettlementDifferenceData {
                    statement_item_id: SupplierSettlementItemId::new(item.base.id.clone()),
                    difference_type: SettlementDifferenceType::Amount,
                    difference_amount,
                    status: SettlementDifferenceStatus::Pending,
                    resolution: None,
                    resolved_by: None,
                    resolved_at: None,
                },
            )?);
        }
        items.push(item);
    }
    if items.is_empty() {
        return Err(Error::BusinessLogicError(
            "SOURCE_EVIDENCE_INCOMPLETE: 来源证据批次没有可结算行".to_string(),
        ));
    }
    Ok(DraftSnapshot {
        items,
        differences,
        erp_amount,
        supplier_amount,
    })
}

fn deterministic_statement_no(req: &CreateSettlementStatementRequest) -> String {
    let digest = digest_parts(&[
        "supplier-settlement-create-v1".to_string(),
        req.request_id.clone(),
        req.idempotency_key.clone(),
    ]);
    let month = req
        .period_end
        .chars()
        .filter(char::is_ascii_digit)
        .take(6)
        .collect::<String>();
    format!("ST-{month}-{}", &digest[..16])
}

fn validate_create_replay(
    statement: &SupplierSettlementStatement,
    req: &CreateSettlementStatementRequest,
    period_start: BusinessDate,
    period_end: BusinessDate,
) -> Result<()> {
    if statement.supplier_id != req.supplier_id
        || statement.period_start != period_start
        || statement.period_end != period_end
    {
        return Err(Error::ConflictError(
            "创建幂等键已用于不同的供应商结算命令".to_string(),
        ));
    }
    Ok(())
}

fn create_fingerprint(req: &CreateSettlementStatementRequest) -> String {
    digest_parts(&[
        "CREATE".to_string(),
        req.request_id.clone(),
        req.supplier_id.to_string(),
        req.period_start.trim().to_string(),
        req.period_end.trim().to_string(),
    ])
}

fn refresh_fingerprint(req: &RefreshSettlementStatementRequest) -> String {
    digest_parts(&[
        "REFRESH".to_string(),
        req.request_id.clone(),
        req.statement_id.clone(),
        req.expected_lock_version.to_string(),
        req.expected_source_snapshot_hash.clone(),
    ])
}

fn refresh_audit(
    audit_id: String,
    fingerprint: &str,
    statement: &SupplierSettlementStatement,
    receipt: &RefreshReceipt,
    actor: &AuditActor,
) -> Result<entities::AuditLog> {
    actor.clone().resource_log_with_id(
        audit_id,
        "supplier_settlement.refresh",
        "supplier_settlement_statement",
        statement.base.id.clone(),
        Some(format!(
            "command_sha256={fingerprint};result={}|{}|{}|{}|{}",
            receipt.request_id,
            receipt.statement_version,
            receipt.source_snapshot_hash,
            receipt.item_count,
            receipt.difference_count,
        )),
    )
}

fn parse_refresh_receipt(message: &str, fingerprint: &str) -> Result<RefreshReceipt> {
    let fields = receipt_result(message, fingerprint, "刷新结算试算")?
        .split('|')
        .collect::<Vec<_>>();
    let [request_id, version, source_hash, item_count, difference_count] = fields.as_slice() else {
        return Err(Error::Internal("刷新结算试算幂等收据非法".to_string()));
    };
    Ok(RefreshReceipt {
        request_id: (*request_id).to_string(),
        statement_version: parse_positive_u64(version, "结算单版本")?,
        source_snapshot_hash: (*source_hash).to_string(),
        item_count: parse_usize(item_count, "结算明细数")?,
        difference_count: parse_usize(difference_count, "结算差异数")?,
    })
}

fn refresh_result(
    statement: SupplierSettlementStatement,
    receipt: RefreshReceipt,
    result_status: &str,
    message: &str,
) -> SettlementDraftCommandResult {
    SettlementDraftCommandResult {
        result_status: result_status.to_string(),
        message: message.to_string(),
        request_id: receipt.request_id,
        statement: statement.into(),
        item_count: receipt.item_count,
        difference_count: receipt.difference_count,
    }
}

fn parse_business_date(value: &str, field: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("{field}不是合法业务日期")))
}

fn parse_positive_u64(value: &str, field: &str) -> Result<u64> {
    let value = value
        .parse::<u64>()
        .map_err(|_| Error::Internal(format!("刷新收据{field}非法")))?;
    if value == 0 {
        return Err(Error::Internal(format!("刷新收据{field}非法")));
    }
    Ok(value)
}

fn parse_usize(value: &str, field: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|_| Error::Internal(format!("刷新收据{field}非法")))
}

fn zero() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
    use entities::money::Quantity;
    use entities::supplier_settlement::{
        SettlementSourceFactType, SupplierSettlementSourceEvidenceData, SupplierSettlementSourceEvidenceLine,
    };

    fn source() -> SupplierSettlementSourceEvidence {
        SupplierSettlementSourceEvidence::new(
            "source-1",
            SupplierSettlementSourceEvidenceData {
                request_id: "source-request-1".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_str("2026-07-01").unwrap(),
                period_end: BusinessDate::from_str("2026-07-31").unwrap(),
                period_policy_id: "monthly".to_string(),
                period_policy_version: "1".to_string(),
                timezone: "Asia/Shanghai".to_string(),
                source_version: 1,
                external_bill_no: "BILL-1".to_string(),
                external_bill_version: "1".to_string(),
                external_bill_evidence_reference_id: "bill://1".to_string(),
                lines: vec![SupplierSettlementSourceEvidenceLine {
                    supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                    supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
                    quantity: Quantity::from_str("1").unwrap(),
                    source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
                    evidence_reference_ids: vec!["fulfillment://1".to_string()],
                    order_gross: Amount::from_str("113.00").unwrap(),
                    order_net: Amount::from_str("100.00").unwrap(),
                    order_tax: Amount::from_str("13.00").unwrap(),
                    freight_gross: zero(),
                    freight_net: zero(),
                    freight_tax: zero(),
                    service_fee_gross: zero(),
                    service_fee_net: zero(),
                    service_fee_tax: zero(),
                    refund_gross: zero(),
                    refund_net: zero(),
                    refund_tax: zero(),
                    erp_gross: Amount::from_str("113.00").unwrap(),
                    erp_net: Amount::from_str("100.00").unwrap(),
                    erp_tax: Amount::from_str("13.00").unwrap(),
                    supplier_billed_gross: Amount::from_str("114.00").unwrap(),
                    supplier_billed_net: Amount::from_str("100.88").unwrap(),
                    supplier_billed_tax: Amount::from_str("13.12").unwrap(),
                }],
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                recorded_by: "finance-1".to_string(),
                source_hash: "a".repeat(64),
                request_hash: "b".repeat(64),
            },
        )
        .unwrap()
    }

    #[test]
    fn snapshot_builds_one_exact_item_and_signed_difference() {
        let snapshot = build_snapshot(&SupplierSettlementStatementId::new("statement-1"), &source()).unwrap();
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.differences.len(), 1);
        assert_eq!(
            snapshot.differences[0].difference_amount,
            Amount::from_str("1.00").unwrap()
        );
        assert_eq!(snapshot.items[0].supplier_fulfillment_item_id.as_ref(), "item-1");
    }
}
