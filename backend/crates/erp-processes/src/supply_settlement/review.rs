use std::str::FromStr;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_core::money::Amount;
use erp_supply::entity::supplier_settlement::{
    SettlementReviewResult, SettlementStatus, SupplierSettlementStatement,
};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::review::{
    ensure_current_subject_and_resolved_differences, ensure_review_submission_ready,
    validate_review_submission_snapshot,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::{
    COMMAND_FINGERPRINT_PREFIX, SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID, SETTLEMENT_REVIEW_OWNER_ROLE,
    SettlementReviewCommand, SettlementReviewDecisionResult, SubmitSettlementReviewRequest,
    SubmitSettlementReviewResult, SupplierSettlementProcess, command_audit_id, digest_parts, dto,
    ensure_audit_resource, ensure_same_id, parse_receipt_number, receipt_result,
};
use crate::{Error, Result};

impl SupplierSettlementProcess {
    /// 提交冻结结算主题并原子创建唯一财务复核任务。
    ///
    /// 命令重验结算单版本、服务端主题摘要、刷新截止策略与完整差异结论；结算单
    /// 进入待复核、`SUPPLIER_SETTLEMENT_REVIEW` 任务和幂等审计在同一事务写入。
    ///
    /// # 错误
    /// 路径身份、版本、主题、策略或差异状态不一致时 fail-closed。
    pub async fn submit_review(
        &self,
        id: &str,
        req: SubmitSettlementReviewRequest,
        actor: &AuditActor,
    ) -> Result<SubmitSettlementReviewResult> {
        req.validate()?;
        ensure_same_id(id, &req.statement_id, "结算单")?;
        let fingerprint = submit_review_fingerprint(&req);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.submit_review", id, &req.idempotency_key);
        if let Some(result) = self.replay_review_submission(&audit_id, &fingerprint, id).await? {
            return Ok(result);
        }
        let statement = self.domain().load_statement(id, &mut NoTransaction).await?;
        if !statement.is_prepared_by(actor.id()) {
            return Err(Error::Forbidden("只有当前结算经办人可以提交财务复核".to_string()));
        }
        statement
            .ensure_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        validate_review_submission_snapshot(&statement, &req)?;
        let auth = crate::adapters::workflow::workflow_auth(
            self.db.clone(),
            crate::adapters::identity::shared_rbac_service(self.db.clone()),
        );
        let policy_revision = self.authorize_reviewer(&auth, &req.reviewer_user_id, actor.id()).await?;
        let work_item = WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::SupplierSettlementReview,
                business_object_type: "supplier_settlement_statement".to_string(),
                business_object_id: statement.base.id.clone(),
                subject_version: statement.subject_hash.clone(),
                owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
                owner_organization_id: SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID.to_string(),
                owner_user_id: req.reviewer_user_id.clone(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("supplier_settlement_review_dispatched".to_string()),
                impact_summary: Some(format!("复核供应商结算单 {}", statement.statement_no)),
            },
        )?;
        let db = self.db.clone();

        let actor_id = actor.id().to_string();
        let expected_subject_hash = statement.subject_hash.clone();
        let audit_actor = actor.clone();
        let operation_id = req.operation_id.clone();
        let operation_id_for_tx = operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let transaction_result = auth
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let mut current = db
                        .supplier_settlement_statements()
                        .find_by_id(&statement.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
                    if current.base.version != statement.base.version
                        || current.subject_hash != expected_subject_hash
                    {
                        return Err(Error::ConflictError("结算单版本或主题已变化，请刷新后重试".to_string()));
                    }
                    ensure_review_submission_ready(&db, &current, &actor_id, session).await?;
                    current.submit_review()?;
                    SupplierSettlementService::new(db.clone())
                        .persist_statement(&mut current, session)
                        .await?;
                    db.work_items().create(&work_item, session).await?;
                    let receipt = ReviewSubmissionReceipt {
                        operation_id: operation_id_for_tx,
                        statement_version: current.base.version,
                        work_item_id: work_item.base.id.clone(),
                        task_version: work_item.base.version,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        "supplier_settlement.submit_review",
                        "supplier_settlement_statement",
                        current.base.id.clone(),
                        Some(review_submission_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(SupplierSettlementStatement, WorkItem), crate::Error>((current, work_item))
                })
            })
            .await;
        let (statement, work_item) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) = self.replay_review_submission(&audit_id, &fingerprint, id).await? {
                    return Ok(result);
                }
                return Err(error);
            },
        };
        Ok(SubmitSettlementReviewResult {
            result_status: dto::SettlementReviewSubmissionStatus::Submitted,
            message: "结算主题已冻结并提交财务复核".to_string(),
            operation_id,
            statement: statement.into(),
            work_item_id: work_item.base.id,
        })
    }

    /// 使用当前正式任务完成供应商结算复核决定。
    ///
    /// `REJECT` 与 `CONFIRM` 均校验任务 CAS、主题摘要、结算单 CAS、当前责任、
    /// 财务角色/组织范围与岗位分离；业务事实、任务完成、审计以及确认形成的应付
    /// 和成本差额在同一事务写入。
    ///
    /// # 错误
    /// 任务、责任、主题、结算版本或正式业务前置条件不一致时 fail-closed。
    pub async fn decide_review(
        &self,
        id: &str,
        req: SettlementReviewCommand,
        actor: &AuditActor,
    ) -> Result<SettlementReviewDecisionResult> {
        req.validate()?;
        ensure_same_id(id, &req.decision.statement_id, "结算单")?;
        let reject_reason = req.decision.parsed_reject_reason()?;
        let expected_task_version = parse_expected_version(&req.expected_task_version, "待办版本")?;
        let action = match req.decision.action {
            dto::SettlementReviewAction::Reject => "supplier_settlement.review_reject",
            dto::SettlementReviewAction::Confirm => "supplier_settlement.review_confirm",
        };
        let fingerprint = review_decision_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), action, id, &req.idempotency_key);
        if let Some(result) =
            self.replay_review_decision(&audit_id, &fingerprint, id, &req.work_item_id).await?
        {
            return Ok(result);
        }
        let mut statement = self.domain().load_statement(id, &mut NoTransaction).await?;
        statement
            .ensure_version(req.decision.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        let mut work_item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算复核待办不存在".to_string()))?;
        validate_settlement_review_work_item(
            &work_item,
            &statement,
            expected_task_version,
            &req.expected_subject_version,
            actor,
        )?;
        let items = self.domain().load_statement_items(id, &mut NoTransaction).await?;
        let differences = self.domain().load_statement_differences(&items, &mut NoTransaction).await?;
        ensure_current_subject_and_resolved_differences(&statement, &differences)?;
        let now = Instant::now();
        let super::review_preparation::PreparedReview {
            payable,
            payable_entry,
            cost_entries,
            cost_delta,
            result_status,
        } = super::review_preparation::prepare(
            &mut statement,
            &mut work_item,
            &items,
            &differences,
            super::review_preparation::ReviewInput {
                request: &req,
                reject_reason,
                actor_id: actor.id(),
                at: now,
            },
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let rbac_for_tx = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let operation_id = req.decision.operation_id.clone();
        let operation_id_for_tx = operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let action_for_tx = action.to_string();
        let result_status_for_tx = result_status;
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let receipt = super::review_posting::post(
                        super::review_posting::Posting {
                            db: &db,
                            actor: &audit_actor,
                            actor_id: &actor_id,
                            rbac: &rbac_for_tx,
                            statement: &mut statement,
                            work_item: &mut work_item,
                            payable: payable.as_ref(),
                            payable_entry: payable_entry.as_ref(),
                            cost_entries: &cost_entries,
                            cost_delta,
                            result_status: result_status_for_tx,
                            operation_id: operation_id_for_tx,
                            fingerprint: fingerprint_for_tx,
                            audit_id: audit_id_for_tx,
                            action: action_for_tx,
                        },
                        session,
                    )
                    .await?;
                    Ok::<(SupplierSettlementStatement, WorkItem, ReviewDecisionReceipt), crate::Error>((
                        statement, work_item, receipt,
                    ))
                })
            })
            .await;
        let (statement, work_item, receipt) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                if let Some(result) =
                    self.replay_review_decision(&audit_id, &fingerprint, id, &req.work_item_id).await?
                {
                    return Ok(result);
                }
                return Err(error);
            },
        };
        Ok(review_decision_result(statement, work_item, operation_id, receipt))
    }

    /// 重放提交复核命令并校验收据载荷。
    async fn replay_review_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
    ) -> Result<Option<SubmitSettlementReviewResult>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, statement_id)?;
        let receipt = parse_review_submission_receipt(
            audit.message.as_deref().unwrap_or_default(),
            expected_fingerprint,
        )?;
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        ensure_submission_statement(&statement, &receipt)?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(&receipt.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("提交复核收据引用的任务不存在".to_string()))?;
        ensure_submission_task(&statement, &work_item, &receipt)?;
        Ok(Some(SubmitSettlementReviewResult {
            result_status: dto::SettlementReviewSubmissionStatus::Submitted,
            message: "结算主题已冻结并提交财务复核".to_string(),
            operation_id: receipt.operation_id,
            statement: statement.into(),
            work_item_id: receipt.work_item_id,
        }))
    }

    /// 重放正式复核决定并恢复同一业务结果。
    async fn replay_review_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
        work_item_id: &str,
    ) -> Result<Option<SettlementReviewDecisionResult>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, statement_id)?;
        let receipt = parse_review_decision_receipt(
            audit.message.as_deref().unwrap_or_default(),
            expected_fingerprint,
        )?;
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("复核决定收据引用的任务不存在".to_string()))?;
        ensure_decision_replay(&statement, &work_item, &receipt)?;
        Ok(Some(review_decision_result(statement, work_item, receipt.operation_id.clone(), receipt)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSubmissionReceipt {
    pub operation_id: String,
    pub statement_version: u64,
    pub work_item_id: String,
    pub task_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewDecisionReceipt {
    pub operation_id: String,
    pub result_status: dto::SettlementReviewDecisionStatus,
    pub statement_version: u64,
    pub task_version: u64,
    pub payable_account_id: Option<String>,
    pub cost_delta: Option<Amount>,
}

pub fn validate_settlement_review_work_item(
    item: &WorkItem,
    statement: &SupplierSettlementStatement,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor: &AuditActor,
) -> Result<()> {
    if item.base.version != expected_task_version {
        return Err(Error::ConflictError("复核任务责任或版本已变化，请刷新后重试".to_string()));
    }
    if expected_subject_version != statement.subject_hash || item.subject_version != statement.subject_hash {
        return Err(Error::ConflictError("结算复核主题已变化，请刷新后重试".to_string()));
    }
    if !statement.is_pending_review()
        || item.work_item_type != WorkItemType::SupplierSettlementReview
        || item.business_object_type != "supplier_settlement_statement"
        || item.business_object_id != statement.base.id
        || item.owner_role != SETTLEMENT_REVIEW_OWNER_ROLE
        || item.owner_organization_id != SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID
    {
        return Err(Error::BusinessLogicError("待办与当前供应商结算复核不匹配".to_string()));
    }
    if !item.is_owned_by(actor.id()) {
        return Err(Error::Forbidden("当前账号不是该复核任务责任人，或处理权已变化".to_string()));
    }
    Ok(())
}

/// 解析跨端字符串版本并拒绝零版本。
fn parse_expected_version(value: &str, field: &str) -> Result<u64> {
    let version =
        value.trim().parse::<u64>().map_err(|_| Error::ValidationError(format!("{field}必须是正整数")))?;
    if version == 0 {
        return Err(Error::ValidationError(format!("{field}必须大于0")));
    }
    Ok(version)
}

/// 计算提交复核命令指纹。
fn submit_review_fingerprint(req: &SubmitSettlementReviewRequest) -> String {
    digest_parts(&[
        "SUBMIT_REVIEW".to_string(),
        req.statement_id.clone(),
        req.expected_lock_version.to_string(),
        req.subject_hash.clone(),
        req.refresh_cutoff_policy_id.clone(),
        req.expected_refresh_cutoff_policy_version.clone(),
        req.reviewer_user_id.clone(),
        req.operation_id.clone(),
        req.comment.clone().unwrap_or_default(),
    ])
}

/// 计算正式复核决定命令指纹。
fn review_decision_fingerprint(req: &SettlementReviewCommand) -> String {
    let action = match req.decision.action {
        dto::SettlementReviewAction::Reject => "REJECT",
        dto::SettlementReviewAction::Confirm => "CONFIRM",
    };
    digest_parts(&[
        req.work_item_id.clone(),
        req.expected_task_version.trim().to_string(),
        req.expected_subject_version.clone(),
        req.decision.statement_id.clone(),
        req.decision.expected_lock_version.to_string(),
        action.to_string(),
        req.decision.operation_id.clone(),
        req.decision.reason_code.as_deref().map(str::trim).unwrap_or_default().to_ascii_uppercase(),
        req.decision.comment.clone().unwrap_or_default(),
    ])
}

/// 编码提交复核幂等收据。
pub fn review_submission_receipt_message(fingerprint: &str, receipt: &ReviewSubmissionReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}",
        receipt.operation_id, receipt.statement_version, receipt.work_item_id, receipt.task_version,
    )
}

/// 解析并校验提交复核幂等收据。
pub fn parse_review_submission_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ReviewSubmissionReceipt> {
    let result = receipt_result(message, expected_fingerprint, "提交复核")?;
    let fields = result.split('|').collect::<Vec<_>>();
    let [operation_id, statement_version, work_item_id, task_version] = fields.as_slice() else {
        return Err(Error::Internal("提交复核幂等收据结果非法".to_string()));
    };
    Ok(ReviewSubmissionReceipt {
        operation_id: (*operation_id).to_string(),
        statement_version: parse_receipt_number(statement_version, "结算单版本")?,
        work_item_id: (*work_item_id).to_string(),
        task_version: parse_receipt_number(task_version, "待办版本")?,
    })
}

/// 编码正式复核决定幂等收据。
pub fn review_decision_receipt_message(fingerprint: &str, receipt: &ReviewDecisionReceipt) -> String {
    let status = match receipt.result_status {
        dto::SettlementReviewDecisionStatus::Confirmed => "C",
        dto::SettlementReviewDecisionStatus::Rejected => "R",
    };
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{status}|{}|{}|{}|{}",
        receipt.operation_id,
        receipt.statement_version,
        receipt.task_version,
        receipt.payable_account_id.as_deref().unwrap_or("-"),
        receipt.cost_delta.map(|value| value.to_string()).unwrap_or_else(|| "-".to_string()),
    )
}

