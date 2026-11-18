use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::{persist_log, recover_command};
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_fulfillment_execution::supply_execution::receipt::persist_supply_receipt;
use erp_identity::AuthorizationService;
use erp_read_models::supplier_center::settlement::require_reviewer;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{ReviewDecisionReceipt, ReviewSubmissionReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_settlement::{
    SettlementReviewRejectReason, SettlementReviewResult, SettlementStatus, SupplierSettlementStatement,
};
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::review::{
    ensure_current_subject_and_resolved_differences, ensure_review_submission_ready,
    review_owner_organization_id, review_task_identity_matches, validate_review_submission_snapshot,
};
use erp_supply::service::supplier_settlement::{SettlementAction, SupplierSettlementService};
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::{WorkItemExt, WorkItemService};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::review_posting::{self, Posting};
use super::review_preparation::{self, PreparedReview, ReviewInput};
use super::{
    SETTLEMENT_REVIEW_OWNER_ROLE, SettlementReviewCommand, SettlementReviewDecisionResult,
    SubmitSettlementReviewRequest, SubmitSettlementReviewResult, SupplierSettlementProcess, command_audit_id,
    digest_parts, dto, ensure_same_id, reviewers,
};
use crate::{Error, Result};

struct PreparedSubmission {
    statement: SupplierSettlementStatement,
    work_item: WorkItem,
}

struct PreparedDecision {
    statement: SupplierSettlementStatement,
    work_item: WorkItem,
    financial: PreparedReview,
}

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 提交冻结结算主题供财务复核，重验单据版本、主题摘要、刷新截止策略、完整差异结论和复核人资格。
    ///
    /// 同一事务将结算单置为待复核，创建唯一 `SUPPLIER_SETTLEMENT_REVIEW` 任务并登记审计；重放返回原结算单与任务，身份、范围或冻结条件不符时拒绝。
    pub async fn submit_review(
        &self,
        id: &str,
        req: SubmitSettlementReviewRequest,
        actor: &AuditActor,
    ) -> Result<SubmitSettlementReviewResult> {
        let process = self.clone();
        let command = req.clone();
        let writer = actor.clone();
        let id = id.to_owned();
        let write_id = id.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { process.submit_review_apply(&write_id, command, &writer, executor).await },
                )
            })
            .await;

        let Err(error) = result else { return result };
        let process = self.clone();
        let actor = actor.clone();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let fingerprint = submit_review_fingerprint(&req);
                    let audit_id = command_audit_id(
                        actor.id(),
                        "supplier_settlement.submit_review",
                        &id,
                        &req.idempotency_key,
                    );

                    process
                        .data_scope
                        .require_action(&actor, SettlementAction::Submit, &id, executor)
                        .await?;
                    process
                        .replay_review_submission(
                            &audit_id,
                            &fingerprint,
                            &id,
                            (actor.id(), &req.idempotency_key),
                            executor,
                        )
                        .await
                })
            })
            .await;
        recover_command(error, recovered)
    }

    // 在提交事务内先核验经办权限和原回执，再准备冻结结算主题及唯一财务待办，避免重试创建第二项任务。
    async fn submit_review_apply(
        &self,
        id: &str,
        req: SubmitSettlementReviewRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SubmitSettlementReviewResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        ensure_same_id(id, &req.statement_id, "结算单")?;

        self.domain().access().require_action(actor, SettlementAction::Submit, id, executor).await?;

        let fingerprint = submit_review_fingerprint(&req);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.submit_review", id, &req.idempotency_key);
        if let Some(result) = self
            .replay_review_submission(&audit_id, &fingerprint, id, (actor.id(), &replay_key), executor)
            .await?
        {
            return Ok(result);
        }

        let statement = self.domain().load_statement(id, executor).await?;
        validate_submission_command(&statement, &req)?;

        reviewers::ensure_reviewer(
            &self.authorization,
            &self.work_items.identity,
            &req.reviewer_user_id,
            actor.id(),
            executor,
        )
        .await?;

        let work_item =
            create_review_work_item(WorkItemId::new(next_id()), &statement, &req.reviewer_user_id)?;
        let (statement, work_item) = self
            .apply_review_submission(
                PreparedSubmission { statement, work_item },
                &req,
                actor,
                (&audit_id, &fingerprint),
                executor,
            )
            .await?;

        Ok(SubmitSettlementReviewResult {
            result_status: dto::SettlementReviewSubmissionStatus::Submitted,
            message: "结算单已提交财务复核，复核期间不可编辑".to_string(),
            operation_id: req.operation_id,
            statement: statement.into(),
            work_item_id: work_item.base.id,
        })
    }

    // 保存前用原执行器重验主题、差异和指定复核人资格，使正式待办始终对应本次有效的冻结资料。
    async fn apply_review_submission(
        &self,
        prepared: PreparedSubmission,
        request: &SubmitSettlementReviewRequest,
        actor: &AuditActor,
        (command_id, fingerprint): (&str, &str),
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, WorkItem)> {
        let statement_id = &prepared.statement.base.id;
        self.data_scope.require_action(actor, SettlementAction::Submit, statement_id, executor).await?;
        let domain = SupplierSettlementService::new(self.db.clone());
        let mut current = domain.load_statement(statement_id, executor).await?;
        if current.base.version != prepared.statement.base.version
            || current.subject_hash != prepared.statement.subject_hash
        {
            return Err(Error::ConflictError("结算单版本或主题已变化，请刷新后重试".to_string()));
        }
        ensure_review_submission_ready(&self.db, &current, executor).await?;
        reviewers::ensure_reviewer(
            &self.authorization,
            &self.work_items.identity,
            &request.reviewer_user_id,
            &current.prepared_by,
            executor,
        )
        .await?;

        current.submit_review()?;
        domain.persist_statement(&mut current, executor).await?;
        self.db.work_items().create(&prepared.work_item, executor).await?;

        persist_review_submission(
            ReviewSubmissionWrite {
                db: &self.db,
                actor,
                statement: &current,
                work_item: &prepared.work_item,
                command_id,
                fingerprint,
                request,
            },
            executor,
        )
        .await?;
        Ok((current, prepared.work_item))
    }

    /// 依据正式复核任务确认或驳回供应商结算。两种决定都校验任务与单据版本、主题摘要、当前责任、财务权限、组织范围和岗位分离。
    ///
    /// 决定、任务完成和审计同事务登记；确认还写入应付和成本差额。重试只恢复仍与回执一致的结果，不重复过账。
    pub async fn decide_review(
        &self,
        id: &str,
        req: SettlementReviewCommand,
        actor: &AuditActor,
    ) -> Result<SettlementReviewDecisionResult> {
        let process = self.clone();
        let command = req.clone();
        let writer = actor.clone();
        let id = id.to_owned();
        let write_id = id.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { process.decide_review_apply(&write_id, command, &writer, executor).await },
                )
            })
            .await;

        let Err(error) = result else { return result };
        let process = self.clone();
        let actor = actor.clone();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let action = review_action(req.decision.action);
                    let fingerprint = review_decision_fingerprint(&req);
                    let audit_id = command_audit_id(actor.id(), action, &id, &req.idempotency_key);
                    process
                        .replay_review_decision(
                            &audit_id,
                            &fingerprint,
                            (&id, &req.work_item_id),
                            &actor,
                            (&req.idempotency_key, action),
                            executor,
                        )
                        .await
                })
            })
            .await;
        recover_command(error, recovered)
    }

    // 在决定事务内核对正式任务及操作者，命中原回执则返回，否则准备决定和财务数据后调用过账，避免重复确认。
    async fn decide_review_apply(
        &self,
        id: &str,
        req: SettlementReviewCommand,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SettlementReviewDecisionResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        ensure_same_id(id, &req.decision.statement_id, "结算单")?;
        let reject_reason = req.decision.parsed_reject_reason()?;
        let expected_task_version = parse_expected_task_version(&req.expected_task_version)?;

        let action = review_action(req.decision.action);
        let fingerprint = review_decision_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), action, id, &req.idempotency_key);
        if let Some(result) = self
            .replay_review_decision(
                &audit_id,
                &fingerprint,
                (id, &req.work_item_id),
                actor,
                (&replay_key, action),
                executor,
            )
            .await?
        {
            return Ok(result);
        }

        let prepared =
            self.prepare_decision(id, &req, actor, expected_task_version, reject_reason, executor).await?;

        let operation_id = req.decision.operation_id.clone();
        let input = DecisionCommit::new(self, &req, actor, prepared, (&audit_id, &fingerprint, action));
        let (statement, work_item, receipt) = input.persist(executor).await?;
        Ok(review_decision_result(statement, work_item, operation_id, receipt))
    }

    // 按结算单、正式任务、明细和差异的顺序读取准备资料，构造复核决定及财务结果供同事务写入。
    async fn prepare_decision(
        &self,
        id: &str,
        req: &SettlementReviewCommand,
        actor: &AuditActor,
        expected_task_version: u64,
        reject_reason: Option<SettlementReviewRejectReason>,

        executor: &mut dyn Executor,
    ) -> Result<PreparedDecision> {
        let (mut statement, mut work_item) =
            self.load_review_decision_state(id, req, actor, expected_task_version, executor).await?;
        let items = self.domain().load_statement_items(id, executor).await?;
        let differences = self.domain().load_statement_differences(&items, executor).await?;
        ensure_current_subject_and_resolved_differences(&statement, &differences)?;

        let now = Instant::now();
        let financial = review_preparation::prepare(
            &mut statement,
            &mut work_item,
            &items,
            &differences,
            ReviewInput { request: req, reject_reason, actor_id: actor.id(), at: now },
        )?;
        Ok(PreparedDecision { statement, work_item, financial })
    }

    // 先校验结算单版本，再读取和校验正式复核任务，避免使用未绑定该单或已变化的任务执行决定。
    async fn load_review_decision_state(
        &self,
        id: &str,
        req: &SettlementReviewCommand,
        actor: &AuditActor,
        expected_task_version: u64,

        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, WorkItem)> {
        let statement = self.domain().load_statement(id, executor).await?;
        statement
            .ensure_version(req.decision.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;

        let work_item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算复核待办不存在".to_string()))?;
        validate_settlement_review_work_item(
            &work_item,
            &statement,
            expected_task_version,
            &req.expected_subject_version,
        )?;
        require_reviewer(&self.db, &self.authorization, actor, &work_item, executor).await?;
        Ok((statement, work_item))
    }

    // 核验提交复核回执的原身份和命令指纹，恢复关联的结算单与任务，不重复冻结主题或创建待办。
    async fn replay_review_submission(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
        identity: (&str, &str),

        executor: &mut dyn Executor,
    ) -> Result<Option<SubmitSettlementReviewResult>> {
        let Some(stored) = self.db.supply_command_receipts().find_command(audit_id, executor).await? else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            identity.0,
            "supplier_settlement.submit_review",
            statement_id,
            &stable_digest(identity.1.trim()),
        )?;
        stored.verify(
            expected_fingerprint,
            Some(statement_id),
            "本次操作编号已用于其他结算复核提交，请核对原提交记录",
        )?;
        let SupplyCommandResult::ReviewSubmission(receipt) = stored.result else {
            return Err(Error::Internal("提交复核回执类型非法".to_string()));
        };

        let statement = self.domain().load_statement(statement_id, executor).await?;
        ensure_submission_statement(&statement, &receipt)?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(&receipt.work_item_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("提交复核收据引用的任务不存在".to_string()))?;
        ensure_submission_task(&statement, &work_item, &receipt)?;

        Ok(Some(SubmitSettlementReviewResult {
            result_status: dto::SettlementReviewSubmissionStatus::Submitted,
            message: "结算单已提交财务复核，复核期间不可编辑".to_string(),
            operation_id: receipt.operation_id,
            statement: statement.into(),
            work_item_id: receipt.work_item_id,
        }))
    }

    // 核对原回执身份及请求指纹，重验复核资格与制单复核分离；只有当前事实仍匹配已完成决定时才恢复，不重复过账。
    async fn replay_review_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        targets: (&str, &str),
        actor: &AuditActor,
        request: (&str, &str),

        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementReviewDecisionResult>> {
        let (statement_id, work_item_id) = targets;
        let Some(stored) = self.db.supply_command_receipts().find_command(audit_id, executor).await? else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            actor.id(),
            request.1,
            statement_id,
            &stable_digest(request.0.trim()),
        )?;
        stored.verify(
            expected_fingerprint,
            Some(statement_id),
            "本次操作编号已用于其他结算复核，请核对原复核结果",
        )?;
        let SupplyCommandResult::ReviewDecision(receipt) = stored.result else {
            return Err(Error::Internal("结算复核决定回执类型非法".to_string()));
        };

        let statement = self.domain().load_statement(statement_id, executor).await?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("复核决定收据引用的任务不存在".to_string()))?;

        if statement.is_prepared_by(actor.id()) {
            return Err(Error::Forbidden("所选人员不能复核本单，请重新选择".into()));
        }
        require_reviewer(&self.db, &self.authorization, actor, &work_item, executor).await?;
        ensure_decision_replay(&statement, &work_item, &receipt)?;

        Ok(Some(review_decision_result(statement, work_item, receipt.operation_id.clone(), receipt)))
    }
}

