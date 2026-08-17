//! `LOW_MARGIN_MANAGER_CONFIRMATION` 的唯一强类型领域决定。

use database::{
    AccessControlExt, DocumentRegistryExt, NoTransaction, SalesOrderExt, SalesReviewExt, Transactional,
    WorkItemExt,
};
use entities::common::time::Instant;
use entities::document_registry::{WorkflowAction, WorkflowActionData, WorkflowActionType};
use entities::ids::{
    BusinessDocumentId, ProcurementConfirmationId, SalesOrderId, SalesOrderReviewId, SalesOrderSubmissionId,
    WorkItemId, WorkflowActionId,
};
use entities::sales_order::{CommercialStatus, ReviewStatus, SubmissionStatus};
use entities::sales_review::{
    LowMarginManagerConfirmationStatus, ProcurementConfirmation, ProcurementConfirmationData,
    ProcurementConfirmationStatus, SalesOrderReview, SalesOrderReviewData, SalesOrderReviewDecision,
    SalesReviewStage,
};
use entities::work_item::{
    AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::procurement_confirmation::parse_task_version;
use super::SalesReviewService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::WorkItemService;

const DECISION_ACTION: &str = "sales_order_review.complete_low_margin_confirmation";

/// 低毛利上级决定载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LowMarginManagerConfirmationDecision {
    /// 同意公司承担低毛利；仍需重新进入采购确认。
    Approve {
        /// 固定任务类型判别器。
        work_item_type: WorkItemType,
        /// 销售单。
        sales_order_id: String,
        /// 原驳回采购确认。
        rejected_procurement_confirmation_id: String,
        /// 本轮低毛利不可变提交。
        low_margin_submission_id: String,
        /// 销售单期望锁版本。
        expected_sales_order_lock_version: u64,
        /// 决定意见。
        comment: Option<String>,
    },
    /// 不同意公司承担低毛利，退回销售继续选择固定出路。
    Reject {
        /// 固定任务类型判别器。
        work_item_type: WorkItemType,
        /// 销售单。
        sales_order_id: String,
        /// 原驳回采购确认。
        rejected_procurement_confirmation_id: String,
        /// 本轮低毛利不可变提交。
        low_margin_submission_id: String,
        /// 销售单期望锁版本。
        expected_sales_order_lock_version: u64,
        /// 结构化驳回原因代码。
        reason_code: String,
        /// 驳回意见。
        comment: String,
    },
}

impl LowMarginManagerConfirmationDecision {
    fn work_item_type(&self) -> WorkItemType {
        match self {
            Self::Approve { work_item_type, .. } | Self::Reject { work_item_type, .. } => *work_item_type,
        }
    }

    fn sales_order_id(&self) -> &str {
        match self {
            Self::Approve { sales_order_id, .. } | Self::Reject { sales_order_id, .. } => sales_order_id,
        }
    }

    fn rejected_confirmation_id(&self) -> &str {
        match self {
            Self::Approve {
                rejected_procurement_confirmation_id,
                ..
            }
            | Self::Reject {
                rejected_procurement_confirmation_id,
                ..
            } => rejected_procurement_confirmation_id,
        }
    }

    fn submission_id(&self) -> &str {
        match self {
            Self::Approve {
                low_margin_submission_id,
                ..
            }
            | Self::Reject {
                low_margin_submission_id,
                ..
            } => low_margin_submission_id,
        }
    }

    fn expected_order_version(&self) -> u64 {
        match self {
            Self::Approve {
                expected_sales_order_lock_version,
                ..
            }
            | Self::Reject {
                expected_sales_order_lock_version,
                ..
            } => *expected_sales_order_lock_version,
        }
    }

    fn decision_reason(&self) -> Option<String> {
        match self {
            Self::Approve { comment, .. } => comment.clone(),
            Self::Reject {
                reason_code, comment, ..
            } => Some(format!("{}：{}", reason_code.trim(), comment.trim())),
        }
    }