/// 解析并校验正式复核决定幂等收据。
pub fn parse_review_decision_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ReviewDecisionReceipt> {
    let result = receipt_result(message, expected_fingerprint, "结算复核决定")?;
    let fields = result.split('|').collect::<Vec<_>>();
    let [operation_id, status, statement_version, task_version, payable_account_id, cost_delta] =
        fields.as_slice()
    else {
        return Err(Error::Internal("结算复核决定幂等收据结果非法".to_string()));
    };
    let result_status = match *status {
        "C" => dto::SettlementReviewDecisionStatus::Confirmed,
        "R" => dto::SettlementReviewDecisionStatus::Rejected,
        _ => return Err(Error::Internal("结算复核决定收据状态非法".to_string())),
    };
    Ok(ReviewDecisionReceipt {
        operation_id: (*operation_id).to_string(),
        result_status,
        statement_version: parse_receipt_number(statement_version, "结算单版本")?,
        task_version: parse_receipt_number(task_version, "待办版本")?,
        payable_account_id: (*payable_account_id != "-").then(|| (*payable_account_id).to_string()),
        cost_delta: if *cost_delta == "-" {
            None
        } else {
            Some(
                Amount::from_str(cost_delta)
                    .map_err(|_| Error::Internal("结算复核决定收据成本差额非法".to_string()))?,
            )
        },
    })
}