struct DecisionCommit<A: WorkflowAuthorizationPort> {
    db: Database,
    actor: AuditActor,
    work_items: WorkItemService<A>,
    authorization: Arc<AuthorizationService>,
    prepared: PreparedDecision,
    operation_id: String,
    fingerprint: String,
    command_id: String,
    idempotency_key: String,
    action: String,
}

impl<A: WorkflowAuthorizationPort> DecisionCommit<A> {
    fn new(
        process: &SupplierSettlementProcess<A>,
        request: &SettlementReviewCommand,
        actor: &AuditActor,
        prepared: PreparedDecision,
        identity: (&str, &str, &str),
    ) -> Self {
        Self {
            db: process.db.clone(),
            actor: actor.clone(),
            work_items: process.work_items.clone(),
            authorization: process.authorization.clone(),
            prepared,
            operation_id: request.decision.operation_id.clone(),
            fingerprint: identity.1.to_string(),
            command_id: identity.0.to_string(),
            idempotency_key: request.idempotency_key.clone(),
            action: identity.2.to_string(),
        }
    }

    // 沿用复核入口的执行器执行资格重验、事实重验和财务过账，使决定与任务、财务结果同事务提交。
    async fn persist(
        mut self,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, WorkItem, ReviewDecisionReceipt)> {
        let receipt = review_posting::post(
            Posting {
                db: &self.db,
                authorization: &self.authorization,
                actor: &self.actor,
                actor_id: self.actor.id(),
                work_items: &self.work_items,
                statement: &mut self.prepared.statement,
                work_item: &mut self.prepared.work_item,
                payable: self.prepared.financial.payable.as_ref(),
                payable_entry: self.prepared.financial.payable_entry.as_ref(),
                cost_entries: &self.prepared.financial.cost_entries,
                cost_delta: self.prepared.financial.cost_delta,
                result_status: self.prepared.financial.result_status,
                operation_id: self.operation_id,
                fingerprint: self.fingerprint,
                audit_id: self.command_id,
                idempotency_key: self.idempotency_key,
                action: self.action,
            },
            executor,
        )
        .await?;
        Ok((self.prepared.statement, self.prepared.work_item, receipt))
    }
}