    fn validate(&self) -> Result<()> {
        if self.work_item_type() != WorkItemType::LowMarginManagerConfirmation {
            return Err(Error::ValidationError(
                "任务类型必须是 LOW_MARGIN_MANAGER_CONFIRMATION".to_string(),
            ));
        }
        for (value, label) in [
            (self.sales_order_id(), "销售单"),
            (self.rejected_confirmation_id(), "原驳回采购确认"),
            (self.submission_id(), "低毛利提交"),
        ] {
            if value.trim().is_empty() {
                return Err(Error::ValidationError(format!("{label}不能为空")));
            }
        }
        if let Self::Reject {
            reason_code, comment, ..
        } = self
        {
            if reason_code.trim().is_empty() || comment.trim().is_empty() {
                return Err(Error::ValidationError(
                    "低毛利驳回原因代码和意见不能为空".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// 完成低毛利上级确认的唯一命令信封。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompleteLowMarginManagerConfirmationCommand {
    /// 当前待办。
    pub work_item_id: String,
    /// 当前任务乐观锁版本。
    pub expected_task_version: String,
    /// 冻结提交版本；必须与任务一致。
    pub expected_subject_version: String,
    /// 强类型决定载荷。
    pub decision: LowMarginManagerConfirmationDecision,
    /// 幂等键。
    pub idempotency_key: String,
}

impl CompleteLowMarginManagerConfirmationCommand {
    fn validate(&self) -> Result<()> {
        if self.work_item_id.trim().is_empty()
            || self.expected_subject_version.trim().is_empty()
            || self.idempotency_key.trim().is_empty()
        {
            return Err(Error::ValidationError(
                "待办、对象版本和幂等键不能为空".to_string(),
            ));
        }
        if self.idempotency_key.chars().count() > 128 {
            return Err(Error::ValidationError("幂等键过长".to_string()));
        }
        self.decision.validate()
    }
}

/// 低毛利上级决定形成的业务结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LowMarginManagerConfirmationBusinessResult {
    /// 上级通过，已为同一新提交创建唯一采购确认。
    LowMarginApprovedAndProcurementResubmitted {
        /// 销售单。
        sales_order_id: String,
        /// 低毛利提交。
        low_margin_submission_id: String,
        /// 正式上级决定。
        sales_order_review_id: String,
        /// 追加工作流动作。
        workflow_action_id: String,
        /// 新采购确认。
        new_procurement_confirmation_id: String,
        /// 新采购待办。
        new_procurement_work_item_id: String,
    },
    /// 上级驳回，销售已回到固定三路处置。
    LowMarginRejectedToSales {
        /// 销售单。
        sales_order_id: String,
        /// 低毛利提交。
        low_margin_submission_id: String,
        /// 正式上级决定。
        sales_order_review_id: String,
        /// 追加工作流动作。
        workflow_action_id: String,
    },
}

/// 完成低毛利上级确认的固定响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteLowMarginManagerConfirmationResult {
    /// 已完成待办。
    pub work_item_id: String,
    /// 固定为 `COMPLETED`。
    pub work_item_status: WorkItemStatus,
    /// 业务结果。
    pub business_result: LowMarginManagerConfirmationBusinessResult,
}

impl SalesReviewService {
    /// 完成低毛利上级确认并在同一事务写决定事实、待办终态及后继业务事实。
    ///
    /// # Errors
    /// 任务责任、角色、岗位分离、对象身份、版本或商业条件漂移时整体回滚。
    pub async fn complete_low_margin_manager_confirmation(
        &self,
        command: CompleteLowMarginManagerConfirmationCommand,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<CompleteLowMarginManagerConfirmationResult> {
        command.validate()?;
        let expected_task_version = parse_task_version(&command.expected_task_version)?;
        let fingerprint = decision_fingerprint(actor.id(), &command)?;
        let audit_id = decision_audit_id(actor.id(), &command.idempotency_key);
        if let Some(result) = self
            .replay_low_margin_decision(&audit_id, &fingerprint, &command)
            .await?
        {
            return Ok(result);
        }
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let command_for_tx = command.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let rbac_for_tx = rbac.clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let service = SalesReviewService::new(db.clone());
                    let mut facts = service
                        .load_low_margin_decision_facts(
                            &command_for_tx,
                            expected_task_version,
                            &actor_id,
                            session,
                        )
                        .await?;
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&actor_owned, &facts.work_item, session)
                        .await?;
                    let now = Instant::now();
                    let decision = match &command_for_tx.decision {
                        LowMarginManagerConfirmationDecision::Approve { comment, .. } => {
                            facts.confirmation.approve(actor_id.clone(), comment.clone(), now)?;
                            facts
                                .order
                                .transition_review(ReviewStatus::PendingProcurementConfirmation, &actor_id)?;
                            SalesOrderReviewDecision::Approved
                        }
                        LowMarginManagerConfirmationDecision::Reject {
                            reason_code,
                            comment,
                            ..
                        } => {
                            facts.confirmation.reject(
                                actor_id.clone(),
                                reason_code.clone(),
                                comment.clone(),
                                now,
                            )?;
                            facts.order.return_to_draft(&actor_id)?;
                            facts.submission.reject(&actor_id)?;
                            SalesOrderReviewDecision::Rejected
                        }
                    };
                    facts.work_item.record_activity(&actor_id, now)?;
                    facts.work_item.complete_by_domain_command(&actor_id, now)?;
                    let review = SalesOrderReview::new(
                        SalesOrderReviewId::new(next_id()),
                        SalesOrderReviewData {
                            sales_order_id: SalesOrderId::new(facts.order.base.id.clone()),
                            submission_id: SalesOrderSubmissionId::new(
                                facts.submission.base.id.clone(),
                            ),
                            review_stage: SalesReviewStage::LowMarginSuperior,
                            status: decision,
                            reviewer_id: actor_id.clone(),
                            reviewed_at: now,
                            decision_reason: command_for_tx.decision.decision_reason(),
                        },
                    )?;
                    let workflow = low_margin_workflow(&facts, &command_for_tx.decision, &actor_id)?;
                    let business_result = if decision == SalesOrderReviewDecision::Approved {
                        let procurement = new_procurement_confirmation(&facts, &actor_id)?;
                        let procurement_work_item = new_procurement_work_item(&procurement, &facts.submission)?;
                        db.procurement_confirmations().create(&procurement, session).await?;
                        db.work_items().create(&procurement_work_item, session).await?;
                        LowMarginManagerConfirmationBusinessResult::LowMarginApprovedAndProcurementResubmitted {
                            sales_order_id: facts.order.base.id.clone(),
                            low_margin_submission_id: facts.submission.base.id.clone(),
                            sales_order_review_id: review.base.id.clone(),
                            workflow_action_id: workflow.base.id.clone(),
                            new_procurement_confirmation_id: procurement.base.id,
                            new_procurement_work_item_id: procurement_work_item.base.id,
                        }
                    } else {
                        LowMarginManagerConfirmationBusinessResult::LowMarginRejectedToSales {
                            sales_order_id: facts.order.base.id.clone(),
                            low_margin_submission_id: facts.submission.base.id.clone(),
                            sales_order_review_id: review.base.id.clone(),
                            workflow_action_id: workflow.base.id.clone(),
                        }
                    };
                    db.low_margin_manager_confirmations()
                        .update(&mut facts.confirmation, session)
                        .await?;
                    db.sales_orders().update(&mut facts.order, session).await?;
                    if decision == SalesOrderReviewDecision::Rejected {
                        db.sales_order_submissions()
                            .update(&mut facts.submission, session)
                            .await?;
                    }
                    db.sales_order_reviews().create(&review, session).await?;
                    db.workflow_actions().create(&workflow, session).await?;
                    db.work_items().update(&mut facts.work_item, session).await?;
                    let result = CompleteLowMarginManagerConfirmationResult {
                        work_item_id: facts.work_item.base.id.clone(),
                        work_item_status: facts.work_item.status,
                        business_result,
                    };
                    let audit = actor_owned.resource_log_with_id(
                        audit_id_for_tx,
                        DECISION_ACTION,
                        "low_margin_manager_confirmation",
                        facts.confirmation.base.id,
                        Some(decision_receipt_message(&fingerprint_for_tx, &result)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CompleteLowMarginManagerConfirmationResult, crate::errors::Error>(result)
                })
            })
            .await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                if let Some(result) = self
                    .replay_low_margin_decision(&audit_id, &fingerprint, &command)
                    .await?
                {
                    return Ok(result);
                }
                Err(error)
            }
        }
    }

    async fn load_low_margin_decision_facts(
        &self,
        command: &CompleteLowMarginManagerConfirmationCommand,
        expected_task_version: u64,
        actor_id: &str,
        executor: &mut dyn database::Executor,
    ) -> Result<LowMarginDecisionFacts> {
        let work_item = self
            .db
            .work_items()
            .find_by_id(&command.work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("低毛利上级确认待办不存在".to_string()))?;
        if work_item.base.version != expected_task_version
            || work_item.subject_version != command.expected_subject_version
            || work_item.subject_version != command.decision.submission_id()
        {
            return Err(Error::ConflictError(
                "低毛利待办版本已变化，请刷新后重试".to_string(),
            ));
        }
        if work_item.work_item_type != WorkItemType::LowMarginManagerConfirmation
            || work_item.business_object_type != "sales_order"
            || work_item.business_object_id != command.decision.sales_order_id()
            || work_item.approval_step_instance_id.is_some()
            || !work_item.is_owned_by(actor_id)
        {
            return Err(Error::Forbidden("当前账号不是该低毛利待办责任人".to_string()));
        }
        let resolver = crate::approval::ApprovalAssigneeResolver::new(self.db.clone());
        if !resolver
            .user_is_eligible_for_assignment(
                actor_id,
                &work_item.owner_role,
                &work_item.owner_organization_id,
                executor,
            )
            .await?
        {
            return Err(Error::Forbidden("当前账号已不具备销售领导责任资格".to_string()));
        }
        let confirmation = self
            .db
            .low_margin_manager_confirmations()
            .find_one_by_field(
                "low_margin_submission_id",
                command.decision.submission_id(),
                executor,
            )
            .await?
            .ok_or_else(|| Error::NotFound("低毛利上级确认事实不存在".to_string()))?;
        if confirmation.status != LowMarginManagerConfirmationStatus::Pending
            || confirmation.sales_order_id.as_ref() != command.decision.sales_order_id()
            || confirmation.rejected_procurement_confirmation_id.as_ref()
                != command.decision.rejected_confirmation_id()
        {
            return Err(Error::ConflictError("低毛利确认链与命令不一致".to_string()));
        }
        if confirmation.requested_by == actor_id {
            return Err(Error::Forbidden("低毛利申请人不得确认自己的申请".to_string()));
        }
        let order = self
            .db
            .sales_orders()
            .find_by_id(command.decision.sales_order_id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != command.decision.expected_order_version()
            || order.commercial_status != CommercialStatus::PendingReview
            || order.review_status != ReviewStatus::PendingLowMarginSuperior
        {
            return Err(Error::ConflictError(
                "销售单已不在待低毛利上级确认状态".to_string(),
            ));
        }
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(command.decision.submission_id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("低毛利提交不存在".to_string()))?;
        if submission.sales_order_id.as_ref() != order.base.id
            || submission.stable.status != SubmissionStatus::InReview
            || submission.submitted_by == actor_id
        {
            return Err(Error::Forbidden("低毛利提交身份或岗位分离不成立".to_string()));
        }
        let rejected_confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(command.decision.rejected_confirmation_id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("原驳回采购确认不存在".to_string()))?;
        if rejected_confirmation.stable.status != ProcurementConfirmationStatus::Rejected
            || rejected_confirmation.submission_id != confirmation.rejected_submission_id
        {
            return Err(Error::ConflictError("原采购驳回事实已不匹配".to_string()));
        }
        let rejected_submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.rejected_submission_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原被驳回提交不存在".to_string()))?;
        let lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                &[
                    SalesOrderSubmissionId::new(submission.base.id.clone()),
                    SalesOrderSubmissionId::new(rejected_submission.base.id.clone()),
                ],
                executor,
            )
            .await?;
        if !submission_terms_equal(&submission, &rejected_submission, &lines) {
            return Err(Error::ConflictError(
                "低毛利提交与原被驳回商业条件不一致".to_string(),
            ));
        }
        Ok(LowMarginDecisionFacts {
            work_item,
            confirmation,
            order,
            submission,
        })
    }

    async fn replay_low_margin_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        command: &CompleteLowMarginManagerConfirmationCommand,
    ) -> Result<Option<CompleteLowMarginManagerConfirmationResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != DECISION_ACTION || audit.resource_type != "low_margin_manager_confirmation" {
            return Err(Error::Internal("低毛利决定幂等收据身份非法".to_string()));
        }
        let result = parse_decision_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("低毛利决定幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        if result.work_item_id != command.work_item_id {
            return Err(Error::Internal("低毛利决定幂等收据待办不一致".to_string()));
        }
        Ok(Some(result))
    }
}

