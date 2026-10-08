use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{ReviewDecisionReceipt, ReviewSubmissionReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_settlement::{
    SettlementReviewResult, SettlementStatus, SupplierSettlementStatement,
};
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::review::{
    ensure_current_subject_and_resolved_differences, ensure_review_submission_ready,
    review_owner_organization_id, review_task_identity_matches, validate_review_submission_snapshot,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::{
    SETTLEMENT_REVIEW_OWNER_ROLE, SettlementReviewCommand, SettlementReviewDecisionResult,
    SubmitSettlementReviewRequest, SubmitSettlementReviewResult, SupplierSettlementProcess, command_audit_id,
    digest_parts, dto, ensure_same_id,
};
use crate::audit::{persist_log, recover_command};
use crate::supply_execution::receipt::persist_supply_receipt;
use crate::{Error, Result};

impl SupplierSettlementProcess {
    /// 提交冻结结算主题并原子创建唯一财务复核任务。
    ///
    /// 命令重验结算单版本、服务端主题摘要、刷新截止策略与完整差异结论；结算单
    /// 进入待复核、`SUPPLIER_SETTLEMENT_REVIEW` 任务和幂等审计在同一事务写入。
    ///
    /// # 参数
    /// * `id` - 路径中的结算单 ID。
    /// * `req` - 提交复核命令。
    /// * `actor` - 当前经办人。
    ///
    /// # 返回
    /// 返回已提交的结算单与新建复核任务；同一命令重放返回原结果。
    ///
    /// # 错误
    /// 路径身份、版本、主题、策略或差异状态不一致时失败关闭。请求校验、范围、复核人资格或事务写入失败时返回对应错误。
    pub async fn submit_review(
        &self,
        id: &str,
        req: SubmitSettlementReviewRequest,
        actor: &AuditActor,
    ) -> Result<SubmitSettlementReviewResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        ensure_same_id(id, &req.statement_id, "结算单")?;
        self.domain().access().require_statement(actor, "submit", id, &mut NoTransaction).await?;
        let fingerprint = submit_review_fingerprint(&req);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.submit_review", id, &req.idempotency_key);
        if let Some(result) =
            self.replay_review_submission(&audit_id, &fingerprint, id, (actor.id(), &replay_key)).await?
        {
            return Ok(result);
        }
        let statement =
            self.domain().access().require_statement(actor, "submit", id, &mut NoTransaction).await?;
        validate_submission_command(&statement, &req, actor)?;
        let auth = crate::adapters::workflow::workflow_auth(
            self.db.clone(),
            crate::adapters::identity::shared_rbac_service(self.db.clone()),
        );
        let policy_revision = self
            .authorize_reviewer(&auth, &req.reviewer_user_id, actor.id(), &statement.business_org_unit_id)
            .await?;
        let work_item =
            create_review_work_item(WorkItemId::new(next_id()), &statement, &req.reviewer_user_id)?;
        let db = self.db.clone();
        let data_scope = self.data_scope.clone();
        let actor_id = actor.id().to_string();
        let expected_subject_hash = statement.subject_hash.clone();
        let audit_actor = actor.clone();
        let operation_id = req.operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let auth_for_tx = auth.clone();
        let transaction_result = auth
            .run_authorized_policy_transaction(policy_revision, move |executor| {
                Box::pin(async move {
                    let mut current = erp_supply::SettlementAccess::new(db.clone(), data_scope)
                        .require_statement(&audit_actor, "submit", &statement.base.id, executor)
                        .await?;
                    if current.base.version != statement.base.version
                        || current.subject_hash != expected_subject_hash
                    {
                        return Err(Error::ConflictError("结算单版本或主题已变化，请刷新后重试".to_string()));
                    }
                    ensure_review_submission_ready(&db, &current, &actor_id, executor).await?;
                    super::reviewers::ensure_reviewer(
                        &auth_for_tx,
                        &req.reviewer_user_id,
                        &current.prepared_by,
                        &current.business_org_unit_id,
                        executor,
                    )
                    .await?;
                    current.submit_review()?;
                    SupplierSettlementService::new(db.clone())
                        .persist_statement(&mut current, executor)
                        .await?;
                    db.work_items().create(&work_item, executor).await?;
                    persist_review_submission(
                        ReviewSubmissionWrite {
                            db: &db,
                            actor: &audit_actor,
                            statement: &current,
                            work_item: &work_item,
                            command_id: &audit_id_for_tx,
                            fingerprint: &fingerprint_for_tx,
                            request: &req,
                        },
                        executor,
                    )
                    .await?;
                    Ok::<(SupplierSettlementStatement, WorkItem), crate::Error>((current, work_item))
                })
            })
            .await;
        let (statement, work_item) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                return recover_command(
                    error,
                    self.replay_review_submission(&audit_id, &fingerprint, id, (actor.id(), &replay_key))
                        .await,
                );
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
    /// # 参数
    /// * `id` - 路径中的结算单 ID。
    /// * `req` - 复核决定命令。
    /// * `actor` - 当前复核人。
    ///
    /// # 返回
    /// 返回复核决定结果；同一命令重放返回原结果。
    ///
    /// # 错误
    /// 任务、责任、主题、结算版本或正式业务前置条件不一致时失败关闭。请求校验或事务写入失败时返回对应错误。
    pub async fn decide_review(
        &self,
        id: &str,
        req: SettlementReviewCommand,
        actor: &AuditActor,
    ) -> Result<SettlementReviewDecisionResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        ensure_same_id(id, &req.decision.statement_id, "结算单")?;
        let reject_reason = req.decision.parsed_reject_reason()?;
        let expected_task_version = parse_expected_version(&req.expected_task_version, "待办版本")?;
        let action = review_action(req.decision.action);
        let fingerprint = review_decision_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), action, id, &req.idempotency_key);
        if let Some(result) = self
            .replay_review_decision(
                &audit_id,
                &fingerprint,
                id,
                &req.work_item_id,
                actor,
                (&replay_key, action),
            )
            .await?
        {
            return Ok(result);
        }
        let (mut statement, mut work_item) =
            self.load_review_decision_state(id, &req, actor, expected_task_version).await?;
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
            .with_transaction(move |executor| {
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
                            idempotency_key: req.idempotency_key.clone(),
                            action: action_for_tx,
                        },
                        executor,
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
                return recover_command(
                    error,
                    self.replay_review_decision(
                        &audit_id,
                        &fingerprint,
                        id,
                        &req.work_item_id,
                        actor,
                        (&replay_key, action),
                    )
                    .await,
                );
            },
        };
        Ok(review_decision_result(statement, work_item, operation_id, receipt))
    }

    /// 按原顺序读取并验证决定所针对的结算单与正式任务。
    async fn load_review_decision_state(
        &self,
        id: &str,
        req: &SettlementReviewCommand,
        actor: &AuditActor,
        expected_task_version: u64,
    ) -> Result<(SupplierSettlementStatement, WorkItem)> {
        let statement = self.domain().load_statement(id, &mut NoTransaction).await?;
        statement
            .ensure_version(req.decision.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        let work_item = self
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
        Ok((statement, work_item))
    }

    /// 重放提交复核命令并校验收据载荷。
    async fn replay_review_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
        identity: (&str, &str),
    ) -> Result<Option<SubmitSettlementReviewResult>> {
        let Some(stored) =
            self.db.supply_command_receipts().find_command(audit_id, &mut NoTransaction).await?
        else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            identity.0,
            "supplier_settlement.submit_review",
            statement_id,
            &stable_digest(identity.1.trim()),
        )?;
        stored.verify(expected_fingerprint, Some(statement_id), "幂等键已用于不同的提交复核命令")?;
        let SupplyCommandResult::ReviewSubmission(receipt) = stored.result else {
            return Err(Error::Internal("提交复核回执类型非法".to_string()));
        };
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
        actor: &AuditActor,
        request: (&str, &str),
    ) -> Result<Option<SettlementReviewDecisionResult>> {
        let Some(stored) =
            self.db.supply_command_receipts().find_command(audit_id, &mut NoTransaction).await?
        else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            actor.id(),
            request.1,
            statement_id,
            &stable_digest(request.0.trim()),
        )?;
        stored.verify(expected_fingerprint, Some(statement_id), "幂等键已用于不同的结算复核决定命令")?;
        let SupplyCommandResult::ReviewDecision(receipt) = stored.result else {
            return Err(Error::Internal("结算复核决定回执类型非法".to_string()));
        };
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("复核决定收据引用的任务不存在".to_string()))?;
        if work_item.owner_user_id.as_deref() != Some(actor.id()) {
            return Err(Error::Forbidden("仅原复核人可重放决定".into()));
        }
        let auth = crate::adapters::workflow::workflow_auth(
            self.db.clone(),
            crate::adapters::identity::shared_rbac_service(self.db.clone()),
        );
        super::reviewers::ensure_reviewer(
            &auth,
            actor.id(),
            &statement.prepared_by,
            &statement.business_org_unit_id,
            &mut NoTransaction,
        )
        .await?;
        ensure_decision_replay(&statement, &work_item, &receipt)?;
        Ok(Some(review_decision_result(statement, work_item, receipt.operation_id.clone(), receipt)))
    }
}