// 按冻结主题构造结算财务待办，任务必须归属单据的内部组织；公司根不能作为任务责任组织。
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

/// 执行决定前确认待办仍开放，任务版本、冻结主题和责任角色及组织仍与当前待复核单据匹配。
///
/// 任务版本、主题变化或待办已结束返回 `ConflictError`；绑定或结算状态不符返回 `BusinessLogicError`。
/// 操作者的办理资格须另行校验。
pub fn validate_settlement_review_work_item(
    item: &WorkItem,
    statement: &SupplierSettlementStatement,
    expected_task_version: u64,
    expected_subject_version: &str,
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
    if item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("复核任务已结束，请刷新后重试".into()));
    }
    Ok(())
}

// 将客户端任务版本解析为正整数，拒绝零或非法值，以便后续版本校验不接受占位值。
fn parse_expected_task_version(value: &str) -> Result<u64> {
    let invalid = || Error::ValidationError("复核待办版本无效，请刷新后重新打开待办".into());
    let version = value.trim().parse::<u64>().map_err(|_| invalid())?;
    if version == 0 {
        return Err(invalid());
    }
    Ok(version)
}

// 用冻结主题、版本、复核人和操作字段生成提交指纹，供回执区分同键重试与载荷变更。
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

// 为复核任务、决定及其版本生成稳定指纹，规范化版本文本和原因标识，供回执检测同键不同决定。
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