struct LowMarginDecisionFacts {
    work_item: WorkItem,
    confirmation: entities::sales_review::LowMarginManagerConfirmation,
    order: entities::sales_order::SalesOrder,
    submission: entities::sales_order::SalesOrderSubmission,
}

fn low_margin_workflow(
    facts: &LowMarginDecisionFacts,
    decision: &LowMarginManagerConfirmationDecision,
    actor_id: &str,
) -> Result<WorkflowAction> {
    let (action_type, to_status) = match decision {
        LowMarginManagerConfirmationDecision::Approve { .. } => (
            WorkflowActionType::Approve,
            ReviewStatus::PendingProcurementConfirmation.as_str(),
        ),
        LowMarginManagerConfirmationDecision::Reject { .. } => {
            (WorkflowActionType::Reject, CommercialStatus::Draft.as_str())
        }
    };
    WorkflowAction::new(
        WorkflowActionId::new(next_id()),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(facts.order.base.id.clone()),
            action_type,
            from_status: ReviewStatus::PendingLowMarginSuperior.as_str().to_string(),
            to_status: to_status.to_string(),
            actor_id: actor_id.to_string(),
            actor_role: facts.work_item.owner_role.clone(),
            comment: decision.decision_reason(),
        },
    )
    .map_err(Error::Logic)
}