/// 创建绑定结算单内部组织的正式复核任务；拒绝公司根。
fn create_review_work_item(
    work_item_id: WorkItemId,
    statement: &SupplierSettlementStatement,
    reviewer_user_id: &str,
) -> Result<WorkItem> {
    Ok(WorkItem::new(
        work_item_id,
        WorkItemData {
            work_item_type: WorkItemType::SupplierSettlementReview,
            business_object_type: "supplier_settlement_statement".to_string(),
            business_object_id: statement.base.id.clone(),
            subject_version: statement.subject_hash.clone(),
            owner_role: SETTLEMENT_REVIEW_OWNER_ROLE.to_string(),
            owner_organization_id: review_owner_organization_id(statement)?.to_string(),
            owner_user_id: reviewer_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some("supplier_settlement_review_dispatched".to_string()),
            impact_summary: Some(format!("复核供应商结算单 {}", statement.statement_no)),
        },
    )?)
}

fn review_work_item_binds_statement(item: &WorkItem, statement: &SupplierSettlementStatement) -> bool {
    item.work_item_type == WorkItemType::SupplierSettlementReview
        && item.business_object_type == "supplier_settlement_statement"
        && item.business_object_id == statement.base.id
        && item.subject_version == statement.subject_hash
        && review_task_identity_matches(&item.owner_role, &item.owner_organization_id, statement)
}