/// 由正式事实与收据构造结算复核响应。
fn review_decision_result(
    statement: SupplierSettlementStatement,
    work_item: WorkItem,
    operation_id: String,
    receipt: ReviewDecisionReceipt,
) -> SettlementReviewDecisionResult {
    let payable_account_id = receipt.payable_account_id;
    let payable_no = payable_account_id.as_ref().map(|_| statement.statement_no.clone());
    SettlementReviewDecisionResult {
        result_status: receipt.result_status,
        message: match receipt.result_status {
            dto::SettlementReviewDecisionStatus::Confirmed => {
                "结算已确认，应付与成本差额已原子登记".to_string()
            },
            dto::SettlementReviewDecisionStatus::Rejected => "结算已驳回给经办人继续处理".to_string(),
        },
        operation_id,
        statement: statement.into(),
        work_item_id: work_item.base.id,
        work_item_status: WorkItemStatus::Completed,
        task_version: receipt.task_version,
        payable_no,
        payable_account_id,
        cost_delta_gross: receipt.cost_delta,
    }
}

/// 提交收据允许结算单推进；该检查仍先于正式任务读取。
fn ensure_submission_statement(
    statement: &SupplierSettlementStatement,
    receipt: &ReviewSubmissionReceipt,
) -> Result<()> {
    if statement.base.version < receipt.statement_version {
        return Err(Error::ConflictError("提交复核幂等收据与当前结算事实不一致".to_string()));
    }
    Ok(())
}
/// 提交收据允许任务推进，保持当前归属与主题校验。
fn ensure_submission_task(
    statement: &SupplierSettlementStatement,
    work_item: &WorkItem,
    receipt: &ReviewSubmissionReceipt,
) -> Result<()> {
    if work_item.base.version < receipt.task_version
        || work_item.work_item_type != WorkItemType::SupplierSettlementReview
        || work_item.business_object_type != "supplier_settlement_statement"
        || work_item.business_object_id != statement.base.id
        || work_item.subject_version != statement.subject_hash
        || work_item.owner_role != SETTLEMENT_REVIEW_OWNER_ROLE
        || work_item.owner_organization_id != SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID
    {
        return Err(Error::ConflictError("提交复核幂等收据与当前正式任务不一致".to_string()));
    }
    Ok(())
}
/// 决定收据要求精确版本和正式完成状态，不接受后续业务状态。
fn ensure_decision_replay(
    statement: &SupplierSettlementStatement,
    work_item: &WorkItem,
    receipt: &ReviewDecisionReceipt,
) -> Result<()> {
    let business_result_matches = match receipt.result_status {
        dto::SettlementReviewDecisionStatus::Confirmed => {
            statement.status == SettlementStatus::Confirmed
                && statement.review_result == Some(SettlementReviewResult::Confirmed)
                && statement.payable_account_id.as_ref().map(ToString::to_string)
                    == receipt.payable_account_id
        },
        dto::SettlementReviewDecisionStatus::Rejected => {
            matches!(statement.status, SettlementStatus::Draft | SettlementStatus::HasDifference)
                && statement.review_result == Some(SettlementReviewResult::Rejected)
                && statement.payable_account_id.is_none()
                && receipt.payable_account_id.is_none()
        },
    };
    if statement.base.version != receipt.statement_version
        || work_item.base.version != receipt.task_version
        || work_item.status != WorkItemStatus::Completed
        || work_item.work_item_type != WorkItemType::SupplierSettlementReview
        || work_item.business_object_type != "supplier_settlement_statement"
        || work_item.business_object_id != statement.base.id
        || work_item.subject_version != statement.subject_hash
        || !business_result_matches
    {
        return Err(Error::ConflictError("复核决定幂等收据与当前正式事实不一致".to_string()));
    }
    Ok(())
}
#[cfg(test)]
mod replay_tests {
    use super::super::tests::{sample_statement, sample_work_item};
    use super::*;
    #[test]
    fn submission_receipt_allows_equal_or_later_versions_but_preserves_subject() {
        let mut statement = sample_statement();
        let mut task = sample_work_item(&statement);
        let receipt = ReviewSubmissionReceipt {
            operation_id: "op-1".into(),
            statement_version: 2,
            work_item_id: task.base.id.clone(),
            task_version: 2,
        };
        for version in [1, 2, 3] {
            statement.base.version = version;
            task.base.version = version;
            assert_eq!(ensure_submission_statement(&statement, &receipt).is_ok(), version >= 2);
            assert_eq!(ensure_submission_task(&statement, &task, &receipt).is_ok(), version >= 2);
        }
        task.subject_version = "c".repeat(64);
        assert!(ensure_submission_task(&statement, &task, &receipt).is_err());
    }
    #[test]
    fn decision_receipt_requires_exact_versions_completion_and_business_result() {
        let mut statement = sample_statement();
        let mut task = sample_work_item(&statement);
        statement.status = SettlementStatus::Confirmed;
        statement.review_result = Some(SettlementReviewResult::Confirmed);
        statement.payable_account_id = Some(erp_core::ids::PayableAccountId::new("payable-1"));
        task.status = WorkItemStatus::Completed;
        let receipt = ReviewDecisionReceipt {
            operation_id: "op-1".into(),
            result_status: dto::SettlementReviewDecisionStatus::Confirmed,
            statement_version: 2,
            task_version: 2,
            payable_account_id: Some("payable-1".into()),
            cost_delta: None,
        };
        for statement_version in [1, 2, 3] {
            for task_version in [1, 2, 3] {
                statement.base.version = statement_version;
                task.base.version = task_version;
                assert_eq!(
                    ensure_decision_replay(&statement, &task, &receipt).is_ok(),
                    statement_version == 2 && task_version == 2
                );
            }
        }
        statement.base.version = 2;
        task.base.version = 2;
        task.status = WorkItemStatus::Open;
        assert!(ensure_decision_replay(&statement, &task, &receipt).is_err());
        task.status = WorkItemStatus::Completed;
        statement.payable_account_id = None;
        assert!(ensure_decision_replay(&statement, &task, &receipt).is_err());
    }
}