fn new_procurement_confirmation(
    facts: &LowMarginDecisionFacts,
    actor_id: &str,
) -> Result<ProcurementConfirmation> {
    ProcurementConfirmation::new(
        ProcurementConfirmationId::new(next_id()),
        ProcurementConfirmationData {
            sales_order_id: SalesOrderId::new(facts.order.base.id.clone()),
            submission_id: SalesOrderSubmissionId::new(facts.submission.base.id.clone()),
            reject_reason_code: None,
            comment: Some("低毛利上级通过后重新进入采购确认".to_string()),
        },
        actor_id,
    )
    .map_err(Error::Logic)
}

fn new_procurement_work_item(
    confirmation: &ProcurementConfirmation,
    submission: &entities::sales_order::SalesOrderSubmission,
) -> Result<WorkItem> {
    WorkItem::new(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::ProcurementConfirmation,
            approval_step_instance_id: None,
            business_object_type: "procurement_confirmation".to_string(),
            business_object_id: confirmation.base.id.clone(),
            subject_version: submission.base.id.clone(),
            assignment_mode: AssignmentMode::Pool,
            owner_role: "role-procurement".to_string(),
            owner_organization_id: "company".to_string(),
            owner_user_id: None,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some("low_margin_approved_procurement_confirmation".to_string()),
            impact_summary: Some("不确认则销售单不能生效".to_string()),
        },
    )
    .map_err(Error::Logic)
}