/// 校验结算复核任务仍绑定当前结算单、版本和责任人。
///
/// # 参数
/// * `item` - 正式复核任务。
/// * `statement` - 当前结算单。
/// * `expected_task_version` - 命令锁定的任务版本。
/// * `expected_subject_version` - 命令锁定的主题摘要。
/// * `actor` - 当前复核人。
///
/// # 返回
/// 绑定、版本和责任一致时无返回值。
///
/// # 错误
/// 任务版本或主题变化时返回 `ConflictError`。任务与结算单不匹配或结算单不在待复核时返回 `BusinessLogicError`。当前账号不是责任人时返回 `Forbidden`。
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
    if !statement.is_pending_review() || !review_work_item_binds_statement(item, statement) {
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
        || !review_work_item_binds_statement(work_item, statement)
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

/// 冻结主题与正式复核任务写入之后的回执输入。
struct ReviewSubmissionWrite<'a> {
    db: &'a Database,
    actor: &'a AuditActor,
    statement: &'a SupplierSettlementStatement,
    work_item: &'a WorkItem,
    command_id: &'a str,
    fingerprint: &'a str,
    request: &'a SubmitSettlementReviewRequest,
}

/// 保存首次复核提交的版本、任务引用及同事务单次事件。
async fn persist_review_submission(
    input: ReviewSubmissionWrite<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let receipt = ReviewSubmissionReceipt {
        operation_id: input.request.operation_id.clone(),
        statement_version: input.statement.base.version,
        work_item_id: input.work_item.base.id.clone(),
        task_version: input.work_item.base.version,
    };
    let audit = input
        .actor
        .clone()
        .resource_log_with_id(
            input.command_id.to_string(),
            "supplier_settlement.submit_review",
            "supplier_settlement_statement",
            input.statement.base.id.clone(),
            Some("结算主题已冻结并提交财务复核".to_string()),
        )?
        .with_command_id(Some(input.command_id.to_string()))?
        .with_resource_number(Some(input.statement.statement_no.clone()))?;
    persist_supply_receipt(
        input.db,
        &audit,
        input.fingerprint,
        &input.request.idempotency_key,
        &input.statement.base.id,
        SupplyCommandResult::ReviewSubmission(receipt),
        executor,
    )
    .await?;
    persist_log(input.db, &audit, executor).await?;
    Ok(())
}

/// 提交前按原顺序验证负责人、结算版本和冻结主题。
fn validate_submission_command(
    statement: &SupplierSettlementStatement,
    req: &SubmitSettlementReviewRequest,
    actor: &AuditActor,
) -> Result<()> {
    if !statement.is_prepared_by(actor.id()) {
        return Err(Error::Forbidden("只有当前对账负责人可以提交财务复核".to_string()));
    }
    statement
        .ensure_version(req.expected_lock_version)
        .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
    validate_review_submission_snapshot(statement, req)?;
    Ok(())
}

/// 将正式复核决定映射为同一已登记动作代码。
fn review_action(action: dto::SettlementReviewAction) -> &'static str {
    match action {
        dto::SettlementReviewAction::Reject => "supplier_settlement.review_reject",
        dto::SettlementReviewAction::Confirm => "supplier_settlement.review_confirm",
    }
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
    fn review_work_item_uses_statement_org_and_rejects_company() {
        let statement = sample_statement();
        let item = create_review_work_item(WorkItemId::new("work-item-1"), &statement, "reviewer-1").unwrap();
        assert_eq!(item.owner_organization_id, "org-finance");
        assert_eq!(item.owner_role, SETTLEMENT_REVIEW_OWNER_ROLE);
        assert!(review_work_item_binds_statement(&item, &statement));
        let mut company = sample_statement();
        company.business_org_unit_id = "company".into();
        assert!(create_review_work_item(WorkItemId::new("work-item-2"), &company, "reviewer-1").is_err());
        let mut mismatched = sample_work_item(&statement);
        mismatched.owner_organization_id = "company".into();
        assert!(!review_work_item_binds_statement(&mismatched, &statement));
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