// 从结算单、已完成任务和原决定回执组装响应，使应付身份、成本差额及任务版本与实际决定一致。
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

// 回放复核提交时先排除早于回执的结算单版本，再读取正式任务；允许结算单在原提交后继续推进。
fn ensure_submission_statement(
    statement: &SupplierSettlementStatement,
    receipt: &ReviewSubmissionReceipt,
) -> Result<()> {
    if statement.base.version < receipt.statement_version {
        return Err(Error::ConflictError("原复核提交记录与当前结算资料不符，请核对结算单详情".to_string()));
    }
    Ok(())
}
// 校验任务仍归属回执对应结算单和冻结主题，并允许任务版本在提交后向前推进。
fn ensure_submission_task(
    statement: &SupplierSettlementStatement,
    work_item: &WorkItem,
    receipt: &ReviewSubmissionReceipt,
) -> Result<()> {
    if work_item.base.version < receipt.task_version
        || !review_work_item_binds_statement(work_item, statement)
    {
        return Err(Error::ConflictError("原复核提交记录与当前复核待办不符，请刷新后核对待办".to_string()));
    }
    Ok(())
}
// 只有结算单、已完成任务及财务结果仍与决定回执精确一致才可回放；拒绝后续业务状态冒充原结果。
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
        return Err(Error::ConflictError("原复核结果与当前结算资料不符，请核对结算单详情".to_string()));
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

// 结算单和待办写入后，用原事务保存首次提交的版本、待办引用和单次审计，为幂等回放提供依据。
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
            Some("结算单已提交财务复核，复核期间不可编辑".to_string()),
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
    .await
    .map_err(Error::from)?;
    persist_log(input.db, &audit, executor).await?;
    Ok(())
}

// 先校验结算单版本，再验证冻结主题及提交前置事实，避免用过期草稿建立复核任务。
fn validate_submission_command(
    statement: &SupplierSettlementStatement,
    req: &SubmitSettlementReviewRequest,
) -> Result<()> {
    statement
        .ensure_version(req.expected_lock_version)
        .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
    validate_review_submission_snapshot(statement, req)?;
    Ok(())
}

// 把确认和驳回映射到各自登记的动作码，供审计和回执准确区分正式决定。
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