fn submission_terms_equal(
    left: &entities::sales_order::SalesOrderSubmission,
    right: &entities::sales_order::SalesOrderSubmission,
    lines: &[entities::sales_order::SalesOrderSubmissionLine],
) -> bool {
    let headers_equal = left.business_type == right.business_type
        && left.customer_id == right.customer_id
        && left.contract_revision_id == right.contract_revision_id
        && left.settlement_party_id == right.settlement_party_id
        && left.customer_snapshot == right.customer_snapshot
        && left.contract_snapshot == right.contract_snapshot
        && left.settlement_party_snapshot == right.settlement_party_snapshot
        && left.payment_term_snapshot == right.payment_term_snapshot
        && left.invoice_requirement_snapshot == right.invoice_requirement_snapshot
        && left.project_name == right.project_name
        && left.business_remark == right.business_remark
        && left.gross_amount == right.gross_amount
        && left.net_amount == right.net_amount
        && left.tax_amount == right.tax_amount;
    if !headers_equal {
        return false;
    }
    let left_lines = lines
        .iter()
        .filter(|line| line.submission_id.as_ref() == left.base.id)
        .collect::<Vec<_>>();
    let right_lines = lines
        .iter()
        .filter(|line| line.submission_id.as_ref() == right.base.id)
        .collect::<Vec<_>>();
    left_lines.len() == right_lines.len()
        && left_lines.iter().all(|left_line| {
            right_lines
                .iter()
                .find(|right_line| right_line.line_no == left_line.line_no)
                .is_some_and(|right_line| submission_line_equal(left_line, right_line))
        })
}

