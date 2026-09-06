//! 域 D33 `supplier_settlement` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建结算草稿：结算单 + 全部明细同事务（`create_statement_with_items` 要求事务
//!   执行器，§6.20）；
//! - 结算确认（§8.4 第 6 条）：锁定结算单并重验差异处理结果、形成结算单应付
//!   （D19 `PayableRepository::create_payable_with_entry`，来源类型
//!   `SupplierSettlement`）并更新结算状态，同一事务完成；最终成本差额（D20
//!   `cost_entry`）不在本域声明依赖内，见 PR「未实现且已知的缺口」。
//!
//! 跨域协作只经 DatabaseExt 调对方域 Repository（P3 §2）：D32 `supplier_fulfillment`
//! （履约订单与明细存在性）、D19 `payable`（应付账户与原始分录）。
//!
//! 资金/状态机入口一律幂等：创建键为 `statement_no`，确认/提交复核/作废重复提交
//! 返回原结算单当前视图（不重复形成应付、不重复推进状态）；差异处理以版本 CAS
//! 防并发覆盖。

use std::str::FromStr;

use database::SupplierSettlementExt;
use entities::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use erp_core::money::Amount;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

mod difference;
mod draft;
mod dto;
mod evidence;
mod item;
mod query;
mod review;
mod source;
mod void;

pub(super) use self::difference::settlement_difference_view;
#[cfg(test)]
pub(super) use self::difference::{
    difference_decision_receipt_message, parse_difference_decision_receipt, DifferenceDecisionReceipt,
};
pub use self::dto::{
    CreateSettlementStatementRequest, RecordSettlementSourceEvidenceLineRequest,
    RecordSettlementSourceEvidenceRequest, RefreshSettlementStatementRequest,
    SettlementDifferenceDecisionRequest, SettlementDifferenceDecisionResult,
    SettlementDifferenceEvidenceRequest, SettlementDifferenceEvidenceResult,
    SettlementDifferenceEvidenceView, SettlementDraftAction, SettlementDraftCommandResult,
    SettlementPageView, SettlementReviewCommand, SettlementReviewDecisionResult,
    SubmitSettlementReviewRequest, SubmitSettlementReviewResult, SupplierSettlementDifferenceListParams,
    SupplierSettlementDifferenceView, SupplierSettlementItemListParams, SupplierSettlementItemView,
    SupplierSettlementSourceEvidenceQuery, SupplierSettlementSourceEvidenceView,
    SupplierSettlementStatementDetailView, SupplierSettlementStatementListParams,
    SupplierSettlementStatementListView, SupplierSettlementStatementView, VoidSettlementRequest,
};
#[cfg(test)]
pub(super) use self::review::{
    build_settlement_cost_delta, parse_review_decision_receipt, parse_review_submission_receipt,
    review_decision_receipt_message, review_submission_receipt_message, validate_settlement_review_work_item,
    ReviewDecisionReceipt, ReviewSubmissionReceipt,
};
pub(super) use self::review::{review_blocker, settlement_review_access};

const REVIEW_CUTOFF_POLICY_ID: &str = "supplier-settlement-review-cutoff";
const REVIEW_CUTOFF_POLICY_VERSION: &str = "1";
const SETTLEMENT_REVIEW_OWNER_ROLE: &str = "role-finance";
/// 当前结算模型尚无更细组织上下文，使用明确的最小公司根并在资格校验中重验。
const SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID: &str = "company";
const COMMAND_RECEIPT_PREFIX: &str = "supplier-settlement-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// 供应商结算服务。
///
/// 提供供应商周期结算单的创建、查询、复核/确认/作废与差异处理编排。
pub struct SupplierSettlementService {
    db: Database,
}

impl SupplierSettlementService {
    /// 创建供应商结算服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 加载结算单全部冻结明细。
    async fn load_statement_items(
        &self,
        statement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        load_statement_items(&self.db, statement_id, executor).await
    }

    /// 加载结算明细关联的全部正式差异。
    async fn load_statement_differences(
        &self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifference>> {
        load_statement_differences(&self.db, items, executor).await
    }

    /// 按 ID 加载未删除结算单。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回结算单实体。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    async fn load_statement(&self, id: &str) -> Result<SupplierSettlementStatement> {
        self.db
            .supplier_settlement_statements()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))
    }
}