fn submission_line_equal(
    left: &entities::sales_order::SalesOrderSubmissionLine,
    right: &entities::sales_order::SalesOrderSubmissionLine,
) -> bool {
    left.sales_order_line_id == right.sales_order_line_id
        && left.line_type == right.line_type
        && left.gross_amount == right.gross_amount
        && left.net_amount == right.net_amount
        && left.tax_amount == right.tax_amount
        && left.sales_tax_rate == right.sales_tax_rate
        && left.item_name_snapshot == right.item_name_snapshot
        && left.spec_snapshot == right.spec_snapshot
        && left.unit_snapshot == right.unit_snapshot
        && left.sku_id == right.sku_id
        && left.sku_revision_id == right.sku_revision_id
        && left.welfare_scenario == right.welfare_scenario
        && left.fulfillment_mode == right.fulfillment_mode
        && left.fulfillment_due_at == right.fulfillment_due_at
        && left.quantity == right.quantity
        && left.base_unit_code == right.base_unit_code
        && left.unit_price_gross == right.unit_price_gross
        && left.face_value == right.face_value
        && left.card_count == right.card_count
        && left.face_value_total == right.face_value_total
        && left.transaction_amount == right.transaction_amount
        && left.gift_amount == right.gift_amount
        && left.gift_rate == right.gift_rate
        && left.card_form == right.card_form
}

fn decision_fingerprint(
    actor_id: &str,
    command: &CompleteLowMarginManagerConfirmationCommand,
) -> Result<String> {
    let payload = serde_json::to_vec(&(DECISION_ACTION, actor_id, command))
        .map_err(|error| Error::Internal(format!("低毛利决定命令序列化失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn decision_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    format!(
        "w05-lm-{:x}",
        Sha256::digest(format!("{DECISION_ACTION}|{actor_id}|{}", idempotency_key.trim()).as_bytes())
    )
}

fn decision_receipt_message(
    fingerprint: &str,
    result: &CompleteLowMarginManagerConfirmationResult,
) -> String {
    let value = match &result.business_result {
        LowMarginManagerConfirmationBusinessResult::LowMarginApprovedAndProcurementResubmitted {
            sales_order_id,
            low_margin_submission_id,
            sales_order_review_id,
            workflow_action_id,
            new_procurement_confirmation_id,
            new_procurement_work_item_id,
        } => format!(
            "A|{}|{sales_order_id}|{low_margin_submission_id}|{sales_order_review_id}|{workflow_action_id}|{new_procurement_confirmation_id}|{new_procurement_work_item_id}",
            result.work_item_id
        ),
        LowMarginManagerConfirmationBusinessResult::LowMarginRejectedToSales {
            sales_order_id,
            low_margin_submission_id,
            sales_order_review_id,
            workflow_action_id,
        } => format!(
            "R|{}|{sales_order_id}|{low_margin_submission_id}|{sales_order_review_id}|{workflow_action_id}",
            result.work_item_id
        ),
    };
    format!("fp={fingerprint};result={value}")
}

fn parse_decision_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<CompleteLowMarginManagerConfirmationResult> {
    let (fingerprint, value) = message
        .strip_prefix("fp=")
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("低毛利决定收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError("同一幂等键已用于不同低毛利决定".to_string()));
    }
    let fields = value.split('|').collect::<Vec<_>>();
    let (work_item_id, business_result) = match fields.as_slice() {
        ["A", work_item_id, order_id, submission_id, review_id, workflow_id, confirmation_id, procurement_work_item_id] => {
            (
                (*work_item_id).to_string(),
                LowMarginManagerConfirmationBusinessResult::LowMarginApprovedAndProcurementResubmitted {
                    sales_order_id: (*order_id).to_string(),
                    low_margin_submission_id: (*submission_id).to_string(),
                    sales_order_review_id: (*review_id).to_string(),
                    workflow_action_id: (*workflow_id).to_string(),
                    new_procurement_confirmation_id: (*confirmation_id).to_string(),
                    new_procurement_work_item_id: (*procurement_work_item_id).to_string(),
                },
            )
        }
        ["R", work_item_id, order_id, submission_id, review_id, workflow_id] => (
            (*work_item_id).to_string(),
            LowMarginManagerConfirmationBusinessResult::LowMarginRejectedToSales {
                sales_order_id: (*order_id).to_string(),
                low_margin_submission_id: (*submission_id).to_string(),
                sales_order_review_id: (*review_id).to_string(),
                workflow_action_id: (*workflow_id).to_string(),
            },
        ),
        _ => return Err(Error::Internal("低毛利决定收据结果非法".to_string())),
    };
    Ok(CompleteLowMarginManagerConfirmationResult {
        work_item_id,
        work_item_status: WorkItemStatus::Completed,
        business_result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_receipt_round_trip_rejects_fingerprint_reuse() {
        let result = CompleteLowMarginManagerConfirmationResult {
            work_item_id: "wi-1".to_string(),
            work_item_status: WorkItemStatus::Completed,
            business_result:
                LowMarginManagerConfirmationBusinessResult::LowMarginApprovedAndProcurementResubmitted {
                    sales_order_id: "so-1".to_string(),
                    low_margin_submission_id: "sub-2".to_string(),
                    sales_order_review_id: "review-1".to_string(),
                    workflow_action_id: "workflow-1".to_string(),
                    new_procurement_confirmation_id: "pc-2".to_string(),
                    new_procurement_work_item_id: "wi-2".to_string(),
                },
        };
        let message = decision_receipt_message("abc", &result);
        assert_eq!(parse_decision_receipt(&message, "abc").unwrap(), result);
        assert!(parse_decision_receipt(&message, "def").is_err());
    }

    #[test]
    fn command_rejects_generic_or_unknown_decision_payloads() {
        let invalid = serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "sub-2",
            "decision": {
                "decision": "APPROVE",
                "work_item_type": "LOW_MARGIN_MANAGER_CONFIRMATION",
                "sales_order_id": "so-1",
                "rejected_procurement_confirmation_id": "pc-1",
                "low_margin_submission_id": "sub-2",
                "expected_sales_order_lock_version": 3,
                "generic_action": "complete_work_item"
            },
            "idempotency_key": "key-1"
        });
        assert!(serde_json::from_value::<CompleteLowMarginManagerConfirmationCommand>(invalid).is_err());
    }
}