pub(super) async fn load_statement_items(
    db: &Database,
    statement_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementItem>> {
    db.supplier_settlement_items()
        .list_by_statement(statement_id, executor)
        .await
        .map_err(Into::into)
}

pub(super) async fn load_statement_differences(
    db: &Database,
    items: &[SupplierSettlementItem],
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierSettlementDifference>> {
    let item_ids = items
        .iter()
        .map(|item| erp_core::ids::SupplierSettlementItemId::new(item.base.id.as_str()))
        .collect::<Vec<_>>();
    db.supplier_settlement_differences()
        .list_by_statement_item_ids(&item_ids, executor)
        .await
        .map_err(Into::into)
}

/// 校验路径身份与命令载荷身份一致。
pub(super) fn ensure_same_id(path_id: &str, command_id: &str, object_name: &str) -> Result<()> {
    if path_id != command_id {
        return Err(Error::ValidationError(format!(
            "{object_name}路径ID与命令载荷不一致"
        )));
    }
    Ok(())
}

/// 对字段逐项加入长度前缀后计算稳定摘要，消除拼接歧义。
pub(super) fn digest_parts(parts: &[String]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}

/// 生成不暴露原始幂等键的稳定审计收据 ID。
pub(super) fn command_audit_id(
    actor_id: &str,
    action: &str,
    resource_id: &str,
    idempotency_key: &str,
) -> String {
    let digest = digest_parts(&[
        actor_id.to_string(),
        action.to_string(),
        resource_id.to_string(),
        idempotency_key.to_string(),
    ]);
    format!("{COMMAND_RECEIPT_PREFIX}{digest}")
}

/// 提取审计消息中的命令指纹与结果载荷。
pub(super) fn receipt_result<'a>(
    message: &'a str,
    expected_fingerprint: &str,
    command_name: &str,
) -> Result<&'a str> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal(format!("{command_name}幂等收据格式非法")))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(format!(
            "幂等键已用于不同的{command_name}命令"
        )));
    }
    Ok(result)
}

/// 解析幂等收据中的正整数版本。
pub(super) fn parse_receipt_number(value: &str, field: &str) -> Result<u64> {
    let value = value
        .parse::<u64>()
        .map_err(|_| Error::Internal(format!("结算命令收据{field}非法")))?;
    if value == 0 {
        return Err(Error::Internal(format!("结算命令收据{field}非法")));
    }
    Ok(value)
}

/// 校验幂等收据仍指向同一成功业务资源。
pub(super) fn ensure_audit_resource(audit: &entities::AuditLog, resource_id: &str) -> Result<()> {
    if !audit.success || audit.resource_id.as_deref() != Some(resource_id) {
        return Err(Error::ConflictError("幂等收据与当前业务资源不一致".to_string()));
    }
    Ok(())
}

/// 返回零金额（表头金额累加起点）。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use application_core::AuditActor;
    use database::SupplierSettlementExt;
    use entities::supplier_settlement::{
        SettlementCostDelta, SettlementDifferenceStatus, SettlementDifferenceType,
        SupplierSettlementDifferenceData, SupplierSettlementDifferenceEvidence,
        SupplierSettlementDifferenceEvidenceData, SupplierSettlementItemData,
        SupplierSettlementStatementData,
    };
    use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
        SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId, WorkItemId,
    };
    use erp_core::money::Quantity;
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;
    use test_support::{require_mongo, TestDb};

    fn sample_statement() -> SupplierSettlementStatement {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            SupplierSettlementStatementData {
                statement_no: "ST-2026-001".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".to_string(),
                period_policy_version: "1".to_string(),
                period_timezone: "Asia/Shanghai".to_string(),
                external_bill_no: Some("BILL-1".to_string()),
                external_bill_version: Some("1".to_string()),
                erp_amount: Amount::from_str("100.00").unwrap(),
                supplier_amount: Amount::from_str("101.00").unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: REVIEW_CUTOFF_POLICY_ID.to_string(),
                refresh_cutoff_policy_version: REVIEW_CUTOFF_POLICY_VERSION.to_string(),
                prepared_by: "preparer-1".to_string(),
            },
        )
        .unwrap();
        statement
            .update_subject_hash(statement.review_subject_hash(&[]))
            .unwrap();
        statement
    }

    fn sample_work_item(statement: &SupplierSettlementStatement) -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("work-item-1"),
            WorkItemData {
                work_item_type: WorkItemType::SupplierSettlementReview,
                business_object_type: "supplier_settlement_statement".to_string(),
                business_object_id: statement.base.id.clone(),
                subject_version: statement.subject_hash.clone(),
                owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
                owner_organization_id: SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID.to_string(),
                owner_user_id: "reviewer-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1_700_000_000),
        )
        .unwrap()
    }

    #[test]
    fn command_receipts_roundtrip_and_reject_fingerprint_reuse() {
        let fingerprint = "f".repeat(64);
        let submission = ReviewSubmissionReceipt {
            operation_id: "op-submit".to_string(),
            statement_version: 2,
            work_item_id: "work-item-1".to_string(),
            task_version: 1,
        };
        let message = review_submission_receipt_message(&fingerprint, &submission);
        assert_eq!(
            parse_review_submission_receipt(&message, &fingerprint).unwrap(),
            submission
        );
        assert!(parse_review_submission_receipt(&message, &"0".repeat(64)).is_err());

        let decision = ReviewDecisionReceipt {
            operation_id: "op-review".to_string(),
            result_status: dto::SettlementReviewDecisionStatus::Confirmed,
            statement_version: 3,
            task_version: 2,
            payable_account_id: Some("payable-1".to_string()),
            cost_delta: Some(Amount::from_str("1.00").unwrap()),
        };
        let message = review_decision_receipt_message(&fingerprint, &decision);
        assert_eq!(
            parse_review_decision_receipt(&message, &fingerprint).unwrap(),
            decision
        );

        let difference = DifferenceDecisionReceipt {
            operation_id: "op-difference".to_string(),
            statement_id: "statement-1".to_string(),
            statement_version: 2,
            difference_version: 2,
        };
        let message = difference_decision_receipt_message(&fingerprint, &difference);
        assert_eq!(
            parse_difference_decision_receipt(&message, &fingerprint).unwrap(),
            difference
        );
    }

    #[test]
    fn review_access_is_actor_specific_and_fail_closed() {
        let statement = sample_statement();
        let item = sample_work_item(&statement);
        let (domain_actions, blockers) = settlement_review_access(&item, "other-reviewer", true, true);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "CURRENT_OWNER_MISMATCH");

        let (domain_actions, blockers) = settlement_review_access(&item, "reviewer-1", true, true);
        assert_eq!(domain_actions, vec!["REJECT", "CONFIRM"]);
        assert!(blockers.is_empty());

        let (domain_actions, blockers) = settlement_review_access(&item, "reviewer-1", false, true);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "ASSIGNMENT_NOT_ELIGIBLE");
        let (domain_actions, blockers) = settlement_review_access(&item, "reviewer-1", true, false);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "SEGREGATION_OF_DUTIES");
    }

    #[test]
    fn work_item_validation_requires_exact_three_versions_and_current_owner() {
        let mut statement = sample_statement();
        statement.submit_review().unwrap();
        let mut item = sample_work_item(&statement);
        let actor = AuditActor::new(
            "reviewer-1".to_string(),
            "reviewer".to_string(),
            AccountKind::Admin,
        );

        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &statement.subject_hash,
            &actor,
        )
        .is_ok());
        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version + 1,
            &statement.subject_hash,
            &actor,
        )
        .is_err());
        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &"0".repeat(64),
            &actor,
        )
        .is_err());
        item.owner_user_id = Some("other-reviewer".to_string());
        assert!(validate_settlement_review_work_item(
            &item,
            &statement,
            item.base.version,
            &statement.subject_hash,
            &actor,
        )
        .is_err());
    }

    #[test]
    fn command_ids_are_bounded() {
        let audit_id = command_audit_id(
            "actor-1",
            "supplier_settlement.review_confirm",
            "statement-1",
            "raw-idempotency-secret",
        );
        assert!(!audit_id.contains("raw-idempotency-secret"));
        assert!(audit_id.len() <= 128);
    }

    #[test]
    fn cost_delta_writer_blocks_nonzero_delta_without_authoritative_lineage() {
        let delta = SettlementCostDelta {
            gross: Amount::from_str("1.00").unwrap(),
            net: Amount::from_str("0.87").unwrap(),
            tax: Amount::from_str("0.13").unwrap(),
        };

        assert!(build_settlement_cost_delta(
            &sample_statement(),
            &delta,
            Instant::from_unix_secs(1_700_000_300),
        )
        .is_err());
    }

    #[test]
    fn cost_delta_writer_skips_zero_delta() {
        assert!(build_settlement_cost_delta(
            &sample_statement(),
            &SettlementCostDelta::zero(),
            Instant::from_unix_secs(1_700_000_300),
        )
        .unwrap()
        .is_empty());
    }

    /// 详情明细夹具（订单 100 + 运费 10 + 服务费 5 − 退款 0 = ERP 115）。
    fn detail_item(id: &str) -> SupplierSettlementItem {
        SupplierSettlementItem::new(
            SupplierSettlementItemId::new(id),
            SupplierSettlementItemData {
                statement_id: SupplierSettlementStatementId::new("statement-1"),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(format!("order-{id}")),
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new(format!("fulfillment-{id}")),
                quantity: Quantity::from_str("1").unwrap(),
                order_amount: Amount::from_str("100.00").unwrap(),
                freight_amount: Amount::from_str("10.00").unwrap(),
                service_fee_amount: Amount::from_str("5.00").unwrap(),
                refund_amount: Amount::from_str("0.00").unwrap(),
                erp_calculated_amount: Amount::from_str("115.00").unwrap(),
                erp_calculated_net_amount: Amount::from_str("100.00").unwrap(),
                erp_calculated_tax_amount: Amount::from_str("15.00").unwrap(),
                supplier_billed_amount: Amount::from_str("115.00").unwrap(),
                supplier_billed_net_amount: Amount::from_str("100.00").unwrap(),
                supplier_billed_tax_amount: Amount::from_str("15.00").unwrap(),
            },
        )
        .unwrap()
    }

    /// 详情差异夹具（待处理或已认可，不带处理三元组）。
    fn detail_difference(
        id: &str,
        item_id: &str,
        status: SettlementDifferenceStatus,
    ) -> SupplierSettlementDifference {
        SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new(id),
            SupplierSettlementDifferenceData {
                statement_item_id: SupplierSettlementItemId::new(item_id),
                difference_type: SettlementDifferenceType::Amount,
                difference_amount: Amount::from_str("1.00").unwrap(),
                status,
                resolution: None,
                resolved_by: None,
                resolved_at: None,
            },
        )
        .unwrap()
    }

    /// 缺失的结算单映射为 `NotFound`（快照 `None` 的服务语义）。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn detail_missing_statement_maps_to_not_found() {
        require_mongo!(async {
            let fixture = TestDb::new("ful_r07_service_detail_missing")
                .await
                .expect("测试数据库创建失败");
            let service = SupplierSettlementService::new(fixture.db().clone());
            let actor = AuditActor::new("viewer-1".to_string(), "viewer".to_string(), AccountKind::Admin);
            let error = service
                .supplier_settlement_statement_detail("statement-missing", &actor)
                .await
                .expect_err("缺失结算单必须失败");
            assert!(
                matches!(error, Error::NotFound(_)),
                "缺失结算单必须映射为 NotFound"
            );
        });
    }

    /// 详情正确挂载补证并上报已举证/待处理计数。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn detail_attaches_evidence_and_reports_counts() {
        require_mongo!(async {
            let fixture = TestDb::new("ful_r07_service_detail_counts")
                .await
                .expect("测试数据库创建失败");
            let db = fixture.db();
            let statement = sample_statement();
            let items = vec![detail_item("item-1"), detail_item("item-2")];
            let differences = vec![
                detail_difference("difference-1", "item-1", SettlementDifferenceStatus::Pending),
                detail_difference(
                    "difference-2",
                    "item-2",
                    SettlementDifferenceStatus::SupplierAcknowledged,
                ),
            ];
            db.supplier_settlement()
                .create_statement_with_items(&statement, &items, &differences, &mut NoTransaction)
                .await
                .expect("结算单及明细差异插入失败");
            let evidence = SupplierSettlementDifferenceEvidence::new(
                "evidence-1",
                SupplierSettlementDifferenceEvidenceData {
                    request_id: "request-1".to_string(),
                    statement_id: SupplierSettlementStatementId::new("statement-1"),
                    difference_id: SupplierSettlementDifferenceId::new("difference-1"),
                    evidence_reference_ids: vec!["ticket://1".to_string()],
                    opinion_code: None,
                    comment: None,
                    provided_by: "preparer-1".to_string(),
                    provided_at: Instant::from_unix_secs(1_700_000_100),
                    command_hash: "a".repeat(64),
                },
            )
            .unwrap();
            db.supplier_settlement_difference_evidence()
                .create(&evidence, &mut NoTransaction)
                .await
                .expect("补证插入失败");
            let service = SupplierSettlementService::new(db.clone());
            let actor = AuditActor::new("viewer-1".to_string(), "viewer".to_string(), AccountKind::Admin);
            let view = service
                .supplier_settlement_statement_detail("statement-1", &actor)
                .await
                .expect("详情查询失败");
            assert_eq!(view.stats.item_count, 2, "明细计数必须为 2");
            assert_eq!(view.stats.difference_count, 2, "差异计数必须为 2");
            assert_eq!(view.stats.pending_difference_count, 1, "待处理计数必须为 1");
            assert_eq!(view.stats.evidenced_difference_count, 1, "已举证计数必须为 1");
            let evidenced = view
                .differences
                .iter()
                .find(|difference| difference.id == "difference-1")
                .expect("差异 difference-1 必须存在");
            assert_eq!(evidenced.evidence.len(), 1, "补证只能归入所属差异");
            let bare = view
                .differences
                .iter()
                .find(|difference| difference.id == "difference-2")
                .expect("差异 difference-2 必须存在");
            assert!(bare.evidence.is_empty(), "无补证差异的证据集合为空");
        });
    }
}
