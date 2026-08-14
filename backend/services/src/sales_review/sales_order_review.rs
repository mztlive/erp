//! 卡券销售审批的强类型 HTTP 合同与事务内领域动作端口。

use std::str::FromStr;

use database::{
    ApprovalExt, DocumentRegistryExt, Executor, NoTransaction, ProjectionExt, ReceivableExt, SalesOrderExt,
    SalesReviewExt, WorkItemExt,
};
use entities::{
    approval::{
        ApprovalDecision, ApprovalInstance, ApprovalInstanceStatus, ApprovalStepInstance, ApprovalStepStatus,
    },
    common::time::Instant,
    document_registry::{
        BusinessDocument, DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionType,
    },
    ids::{
        BusinessDocumentId, ReceivableAccountId, ReceivableEntryId, SalesOrderId,
        SalesOrderProjectionDeliveryId, SalesOrderProjectionId, SalesOrderProjectionRevisionId,
        SalesOrderReviewId, SalesOrderSubmissionId, WorkItemId, WorkflowActionId,
    },
    money::Amount,
    projection::{
        CardForm as ProjectionCardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection,
        SalesOrderProjectionData, SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData,
        SalesOrderProjectionRevision, SalesOrderProjectionRevisionData,
    },
    receivable::{
        AccountReviewStatus, EntryDirection, ReceivableAccount, ReceivableAccountData, ReceivableEntry,
        ReceivableEntryData, ReceivableEntryType,
    },
    sales_order::{
        BusinessType, CardForm, CommercialStatus, LineType, ReviewStatus, RevisionSource, SalesOrder,
        SalesOrderSubmission, SalesOrderSubmissionLine, SubmissionStatus,
    },
    sales_review::{SalesOrderReview, SalesOrderReviewData, SalesOrderReviewDecision, SalesReviewStage},
    work_item::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus,
        WorkItemType,
    },
    AccountKind,
};
use id_generator::next_id;
use mongodb::{bson::doc, Database};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::dto;
use super::formalization::{build_revision, RevisionAggregate};
use super::{
    PageView, SalesOrderReviewFilter, SalesOrderReviewListParams, SalesOrderReviewView, SalesReviewService,
};
use crate::{
    approval::{
        ApprovalActionContext, ApprovalAssigneeResolver, ApprovalBusinessAction, ApprovalDomainActionPort,
        ApprovalRuntimeView, CancelApprovalCommand, SubmitDecisionCommand, CARD_SALES_APPROVAL,
        CARD_SALES_APPROVAL_VERSION, OPERATIONS_APPROVAL, SALES_MANAGER_APPROVAL,
    },
    audit::AuditActor,
    errors::{Error, Result},
    projection::projection_content_hash,
};

const SALES_ORDER_OBJECT: &str = "sales_order";
const SALES_MANAGER_ROLE: &str = "role-sales-leader";
const OPERATIONS_ROLE: &str = "role-operations";
const SALES_ROLE: &str = "role-sales";
const MAX_ID_LEN: usize = 128;
const MAX_IDEMPOTENCY_KEY_LEN: usize = 256;
const MAX_REASON_CODE_LEN: usize = 64;
const MAX_COMMENT_LEN: usize = 512;

/// W05 当前步骤允许提交的卡券销售审批结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardSalesReviewDecision {
    /// 通过当前审批步骤。
    Approve,
    /// 驳回申请人并结束当前审批实例。
    Reject,
    /// 终止当前审批实例；该决定不会形成驳回审核记录。
    Terminate,
}

/// W05 强命令锁定的销售审核轨状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardSalesExpectedReviewStatus {
    /// 等待销售领导审批。
    PendingSalesLead,
    /// 等待运营审批。
    PendingOperations,
}

impl CardSalesExpectedReviewStatus {
    fn domain_status(self) -> ReviewStatus {
        match self {
            Self::PendingSalesLead => ReviewStatus::PendingSalesLeader,
            Self::PendingOperations => ReviewStatus::PendingOperations,
        }
    }
}

/// W05 卡券销售审批决定的业务对象锁定部分。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardSalesApprovalDecision {
    /// 销售单稳定 ID。
    pub sales_order_id: String,
    /// 被审批的不可变提交 ID。
    pub sales_order_submission_id: String,
    /// 客户端读取到的销售单乐观锁版本。
    pub expected_sales_order_lock_version: u64,
    /// 客户端读取到的提交序号。
    pub expected_submission_no: u32,
    /// 固定审批任务类型。
    pub work_item_type: WorkItemType,
    /// 固定审核轨状态。
    pub expected_review_status: CardSalesExpectedReviewStatus,
    /// 当前步骤的正式结论。
    pub review_decision: CardSalesReviewDecision,
    /// 驳回或终止原因代码；通过时必须为空。
    pub reason_code: Option<String>,
    /// 审批意见。
    pub comment: Option<String>,
}

/// W05 §8.3 卡券销售审批的唯一 HTTP 请求信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitCardSalesApprovalDecisionCommand {
    /// 审批实例 ID。
    pub approval_instance_id: String,
    /// 客户端读取到的审批实例版本。
    pub expected_instance_version: String,
    /// 当前步骤实例 ID。
    pub approval_step_instance_id: String,
    /// 客户端读取到的步骤版本。
    pub expected_step_version: String,
    /// 当前待办 ID。
    pub work_item_id: String,
    /// 客户端读取到的待办版本。
    pub expected_task_version: String,
    /// 审批实例冻结的提交 ID。
    pub expected_subject_version: String,
    /// 强类型业务决定。
    pub decision: CardSalesApprovalDecision,
    /// 正式操作幂等键。
    pub idempotency_key: String,
}

/// W05 卡券销售审批撤回的唯一 HTTP 请求信封。
///
/// 当前步骤受阻且尚未形成开放待办时，`work_item_id` 与
/// `expected_task_version` 必须同时为空；其它情况必须同时存在。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelCardSalesApprovalCommand {
    /// 审批实例 ID。
    pub approval_instance_id: String,
    /// 当前步骤实例 ID。
    pub current_step_instance_id: String,
    /// 当前开放待办 ID；解析受阻且未建待办时为空。
    pub work_item_id: Option<String>,
    /// 客户端读取到的审批实例版本。
    pub expected_instance_version: String,
    /// 客户端读取到的当前步骤版本。
    pub expected_step_version: String,
    /// 客户端读取到的待办版本；无待办时为空。
    pub expected_task_version: Option<String>,
    /// 审批实例冻结的不可变销售提交 ID。
    pub expected_subject_version: String,
    /// 结构化撤回原因。
    pub reason: String,
    /// 正式操作幂等键。
    pub idempotency_key: String,
}

/// 重读 W05 撤回结果所需的服务端可信身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSalesApprovalCancelGuard {
    approval_instance_id: String,
    current_step_instance_id: String,
    work_item_id: Option<String>,
    expected_subject_version: String,
    actor_id: String,
}

impl CancelCardSalesApprovalCommand {
    /// 校验 W05 撤回信封并转换为稳定审批运行时命令。
    ///
    /// `actor_id` 必须来自已认证身份；客户端无法指定撤回人。
    ///
    /// # 错误
    /// 身份、版本、待办字段配对、原因或幂等键不合法时返回参数错误。
    pub fn into_runtime_command(
        self,
        actor_id: impl Into<String>,
    ) -> Result<(CancelApprovalCommand, CardSalesApprovalCancelGuard)> {
        let actor_id = actor_id.into();
        validate_text(&actor_id, "撤回人", MAX_ID_LEN)?;
        validate_text(&self.approval_instance_id, "审批实例ID", MAX_ID_LEN)?;
        validate_text(&self.current_step_instance_id, "审批步骤实例ID", MAX_ID_LEN)?;
        validate_text(&self.expected_subject_version, "审批对象版本", MAX_ID_LEN)?;
        validate_text(&self.reason, "撤回原因", MAX_COMMENT_LEN)?;
        validate_text(&self.idempotency_key, "幂等键", MAX_IDEMPOTENCY_KEY_LEN)?;
        if let Some(work_item_id) = self.work_item_id.as_deref() {
            validate_text(work_item_id, "审批任务ID", MAX_ID_LEN)?;
        }
        if self.work_item_id.is_some() != self.expected_task_version.is_some() {
            return Err(Error::ValidationError(
                "审批任务ID与任务期望版本必须同时提供或同时为空".to_string(),
            ));
        }
        let expected_task_version = self
            .expected_task_version
            .as_deref()
            .map(|value| parse_version(value, "审批任务"))
            .transpose()?;
        let guard = CardSalesApprovalCancelGuard {
            approval_instance_id: self.approval_instance_id.clone(),
            current_step_instance_id: self.current_step_instance_id.clone(),
            work_item_id: self.work_item_id.clone(),
            expected_subject_version: self.expected_subject_version.clone(),
            actor_id: actor_id.clone(),
        };
        Ok((
            CancelApprovalCommand {
                approval_instance_id: self.approval_instance_id,
                current_step_instance_id: self.current_step_instance_id,
                current_work_item_id: self.work_item_id,
                expected_instance_version: parse_version(&self.expected_instance_version, "审批实例")?,
                expected_step_version: parse_version(&self.expected_step_version, "审批步骤")?,
                expected_task_version,
                expected_subject_version: self.expected_subject_version,
                actor_id,
                reason: self.reason,
                idempotency_key: self.idempotency_key,
            },
            guard,
        ))
    }
}

/// 事务内重验 W05 强命令所需的不可变服务端护栏。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSalesApprovalDecisionGuard {
    approval_instance_id: String,
    approval_step_instance_id: String,
    work_item_id: String,
    expected_subject_version: String,
    sales_order_id: String,
    submission_id: String,
    expected_sales_order_lock_version: u64,
    expected_submission_no: u32,
    work_item_type: WorkItemType,
    expected_review_status: CardSalesExpectedReviewStatus,
    review_decision: CardSalesReviewDecision,
    reason: Option<String>,
    actor_id: String,
}

impl CardSalesApprovalDecisionGuard {
    fn review_stage(&self) -> Result<SalesReviewStage> {
        match self.work_item_type {
            WorkItemType::CardSalesManagerApproval => Ok(SalesReviewStage::SalesLeader),
            WorkItemType::CardSalesOperationApproval => Ok(SalesReviewStage::Operations),
            _ => Err(Error::Internal("W05 强命令护栏包含未注册任务类型".to_string())),
        }
    }
}

impl SubmitCardSalesApprovalDecisionCommand {
    /// 校验强类型信封，并转换为审批运行时命令与事务内业务护栏。
    ///
    /// `actor_id` 必须来自已认证身份，客户端请求中没有该字段。
    ///
    /// # 错误
    /// 身份、版本、固定任务组合或驳回原因不合法时返回参数错误。
    pub fn into_runtime_command(
        self,
        actor_id: impl Into<String>,
    ) -> Result<(SubmitDecisionCommand, CardSalesApprovalDecisionGuard)> {
        let actor_id = actor_id.into();
        validate_text(&actor_id, "审批人", MAX_ID_LEN)?;
        validate_text(&self.approval_instance_id, "审批实例ID", MAX_ID_LEN)?;
        validate_text(&self.approval_step_instance_id, "审批步骤实例ID", MAX_ID_LEN)?;
        validate_text(&self.work_item_id, "审批任务ID", MAX_ID_LEN)?;
        validate_text(&self.expected_subject_version, "审批对象版本", MAX_ID_LEN)?;
        validate_text(&self.idempotency_key, "幂等键", MAX_IDEMPOTENCY_KEY_LEN)?;
        validate_text(&self.decision.sales_order_id, "销售单ID", MAX_ID_LEN)?;
        validate_text(&self.decision.sales_order_submission_id, "销售提交ID", MAX_ID_LEN)?;
        if self.expected_subject_version != self.decision.sales_order_submission_id {
            return Err(Error::ValidationError(
                "审批对象版本必须与销售提交ID一致".to_string(),
            ));
        }
        if self.decision.expected_sales_order_lock_version == 0 {
            return Err(Error::ValidationError("销售单期望版本必须为正整数".to_string()));
        }
        if self.decision.expected_submission_no == 0 {
            return Err(Error::ValidationError("提交序号必须为正整数".to_string()));
        }
        validate_fixed_stage(self.decision.work_item_type, self.decision.expected_review_status)?;

        let reason_code = normalize_optional(self.decision.reason_code, "驳回原因代码", MAX_REASON_CODE_LEN)?;
        let comment = normalize_optional(self.decision.comment, "审批意见", MAX_COMMENT_LEN)?;
        let reason = match self.decision.review_decision {
            CardSalesReviewDecision::Approve => {
                if reason_code.is_some() {
                    return Err(Error::ValidationError("通过决定不得携带驳回原因代码".to_string()));
                }
                comment
            }
            CardSalesReviewDecision::Reject => {
                let reason_code = reason_code
                    .ok_or_else(|| Error::ValidationError("驳回决定必须填写原因代码".to_string()))?;
                let reason = match comment {
                    Some(comment) => format!("{reason_code}: {comment}"),
                    None => reason_code,
                };
                if reason.chars().count() > MAX_COMMENT_LEN {
                    return Err(Error::ValidationError("驳回原因过长".to_string()));
                }
                Some(reason)
            }
            CardSalesReviewDecision::Terminate => {
                let reason_code = reason_code
                    .ok_or_else(|| Error::ValidationError("终止决定必须填写原因代码".to_string()))?;
                let reason = match comment {
                    Some(comment) => format!("{reason_code}: {comment}"),
                    None => reason_code,
                };
                if reason.chars().count() > MAX_COMMENT_LEN {
                    return Err(Error::ValidationError("终止原因过长".to_string()));
                }
                Some(reason)
            }
        };
        let expected_task_version = parse_version(&self.expected_task_version, "审批任务")?;
        let expected_instance_version = parse_version(&self.expected_instance_version, "审批实例")?;
        let expected_step_version = parse_version(&self.expected_step_version, "审批步骤")?;
        let decision = match self.decision.review_decision {
            CardSalesReviewDecision::Approve => ApprovalDecision::Approve,
            CardSalesReviewDecision::Reject => ApprovalDecision::RejectToApplicant,
            CardSalesReviewDecision::Terminate => ApprovalDecision::TerminateApproval,
        };
        let guard = CardSalesApprovalDecisionGuard {
            approval_instance_id: self.approval_instance_id.clone(),
            approval_step_instance_id: self.approval_step_instance_id.clone(),
            work_item_id: self.work_item_id.clone(),
            expected_subject_version: self.expected_subject_version.clone(),
            sales_order_id: self.decision.sales_order_id,
            submission_id: self.decision.sales_order_submission_id,
            expected_sales_order_lock_version: self.decision.expected_sales_order_lock_version,
            expected_submission_no: self.decision.expected_submission_no,
            work_item_type: self.decision.work_item_type,
            expected_review_status: self.decision.expected_review_status,
            review_decision: self.decision.review_decision,
            reason: reason.clone(),
            actor_id: actor_id.clone(),
        };
        Ok((
            SubmitDecisionCommand {
                work_item_id: self.work_item_id,
                approval_instance_id: self.approval_instance_id,
                approval_step_instance_id: self.approval_step_instance_id,
                expected_task_version,
                expected_instance_version,
                expected_step_version,
                expected_subject_version: self.expected_subject_version,
                decision,
                reason,
                actor_id,
                idempotency_key: self.idempotency_key,
            },
            guard,
        ))
    }
}

/// W05 卡券销售审批形成的业务结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardSalesApprovalBusinessResult {
    /// 销售领导通过，并进入运营步骤。
    ManagerApproved {
        /// 销售单 ID。
        sales_order_id: String,
        /// 不可变审批记录 ID。
        sales_order_review_id: String,
        /// 追加式工作流动作 ID。
        workflow_action_id: String,
        /// 当前销售审核状态。
        sales_order_commercial_status: String,
        /// 成功解析时的新运营待办 ID；阻塞时为空。
        next_work_item_id: Option<String>,
        /// 成功解析时的新运营待办状态；阻塞时为空。
        next_work_item_status: Option<WorkItemStatus>,
    },
    /// 运营通过并原子形成全部首版正式事实。
    OperationsApprovedAndEffective {
        /// 销售单 ID。
        sales_order_id: String,
        /// 不可变审批记录 ID。
        sales_order_review_id: String,
        /// 追加式工作流动作 ID。
        workflow_action_id: String,
        /// 销售单商业状态。
        sales_order_commercial_status: String,
        /// 首个正式销售版本 ID。
        sales_order_revision_id: String,
        /// 原始应收子账 ID。
        receivable_account_id: String,
        /// 待发送投影下发操作 ID。
        execution_projection_operation_id: String,
    },
    /// 当前阶段驳回并退回销售草稿。
    RejectedToSales {
        /// 销售单 ID。
        sales_order_id: String,
        /// 不可变审批记录 ID。
        sales_order_review_id: String,
        /// 追加式工作流动作 ID。
        workflow_action_id: String,
        /// 销售单商业状态。
        sales_order_commercial_status: String,
    },
    /// 当前阶段终止审批并回到销售草稿；不会伪装为驳回记录。
    Terminated {
        /// 销售单 ID。
        sales_order_id: String,
        /// 被终止审批的冻结提交 ID。
        sales_order_submission_id: String,
        /// 冻结提交终态，固定为 `SUPERSEDED`。
        submission_status: SubmissionStatus,
        /// 追加式终止工作流动作 ID。
        workflow_action_id: String,
        /// 销售单商业状态，固定为草稿。
        sales_order_commercial_status: String,
    },
}

/// W05 卡券销售审批决定响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmitCardSalesApprovalDecisionResult {
    /// 审批实例最新状态。
    pub approval_instance_status: ApprovalInstanceStatus,
    /// 本次命令完成的原待办 ID。
    pub work_item_id: String,
    /// 本次命令完成的原待办状态。
    pub work_item_status: WorkItemStatus,
    /// 服务端重读的正式业务结果。
    pub business_result: CardSalesApprovalBusinessResult,
    /// 审批运行时最新视图；用于显式呈现下一步骤阻塞等状态。
    pub approval: ApprovalRuntimeView,
}

/// W05 卡券审批撤回后服务端重读的可编辑业务事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardSalesApprovalCancelledBusinessResult {
    /// 固定撤回结果码。
    pub outcome: &'static str,
    /// 销售单稳定 ID。
    pub sales_order_id: String,
    /// 销售单最新乐观锁版本。
    pub sales_order_version: String,
    /// 撤回后商业主状态。
    pub sales_order_commercial_status: CommercialStatus,
    /// 撤回后审核轨状态。
    pub sales_order_review_status: ReviewStatus,
    /// 被撤回的冻结提交 ID。
    pub sales_order_submission_id: String,
    /// 冻结提交最新乐观锁版本。
    pub submission_version: String,
    /// 冻结提交撤回后的终态。
    pub submission_status: SubmissionStatus,
    /// 追加式撤回工作流动作 ID。
    pub workflow_action_id: String,
}

/// W05 卡券审批撤回响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelCardSalesApprovalResult {
    /// 审批实例最新状态，成功时固定为 `CANCELLED`。
    pub approval_instance_status: ApprovalInstanceStatus,
    /// 原开放待办 ID；阻塞前未建待办时为空。
    pub work_item_id: Option<String>,
    /// 原开放待办最新状态；存在时固定为 `CLOSED`。
    pub work_item_status: Option<WorkItemStatus>,
    /// 服务端重读的销售单、提交和撤回动作事实。
    pub business_result: CardSalesApprovalCancelledBusinessResult,
    /// 审批实例、步骤和可选待办的最新运行时事实。
    pub approval: ApprovalRuntimeView,
}

/// 卡券销售审批的事务内领域动作端口。
#[derive(Clone)]
pub struct CardSalesApprovalActionPort {
    db: Database,
    decision_guard: Option<CardSalesApprovalDecisionGuard>,
}

impl CardSalesApprovalActionPort {
    /// 创建用于启动、取消和阻塞恢复的卡券销售审批端口。
    pub fn new(db: Database) -> Self {
        Self {
            db,
            decision_guard: None,
        }
    }

    /// 创建绑定 W05 强命令护栏的正式决定端口。
    pub fn for_decision(db: Database, guard: CardSalesApprovalDecisionGuard) -> Self {
        Self {
            db,
            decision_guard: Some(guard),
        }
    }

    async fn execute_action(
        &self,
        action: ApprovalBusinessAction,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_card_context(context)?;
        match action {
            ApprovalBusinessAction::SubmitCardSalesApproval => {
                self.validate_submission_ready(context, executor).await
            }
            ApprovalBusinessAction::RecordSalesManagerApproval => {
                self.record_manager_approval(context, executor).await
            }
            ApprovalBusinessAction::RejectCardSalesBySalesManager => {
                self.reject_current_stage(context, executor).await
            }
            ApprovalBusinessAction::TerminateCardSalesBySalesManager
            | ApprovalBusinessAction::TerminateCardSalesByOperations => {
                self.terminate_current_stage(action, context, executor).await
            }
            ApprovalBusinessAction::ApproveAndActivateCardSales => {
                self.approve_and_activate(context, executor).await
            }
            ApprovalBusinessAction::RejectCardSalesByOperations => {
                self.reject_current_stage(context, executor).await
            }
            ApprovalBusinessAction::CancelCardSalesApproval => self.cancel_approval(context, executor).await,
            ApprovalBusinessAction::ValidateCardSalesApprovalRecovery => {
                self.validate_recovery(context, executor).await
            }
        }
    }

    async fn validate_submission_ready(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let facts = self.load_card_facts(context, executor).await?;
        if facts.submission.submitted_by != context.actor_id {
            return Err(Error::Forbidden("只有冻结提交人可以启动卡券销售审批".to_string()));
        }
        ensure_pending_stage(&facts.order, ReviewStatus::PendingSalesLeader)?;
        ensure_submission_in_review(&facts.submission)?;
        self.ensure_review_absent(&facts.submission, SalesReviewStage::SalesLeader, executor)
            .await
    }

    async fn record_manager_approval(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut facts = self
            .validate_decision(
                ApprovalBusinessAction::RecordSalesManagerApproval,
                context,
                executor,
            )
            .await?;
        let at = decision_time(&facts.step)?;
        let review = new_review(
            &facts.order,
            &facts.submission,
            SalesReviewStage::SalesLeader,
            SalesOrderReviewDecision::Approved,
            context,
            at,
        )?;
        let workflow = new_workflow_action(
            &facts.order,
            WorkflowActionType::Approve,
            ReviewStatus::PendingSalesLeader.as_str(),
            ReviewStatus::PendingOperations.as_str(),
            &facts.work_item.owner_role,
            context,
        )?;
        facts
            .order
            .transition_review(ReviewStatus::PendingOperations, &context.actor_id)?;

        self.db.sales_order_reviews().create(&review, executor).await?;
        self.db.sales_orders().update(&mut facts.order, executor).await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(())
    }

    async fn reject_current_stage(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let action = match self.decision_guard.as_ref().map(|guard| guard.work_item_type) {
            Some(WorkItemType::CardSalesManagerApproval) => {
                ApprovalBusinessAction::RejectCardSalesBySalesManager
            }
            Some(WorkItemType::CardSalesOperationApproval) => {
                ApprovalBusinessAction::RejectCardSalesByOperations
            }
            _ => {
                return Err(Error::BusinessLogicError(
                    "卡券销售驳回缺少强类型业务护栏".to_string(),
                ));
            }
        };
        let mut facts = self.validate_decision(action, context, executor).await?;
        let at = decision_time(&facts.step)?;
        let review = new_review(
            &facts.order,
            &facts.submission,
            facts.stage,
            SalesOrderReviewDecision::Rejected,
            context,
            at,
        )?;
        let from_status = facts.order.review_status.as_str();
        let workflow = new_workflow_action(
            &facts.order,
            WorkflowActionType::Reject,
            from_status,
            CommercialStatus::Draft.as_str(),
            &facts.work_item.owner_role,
            context,
        )?;
        facts.order.return_to_draft(&context.actor_id)?;
        facts.submission.reject(&context.actor_id)?;

        self.db.sales_order_reviews().create(&review, executor).await?;
        self.db.sales_orders().update(&mut facts.order, executor).await?;
        self.db
            .sales_order_submissions()
            .update(&mut facts.submission, executor)
            .await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(())
    }

    async fn terminate_current_stage(
        &self,
        action: ApprovalBusinessAction,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut facts = self.validate_decision(action, context, executor).await?;
        let from_status = facts.order.review_status.as_str();
        let workflow = new_workflow_action(
            &facts.order,
            WorkflowActionType::Complete,
            from_status,
            CommercialStatus::Draft.as_str(),
            &facts.work_item.owner_role,
            context,
        )?;
        facts.order.return_to_draft(&context.actor_id)?;
        facts.submission.mark_superseded(&context.actor_id)?;

        self.db.sales_orders().update(&mut facts.order, executor).await?;
        self.db
            .sales_order_submissions()
            .update(&mut facts.submission, executor)
            .await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(())
    }

    async fn approve_and_activate(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut facts = self
            .validate_decision(
                ApprovalBusinessAction::ApproveAndActivateCardSales,
                context,
                executor,
            )
            .await?;
        let at = decision_time(&facts.step)?;
        let review = new_review(
            &facts.order,
            &facts.submission,
            SalesReviewStage::Operations,
            SalesOrderReviewDecision::Approved,
            context,
            at,
        )?;
        let workflow = new_workflow_action(
            &facts.order,
            WorkflowActionType::Approve,
            ReviewStatus::PendingOperations.as_str(),
            CommercialStatus::Effective.as_str(),
            &facts.work_item.owner_role,
            context,
        )?;
        let actor = AuditActor::new(
            context.actor_id.clone(),
            context.actor_id.clone(),
            AccountKind::Admin,
        );
        let revision = build_revision(
            &facts.order,
            &facts.submission,
            &facts.lines,
            RevisionSource::ErpApproval,
            at,
            &actor,
        )?;
        let voucher_line = exactly_one_voucher_revision(&revision)?;
        let (receivable, receivable_entry) =
            build_card_receivable(&facts.order, &facts.submission, &revision, context, at)?;
        let funds_review_work_item = build_card_funds_review_work_item(
            &receivable,
            &revision,
            &facts.work_item.owner_organization_id,
            at,
        )?;
        let (projection, projection_revision, delivery) =
            build_card_projection(&facts.order, &facts.submission, &revision, voucher_line, at)?;
        let mut document = facts.document;

        facts.order.approve(at, &context.actor_id)?;
        facts
            .order
            .attach_revision(revision.revision.base.id.clone(), &context.actor_id);
        facts.submission.approve(&context.actor_id)?;
        document.formalize(at);

        self.db.sales_order_reviews().create(&review, executor).await?;
        self.db
            .sales_order()
            .formalize_submission(
                &mut facts.order,
                &revision.revision,
                &revision.lines,
                &revision.goods_lines,
                &revision.voucher_lines,
                executor,
            )
            .await?;
        self.db
            .sales_order_submissions()
            .update(&mut facts.submission, executor)
            .await?;
        self.db
            .receivable()
            .create_receivable_with_entry(&receivable, &receivable_entry, executor)
            .await?;
        self.db
            .work_items()
            .create(&funds_review_work_item, executor)
            .await?;
        self.db
            .projection()
            .create_projection_revision(&projection, &projection_revision, executor)
            .await?;
        self.db
            .sales_order_projection_deliveries()
            .create(&delivery, executor)
            .await?;
        self.db
            .business_documents()
            .update(&mut document, executor)
            .await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(())
    }

    async fn cancel_approval(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut facts = self.load_card_facts(context, executor).await?;
        let instance = self.load_instance(context, executor).await?;
        if !matches!(
            instance.status,
            ApprovalInstanceStatus::Running | ApprovalInstanceStatus::Blocked
        ) {
            return Err(Error::ConflictError("审批实例当前不可撤回".to_string()));
        }
        if instance.started_by != context.actor_id || facts.submission.submitted_by != context.actor_id {
            return Err(Error::Forbidden("仅本次提交人可以撤回审批".to_string()));
        }
        ensure_pending_stage(&facts.order, ReviewStatus::PendingSalesLeader)?;
        ensure_submission_in_review(&facts.submission)?;
        self.ensure_review_absent(&facts.submission, SalesReviewStage::SalesLeader, executor)
            .await?;
        self.ensure_review_absent(&facts.submission, SalesReviewStage::Operations, executor)
            .await?;
        let step_id = context
            .approval_step_instance_id
            .as_deref()
            .ok_or_else(|| Error::ValidationError("撤回上下文缺少审批步骤".to_string()))?;
        let step = self
            .db
            .approval_step_instances()
            .find_by_id(step_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("当前审批步骤不存在".to_string()))?;
        if step.approval_instance_id.to_string() != instance.base.id
            || instance
                .current_step_instance_id
                .as_ref()
                .map(ToString::to_string)
                != Some(step.base.id.clone())
            || step.step_key != SALES_MANAGER_APPROVAL
            || step.decision.is_some()
            || step.decided_by.is_some()
            || step.decided_at.is_some()
            || !matches!(
                step.status,
                ApprovalStepStatus::Active | ApprovalStepStatus::Blocked
            )
        {
            return Err(Error::ConflictError("撤回上下文与审批当前步骤不一致".to_string()));
        }
        if let Some(work_item_id) = context.work_item_id.as_deref() {
            let item = self
                .db
                .work_items()
                .find_by_id(work_item_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("当前审批任务不存在".to_string()))?;
            ensure_work_item_relation(&item, context, Some(&step))?;
            if item.work_item_type != WorkItemType::CardSalesManagerApproval {
                return Err(Error::ConflictError(
                    "当前审批任务已不属于可撤回的销售领导步骤".to_string(),
                ));
            }
        } else if step.status != ApprovalStepStatus::Blocked {
            return Err(Error::ConflictError("活动审批步骤缺少当前开放待办".to_string()));
        }
        let workflow = new_workflow_action(
            &facts.order,
            WorkflowActionType::Complete,
            facts.order.review_status.as_str(),
            CommercialStatus::Draft.as_str(),
            SALES_ROLE,
            context,
        )?;
        facts.order.return_to_draft(&context.actor_id)?;
        facts.submission.mark_superseded(&context.actor_id)?;

        self.db.sales_orders().update(&mut facts.order, executor).await?;
        self.db
            .sales_order_submissions()
            .update(&mut facts.submission, executor)
            .await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(())
    }

    async fn validate_recovery(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let facts = self.load_card_facts(context, executor).await?;
        let instance = self.load_instance(context, executor).await?;
        if instance.status != ApprovalInstanceStatus::Blocked {
            return Err(Error::ConflictError(
                "只有阻塞中的卡券销售审批可以恢复".to_string(),
            ));
        }
        let step_id = context
            .approval_step_instance_id
            .as_deref()
            .ok_or_else(|| Error::ValidationError("恢复上下文缺少审批步骤".to_string()))?;
        let step = self
            .db
            .approval_step_instances()
            .find_by_id(step_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("审批步骤不存在".to_string()))?;
        if step.status != ApprovalStepStatus::Blocked
            || step.approval_instance_id.to_string() != instance.base.id
            || instance
                .current_step_instance_id
                .as_ref()
                .map(ToString::to_string)
                != Some(step.base.id.clone())
        {
            return Err(Error::ConflictError("审批阻塞位置与恢复上下文不一致".to_string()));
        }
        let recovery_metadata = match step.step_key.as_str() {
            SALES_MANAGER_APPROVAL => manager_metadata(ApprovalDecision::Approve),
            OPERATIONS_APPROVAL => operations_metadata(ApprovalDecision::Approve),
            _ => {
                return Err(Error::BusinessLogicError(
                    "阻塞步骤不属于卡券销售审批注册表".to_string(),
                ));
            }
        };
        ensure_pending_stage(&facts.order, recovery_metadata.review_status)?;
        ensure_submission_in_review(&facts.submission)?;
        self.ensure_review_absent(&facts.submission, recovery_metadata.stage, executor)
            .await?;
        if recovery_metadata.stage == SalesReviewStage::Operations {
            self.ensure_manager_approved(&facts.submission, executor).await?;
        }
        if let Some(work_item_id) = context.work_item_id.as_deref() {
            let item = self
                .db
                .work_items()
                .find_by_id(work_item_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("阻塞审批原任务不存在".to_string()))?;
            ensure_work_item_relation(&item, context, Some(&step))?;
            if item.work_item_type != recovery_metadata.work_item_type
                || item.owner_role != recovery_metadata.owner_role
                || item.assignment_mode != recovery_metadata.assignment_mode
            {
                return Err(Error::BusinessLogicError(
                    "阻塞审批原任务与冻结步骤注册表不一致".to_string(),
                ));
            }
        }
        Ok(())
    }

    async fn validate_decision(
        &self,
        action: ApprovalBusinessAction,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<DecisionFacts> {
        let metadata = decision_metadata(action)
            .ok_or_else(|| Error::BusinessLogicError("动作不是卡券销售正式决定".to_string()))?;
        let card = self.load_card_facts(context, executor).await?;
        ensure_pending_stage(&card.order, metadata.review_status)?;
        ensure_submission_in_review(&card.submission)?;
        let instance = self.load_instance(context, executor).await?;
        if instance.status != ApprovalInstanceStatus::Running {
            return Err(Error::ConflictError("审批实例当前不可形成业务决定".to_string()));
        }
        let step_id = context
            .approval_step_instance_id
            .as_deref()
            .ok_or_else(|| Error::ValidationError("审批决定上下文缺少步骤".to_string()))?;
        let step = self
            .db
            .approval_step_instances()
            .find_by_id(step_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("审批步骤不存在".to_string()))?;
        ensure_decided_step(&step, &instance, context, metadata)?;
        let work_item_id = context
            .work_item_id
            .as_deref()
            .ok_or_else(|| Error::ValidationError("审批决定上下文缺少任务".to_string()))?;
        let work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("当前审批任务不存在".to_string()))?;
        ensure_work_item_relation(&work_item, context, Some(&step))?;
        if work_item.work_item_type != metadata.work_item_type
            || work_item.owner_role != metadata.owner_role
            || work_item.assignment_mode != metadata.assignment_mode
        {
            return Err(Error::BusinessLogicError(
                "当前审批任务与注册步骤不一致".to_string(),
            ));
        }
        if work_item.owner_user_id.as_deref() != Some(context.actor_id.as_str()) {
            return Err(Error::Forbidden("当前账号不是该审批任务责任人".to_string()));
        }
        if !ApprovalAssigneeResolver::new(self.db.clone())
            .user_is_eligible_for_assignment(
                &context.actor_id,
                &work_item.owner_role,
                &work_item.owner_organization_id,
                executor,
            )
            .await?
        {
            return Err(Error::Forbidden(
                "当前责任人已不具备审批角色或组织范围资格".to_string(),
            ));
        }
        if context.actor_id == instance.started_by || context.actor_id == card.submission.submitted_by {
            return Err(Error::Forbidden("提交人与审批人必须岗位分离".to_string()));
        }
        if metadata.stage == SalesReviewStage::Operations {
            if self
                .db
                .sales_order_projections()
                .find_by_sales_order_and_mall(
                    &SalesOrderId::new(card.order.base.id.clone()),
                    card.submission.target_mall_id.as_ref().ok_or_else(|| {
                        Error::BusinessLogicError("卡券销售提交缺少冻结目标商城".to_string())
                    })?,
                    executor,
                )
                .await?
                .is_some()
            {
                return Err(Error::ConflictError(
                    "该卡券销售单的目标商城执行投影已经存在".to_string(),
                ));
            }
            if self
                .db
                .receivable_accounts()
                .find_one(
                    doc! {
                        "sales_order_id": card.order.base.id.clone(),
                        "account_seq": 1_i32,
                    },
                    executor,
                )
                .await?
                .is_some()
            {
                return Err(Error::ConflictError(
                    "该卡券销售单的原始应收子账已经存在".to_string(),
                ));
            }
        }
        self.ensure_review_absent(&card.submission, metadata.stage, executor)
            .await?;
        if metadata.stage == SalesReviewStage::Operations {
            let manager_review = self.ensure_manager_approved(&card.submission, executor).await?;
            if manager_review.reviewer_id == context.actor_id {
                return Err(Error::Forbidden(
                    "销售领导决定人与运营决定人必须岗位分离".to_string(),
                ));
            }
        }
        self.validate_decision_guard(action, context, &card, &work_item)?;
        Ok(DecisionFacts {
            order: card.order,
            submission: card.submission,
            lines: card.lines,
            step,
            work_item,
            stage: metadata.stage,
            document: card.document,
        })
    }

    fn validate_decision_guard(
        &self,
        action: ApprovalBusinessAction,
        context: &ApprovalActionContext,
        card: &CardFacts,
        work_item: &WorkItem,
    ) -> Result<()> {
        let Some(guard) = self.decision_guard.as_ref() else {
            if matches!(
                action,
                ApprovalBusinessAction::RecordSalesManagerApproval
                    | ApprovalBusinessAction::RejectCardSalesBySalesManager
                    | ApprovalBusinessAction::TerminateCardSalesBySalesManager
                    | ApprovalBusinessAction::ApproveAndActivateCardSales
                    | ApprovalBusinessAction::RejectCardSalesByOperations
                    | ApprovalBusinessAction::TerminateCardSalesByOperations
            ) {
                return Err(Error::BusinessLogicError(
                    "卡券销售正式决定缺少 W05 强类型业务护栏".to_string(),
                ));
            }
            return Ok(());
        };
        let expected_decision = match action {
            ApprovalBusinessAction::RecordSalesManagerApproval
            | ApprovalBusinessAction::ApproveAndActivateCardSales => CardSalesReviewDecision::Approve,
            ApprovalBusinessAction::RejectCardSalesBySalesManager
            | ApprovalBusinessAction::RejectCardSalesByOperations => CardSalesReviewDecision::Reject,
            ApprovalBusinessAction::TerminateCardSalesBySalesManager
            | ApprovalBusinessAction::TerminateCardSalesByOperations => CardSalesReviewDecision::Terminate,
            _ => {
                return Err(Error::BusinessLogicError(
                    "W05 强类型决定与运行时动作不兼容".to_string(),
                ));
            }
        };
        if guard.approval_instance_id != context.approval_instance_id
            || Some(guard.approval_step_instance_id.as_str()) != context.approval_step_instance_id.as_deref()
            || Some(guard.work_item_id.as_str()) != context.work_item_id.as_deref()
            || guard.expected_subject_version != context.subject_version
            || guard.sales_order_id != context.business_object_id
            || guard.submission_id != context.subject_version
            || guard.actor_id != context.actor_id
            || guard.reason != context.reason
            || guard.review_decision != expected_decision
            || guard.work_item_type != work_item.work_item_type
            || guard.expected_review_status.domain_status() != card.order.review_status
        {
            return Err(Error::ConflictError(
                "W05 强命令与当前审批业务事实不一致".to_string(),
            ));
        }
        if card.order.base.version != guard.expected_sales_order_lock_version {
            return Err(Error::ConflictError(
                "销售单已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if card.submission.submission_no != guard.expected_submission_no {
            return Err(Error::ConflictError(
                "销售提交序号已变化，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    async fn load_card_facts(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<CardFacts> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(&context.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("卡券销售单不存在".to_string()))?;
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&context.subject_version, executor)
            .await?
            .ok_or_else(|| Error::NotFound("卡券销售提交不存在".to_string()))?;
        if submission.sales_order_id.to_string() != order.base.id
            || submission.business_type != BusinessType::Voucher
            || order.business_type != BusinessType::Voucher
            || submission.customer_id != order.customer_id
            || submission.settlement_party_id != order.settlement_party_id
        {
            return Err(Error::BusinessLogicError(
                "审批对象不是该销售单的卡券冻结提交".to_string(),
            ));
        }
        ensure_frozen_projection_fields(&submission)?;
        let lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                &[SalesOrderSubmissionId::new(submission.base.id.clone())],
                executor,
            )
            .await?;
        if lines.len() != 1 || lines[0].line_type != LineType::Voucher {
            return Err(Error::BusinessLogicError(
                "卡券销售提交必须且只能包含一条卡券明细".to_string(),
            ));
        }
        let document = self
            .db
            .business_documents()
            .find_by_id(&order.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单业务单据注册不存在".to_string()))?;
        ensure_sales_document(&document, &order)?;
        Ok(CardFacts {
            order,
            submission,
            lines,
            document,
        })
    }

    async fn load_instance(
        &self,
        context: &ApprovalActionContext,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalInstance> {
        let instance = self
            .db
            .approval_instances()
            .find_by_id(&context.approval_instance_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("卡券销售审批实例不存在".to_string()))?;
        if instance.definition_key != CARD_SALES_APPROVAL
            || instance.definition_version != CARD_SALES_APPROVAL_VERSION
            || instance.runtime_kind != entities::approval::ApprovalRuntimeKind::Internal
            || instance.business_object_type != SALES_ORDER_OBJECT
            || instance.business_object_id != context.business_object_id
            || instance.subject_version != context.subject_version
        {
            return Err(Error::ConflictError(
                "审批实例与卡券销售冻结对象不一致".to_string(),
            ));
        }
        Ok(instance)
    }

    async fn ensure_review_absent(
        &self,
        submission: &SalesOrderSubmission,
        stage: SalesReviewStage,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if self
            .db
            .sales_order_reviews()
            .find_by_submission_and_stage(
                &SalesOrderSubmissionId::new(submission.base.id.clone()),
                stage,
                executor,
            )
            .await?
            .is_some()
        {
            return Err(Error::ConflictError(
                "当前提交阶段已经形成不可变审批决定".to_string(),
            ));
        }
        Ok(())
    }

    async fn ensure_manager_approved(
        &self,
        submission: &SalesOrderSubmission,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrderReview> {
        let review = self
            .db
            .sales_order_reviews()
            .find_by_submission_and_stage(
                &SalesOrderSubmissionId::new(submission.base.id.clone()),
                SalesReviewStage::SalesLeader,
                executor,
            )
            .await?
            .ok_or_else(|| Error::BusinessLogicError("运营审批缺少销售领导通过事实".to_string()))?;
        if review.status != SalesOrderReviewDecision::Approved {
            return Err(Error::BusinessLogicError(
                "运营审批的销售领导前置决定不是通过".to_string(),
            ));
        }
        Ok(review)
    }
}

impl ApprovalDomainActionPort for CardSalesApprovalActionPort {
    fn execute<'a>(
        &'a self,
        action: ApprovalBusinessAction,
        context: &'a ApprovalActionContext,
        executor: &'a mut dyn Executor,
    ) -> crate::approval::ApprovalActionFuture<'a> {
        Box::pin(async move { self.execute_action(action, context, executor).await })
    }
}

struct CardFacts {
    order: SalesOrder,
    submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
    document: BusinessDocument,
}

struct DecisionFacts {
    order: SalesOrder,
    submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
    step: ApprovalStepInstance,
    work_item: WorkItem,
    stage: SalesReviewStage,
    document: BusinessDocument,
}

#[derive(Clone, Copy)]
struct DecisionMetadata {
    step_key: &'static str,
    work_item_type: WorkItemType,
    owner_role: &'static str,
    assignment_mode: AssignmentMode,
    decision: ApprovalDecision,
    stage: SalesReviewStage,
    review_status: ReviewStatus,
}

fn decision_metadata(action: ApprovalBusinessAction) -> Option<DecisionMetadata> {
    match action {
        ApprovalBusinessAction::RecordSalesManagerApproval => {
            Some(manager_metadata(ApprovalDecision::Approve))
        }
        ApprovalBusinessAction::RejectCardSalesBySalesManager => {
            Some(manager_metadata(ApprovalDecision::RejectToApplicant))
        }
        ApprovalBusinessAction::TerminateCardSalesBySalesManager => {
            Some(manager_metadata(ApprovalDecision::TerminateApproval))
        }
        ApprovalBusinessAction::ApproveAndActivateCardSales => {
            Some(operations_metadata(ApprovalDecision::Approve))
        }
        ApprovalBusinessAction::RejectCardSalesByOperations => {
            Some(operations_metadata(ApprovalDecision::RejectToApplicant))
        }
        ApprovalBusinessAction::TerminateCardSalesByOperations => {
            Some(operations_metadata(ApprovalDecision::TerminateApproval))
        }
        _ => None,
    }
}

fn manager_metadata(decision: ApprovalDecision) -> DecisionMetadata {
    DecisionMetadata {
        step_key: SALES_MANAGER_APPROVAL,
        work_item_type: WorkItemType::CardSalesManagerApproval,
        owner_role: SALES_MANAGER_ROLE,
        assignment_mode: AssignmentMode::Direct,
        decision,
        stage: SalesReviewStage::SalesLeader,
        review_status: ReviewStatus::PendingSalesLeader,
    }
}

fn operations_metadata(decision: ApprovalDecision) -> DecisionMetadata {
    DecisionMetadata {
        step_key: OPERATIONS_APPROVAL,
        work_item_type: WorkItemType::CardSalesOperationApproval,
        owner_role: OPERATIONS_ROLE,
        assignment_mode: AssignmentMode::Pool,
        decision,
        stage: SalesReviewStage::Operations,
        review_status: ReviewStatus::PendingOperations,
    }
}

fn ensure_card_context(context: &ApprovalActionContext) -> Result<()> {
    if context.definition_key != CARD_SALES_APPROVAL
        || context.business_object_type != SALES_ORDER_OBJECT
        || context.business_object_id.trim().is_empty()
        || context.subject_version.trim().is_empty()
    {
        return Err(Error::BusinessLogicError(
            "领域动作上下文不属于 CARD_SALES_APPROVAL 销售单".to_string(),
        ));
    }
    Ok(())
}

fn ensure_decided_step(
    step: &ApprovalStepInstance,
    instance: &ApprovalInstance,
    context: &ApprovalActionContext,
    metadata: DecisionMetadata,
) -> Result<()> {
    if step.approval_instance_id.to_string() != instance.base.id
        || instance
            .current_step_instance_id
            .as_ref()
            .map(ToString::to_string)
            != Some(step.base.id.clone())
        || step.step_key != metadata.step_key
        || step.status
            != match metadata.decision {
                ApprovalDecision::Approve => ApprovalStepStatus::Approved,
                ApprovalDecision::RejectToApplicant => ApprovalStepStatus::Rejected,
                ApprovalDecision::TerminateApproval => ApprovalStepStatus::Terminated,
            }
        || step.decision != Some(metadata.decision)
        || step.decided_by.as_deref() != Some(context.actor_id.as_str())
    {
        return Err(Error::ConflictError(
            "当前审批步骤决定与注册动作不一致".to_string(),
        ));
    }
    Ok(())
}

fn ensure_work_item_relation(
    item: &WorkItem,
    context: &ApprovalActionContext,
    step: Option<&ApprovalStepInstance>,
) -> Result<()> {
    if item.status != WorkItemStatus::Open
        || item.business_object_type != SALES_ORDER_OBJECT
        || item.business_object_id != context.business_object_id
        || item.subject_version != context.subject_version
        || step.is_some_and(|step| item.approval_step_instance_id.as_deref() != Some(step.base.id.as_str()))
    {
        return Err(Error::ConflictError(
            "当前开放待办与卡券销售审批上下文不一致".to_string(),
        ));
    }
    Ok(())
}

fn ensure_pending_stage(order: &SalesOrder, expected: ReviewStatus) -> Result<()> {
    if order.commercial_status != CommercialStatus::PendingReview
        || order.review_status != expected
        || order.stable.current_revision_id.is_some()
    {
        return Err(Error::ConflictError(
            "卡券销售单已不处于命令锁定的审批状态".to_string(),
        ));
    }
    Ok(())
}

fn ensure_submission_in_review(submission: &SalesOrderSubmission) -> Result<()> {
    if submission.stable.status != SubmissionStatus::InReview {
        return Err(Error::ConflictError("卡券销售冻结提交已不在审批中".to_string()));
    }
    Ok(())
}

fn ensure_frozen_projection_fields(submission: &SalesOrderSubmission) -> Result<()> {
    if submission.target_mall_id.is_none()
        || submission.customer_external_identity.is_none()
        || submission.voucher_category_external_identity.is_none()
        || submission.receivable_due_date.is_none()
        || submission.voucher_expiry_at.is_none()
        || submission.voucher_category_sku_id.is_none()
    {
        return Err(Error::BusinessLogicError(
            "卡券销售提交缺少服务端冻结的投影或应收字段".to_string(),
        ));
    }
    Ok(())
}

fn ensure_sales_document(document: &BusinessDocument, order: &SalesOrder) -> Result<()> {
    if document.document_type != DocumentType::SalesOrder
        || document.document_no != order.order_no
        || document.formalized_at.is_some()
    {
        return Err(Error::ConflictError(
            "销售单业务单据注册与待正式化对象不一致".to_string(),
        ));
    }
    Ok(())
}

fn decision_time(step: &ApprovalStepInstance) -> Result<Instant> {
    step.decided_at
        .ok_or_else(|| Error::Internal("已决定审批步骤缺少决定时间".to_string()))
}

fn new_review(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    stage: SalesReviewStage,
    decision: SalesOrderReviewDecision,
    context: &ApprovalActionContext,
    at: Instant,
) -> Result<SalesOrderReview> {
    SalesOrderReview::new(
        SalesOrderReviewId::new(next_id()),
        SalesOrderReviewData {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            submission_id: SalesOrderSubmissionId::new(submission.base.id.clone()),
            review_stage: stage,
            status: decision,
            reviewer_id: context.actor_id.clone(),
            reviewed_at: at,
            decision_reason: context.reason.clone(),
        },
    )
    .map_err(Into::into)
}

fn new_workflow_action(
    order: &SalesOrder,
    action_type: WorkflowActionType,
    from_status: &str,
    to_status: &str,
    actor_role: &str,
    context: &ApprovalActionContext,
) -> Result<WorkflowAction> {
    WorkflowAction::new(
        card_workflow_action_id(
            context
                .approval_step_instance_id
                .as_deref()
                .ok_or_else(|| Error::Internal("工作流动作缺少审批步骤身份".to_string()))?,
        ),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(order.base.id.clone()),
            action_type,
            from_status: from_status.to_string(),
            to_status: to_status.to_string(),
            actor_id: context.actor_id.clone(),
            actor_role: actor_role.to_string(),
            comment: context.reason.clone(),
        },
    )
    .map_err(Into::into)
}

fn card_workflow_action_id(step_instance_id: &str) -> WorkflowActionId {
    WorkflowActionId::new(format!("card-sales-workflow-{step_instance_id}"))
}

fn exactly_one_voucher_revision(
    revision: &RevisionAggregate,
) -> Result<&entities::sales_order::SalesOrderVoucherLineRevision> {
    let [voucher] = revision.voucher_lines.as_slice() else {
        return Err(Error::BusinessLogicError(
            "卡券销售正式版本必须且只能包含一条卡券明细".to_string(),
        ));
    };
    if !revision.goods_lines.is_empty() || revision.lines.len() != 1 {
        return Err(Error::BusinessLogicError(
            "卡券销售正式版本包含不允许的明细".to_string(),
        ));
    }
    Ok(voucher)
}

fn build_card_receivable(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    revision: &RevisionAggregate,
    context: &ApprovalActionContext,
    at: Instant,
) -> Result<(ReceivableAccount, ReceivableEntry)> {
    let zero =
        Amount::from_str("0.00").map_err(|error| Error::Internal(format!("无法构造零金额: {error}")))?;
    let account_id = ReceivableAccountId::new(next_id());
    let account = ReceivableAccount::new(
        account_id.clone(),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            account_seq: 1,
            customer_id: submission.customer_id.clone(),
            counterparty_party_id: submission.settlement_party_id.clone(),
            source_sales_order_revision_id: revision.revision.base.id.clone().into(),
            review_status: AccountReviewStatus::OpeningPending,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: submission.gross_amount,
            settled_total: zero,
            invoiceable_total: submission.gross_amount,
            invoiced_total: zero,
        },
        context.actor_id.clone(),
    )?;
    let due_date = submission
        .receivable_due_date
        .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结应收到期日".to_string()))?;
    let entry = ReceivableEntry::new(
        ReceivableEntryId::new(next_id()),
        ReceivableEntryData {
            receivable_account_id: account_id,
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: submission.gross_amount,
            due_date,
            source_fact_type: "SALES_ORDER".to_string(),
            source_document_id: order.base.id.clone(),
            source_revision_id: revision.revision.base.id.clone(),
            source_sequence: 1,
            posted_at: at,
        },
    )?;
    Ok((account, entry))
}

fn build_card_funds_review_work_item(
    account: &ReceivableAccount,
    revision: &RevisionAggregate,
    owner_organization_id: &str,
    at: Instant,
) -> Result<WorkItem> {
    WorkItem::new_at(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::CardFundsReview,
            approval_step_instance_id: None,
            business_object_type: "receivable_account".to_string(),
            business_object_id: account.base.id.clone(),
            subject_version: revision.revision.base.id.clone(),
            assignment_mode: AssignmentMode::Pool,
            owner_role: "role-finance".to_string(),
            owner_organization_id: owner_organization_id.to_string(),
            owner_user_id: None,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some("CARD_FUNDS_OPENING_REVIEW".to_string()),
            impact_summary: Some("卡券销售生效后核对期初回款与开票事实".to_string()),
        },
        at,
    )
    .map_err(Into::into)
}

fn build_card_projection(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    revision: &RevisionAggregate,
    voucher: &entities::sales_order::SalesOrderVoucherLineRevision,
    at: Instant,
) -> Result<(
    SalesOrderProjection,
    SalesOrderProjectionRevision,
    SalesOrderProjectionDelivery,
)> {
    let target_mall_id = submission
        .target_mall_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结目标商城".to_string()))?;
    let projection_id = SalesOrderProjectionId::new(next_id());
    let projection = SalesOrderProjection::new(
        projection_id.clone(),
        SalesOrderProjectionData {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            target_mall_id: target_mall_id.clone(),
        },
    )?;
    let projection_revision_id = SalesOrderProjectionRevisionId::new(next_id());
    let mut projection_revision = SalesOrderProjectionRevision::new(
        projection_revision_id.clone(),
        1,
        SalesOrderProjectionRevisionData {
            projection_id,
            projection_source: ProjectionSource::ErpRevision,
            sales_order_revision_id: revision.revision.base.id.clone().into(),
            customer_external_identity: submission
                .customer_external_identity
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结商城客户身份".to_string()))?,
            voucher_category_external_identity: submission
                .voucher_category_external_identity
                .clone()
                .ok_or_else(|| {
                    Error::BusinessLogicError("卡券销售提交缺少冻结商城卡券类目身份".to_string())
                })?,
            voucher_expiry_at: submission
                .voucher_expiry_at
                .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结履约期限".to_string()))?,
            face_value: voucher.face_value,
            card_count: voucher.card_count,
            card_form: match voucher.card_form {
                CardForm::Electronic => ProjectionCardForm::Electronic,
                CardForm::Physical => ProjectionCardForm::Physical,
            },
            effective_at: at,
            content_hash: "pending".to_string(),
        },
    )?;
    projection_revision.content_hash = projection_content_hash(&projection_revision);
    let delivery = SalesOrderProjectionDelivery::new(
        SalesOrderProjectionDeliveryId::new(next_id()),
        SalesOrderProjectionDeliveryData {
            projection_revision_id,
            target_mall_id,
            status: ProjectionDeliveryStatus::PendingSend,
            attempt_count: 0,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_execution_baseline: None,
            error_code: None,
            error_summary: None,
        },
    )?;
    Ok((projection, projection_revision, delivery))
}

fn validate_fixed_stage(
    work_item_type: WorkItemType,
    review_status: CardSalesExpectedReviewStatus,
) -> Result<()> {
    if !matches!(
        (work_item_type, review_status),
        (
            WorkItemType::CardSalesManagerApproval,
            CardSalesExpectedReviewStatus::PendingSalesLead
        ) | (
            WorkItemType::CardSalesOperationApproval,
            CardSalesExpectedReviewStatus::PendingOperations
        )
    ) {
        return Err(Error::ValidationError(
            "任务类型与卡券销售审核状态不匹配".to_string(),
        ));
    }
    Ok(())
}

fn validate_text(value: &str, label: &str, max: usize) -> Result<()> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(Error::ValidationError(format!("{label}不能为空")));
    }
    if normalized != value {
        return Err(Error::ValidationError(format!("{label}不得包含首尾空白")));
    }
    if value.chars().count() > max {
        return Err(Error::ValidationError(format!("{label}过长")));
    }
    Ok(())
}

fn normalize_optional(value: Option<String>, label: &str, max: usize) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max {
        return Err(Error::ValidationError(format!("{label}过长")));
    }
    Ok(Some(value))
}

fn parse_version(value: &str, label: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{label}期望版本必须是正整数")))?;
    if parsed == 0 {
        return Err(Error::ValidationError(format!("{label}期望版本必须是正整数")));
    }
    Ok(parsed)
}

impl SalesReviewService {
    /// 分页查询已形成的不可变销售审批决定。
    pub async fn sales_order_review_list(
        &self,
        params: &SalesOrderReviewListParams,
    ) -> Result<PageView<SalesOrderReviewView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderReviewFilter {
            submission_id: query.submission_id.map(SalesOrderSubmissionId::new),
            sales_order_id: query.sales_order_id.map(SalesOrderId::new),
            review_stage: query.review_stage,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .sales_order_reviews()
            .search_sales_order_reviews(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesOrderReviewView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                submission_id: row.submission_id,
                review_stage: row.review_stage,
                status: row.status,
                reviewer_id: row.reviewer_id,
                reviewed_at: row.reviewed_at,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 在审批事务提交后重读正式业务事实并形成 W05 决策响应。
    ///
    /// # 错误
    /// 运行时结果与已提交的不可变审批、工作流、版本、应收或投影事实不一致时失败。
    pub async fn card_sales_decision_result(
        &self,
        approval: ApprovalRuntimeView,
        guard: &CardSalesApprovalDecisionGuard,
    ) -> Result<SubmitCardSalesApprovalDecisionResult> {
        let review = if guard.review_decision == CardSalesReviewDecision::Terminate {
            None
        } else {
            Some(
                self.db
                    .sales_order_reviews()
                    .find_by_submission_and_stage(
                        &SalesOrderSubmissionId::new(guard.submission_id.clone()),
                        guard.review_stage()?,
                        &mut NoTransaction,
                    )
                    .await?
                    .ok_or_else(|| Error::Internal("审批事务已提交但决定记录不存在".to_string()))?,
            )
        };
        let order = self
            .db
            .sales_orders()
            .find_by_id(&guard.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("审批事务已提交但销售单不存在".to_string()))?;
        let workflow = self
            .db
            .workflow_actions()
            .find_by_id(
                card_workflow_action_id(&guard.approval_step_instance_id).as_ref(),
                &mut NoTransaction,
            )
            .await?;
        let workflow = workflow
            .filter(|action| {
                action.actor_id == guard.actor_id
                    && action.document_id.to_string() == guard.sales_order_id
                    && match guard.review_decision {
                        CardSalesReviewDecision::Approve => action.action_type == WorkflowActionType::Approve,
                        CardSalesReviewDecision::Reject => action.action_type == WorkflowActionType::Reject,
                        CardSalesReviewDecision::Terminate => {
                            action.action_type == WorkflowActionType::Complete
                        }
                    }
            })
            .ok_or_else(|| Error::Internal("审批事务已提交但工作流动作不存在".to_string()))?;
        let business_result = match (guard.work_item_type, guard.review_decision) {
            (WorkItemType::CardSalesManagerApproval, CardSalesReviewDecision::Approve) => {
                let review = review
                    .as_ref()
                    .ok_or_else(|| Error::Internal("销售领导通过缺少审核记录".to_string()))?;
                CardSalesApprovalBusinessResult::ManagerApproved {
                    sales_order_id: order.base.id.clone(),
                    sales_order_review_id: review.base.id.clone(),
                    workflow_action_id: workflow.base.id,
                    sales_order_commercial_status: ReviewStatus::PendingOperations.as_str().to_string(),
                    next_work_item_id: approval.work_item.as_ref().map(|item| item.id.clone()),
                    next_work_item_status: approval.work_item.as_ref().map(|item| item.status),
                }
            }
            (WorkItemType::CardSalesOperationApproval, CardSalesReviewDecision::Approve) => {
                let review = review
                    .as_ref()
                    .ok_or_else(|| Error::Internal("运营通过缺少审核记录".to_string()))?;
                let revision_id = order
                    .stable
                    .current_revision_id
                    .clone()
                    .ok_or_else(|| Error::Internal("生效销售单缺少当前版本".to_string()))?;
                let account = self
                    .db
                    .receivable_accounts()
                    .find_one(
                        doc! {
                            "sales_order_id": order.base.id.clone(),
                            "account_seq": 1_i32,
                        },
                        &mut NoTransaction,
                    )
                    .await?
                    .ok_or_else(|| Error::Internal("生效销售单缺少原始应收子账".to_string()))?;
                let projection_revision = self
                    .db
                    .sales_order_projection_revisions()
                    .find_one(
                        doc! { "sales_order_revision_id": revision_id.clone() },
                        &mut NoTransaction,
                    )
                    .await?
                    .ok_or_else(|| Error::Internal("生效销售单缺少执行投影版本".to_string()))?;
                let delivery = self
                    .db
                    .sales_order_projection_deliveries()
                    .find_one(
                        doc! { "projection_revision_id": projection_revision.base.id.clone() },
                        &mut NoTransaction,
                    )
                    .await?
                    .ok_or_else(|| Error::Internal("执行投影版本缺少下发操作".to_string()))?;
                CardSalesApprovalBusinessResult::OperationsApprovedAndEffective {
                    sales_order_id: order.base.id.clone(),
                    sales_order_review_id: review.base.id.clone(),
                    workflow_action_id: workflow.base.id,
                    sales_order_commercial_status: CommercialStatus::Effective.as_str().to_string(),
                    sales_order_revision_id: revision_id,
                    receivable_account_id: account.base.id,
                    execution_projection_operation_id: delivery.base.id,
                }
            }
            (_, CardSalesReviewDecision::Reject) => {
                let review = review
                    .as_ref()
                    .ok_or_else(|| Error::Internal("审批驳回缺少审核记录".to_string()))?;
                CardSalesApprovalBusinessResult::RejectedToSales {
                    sales_order_id: order.base.id.clone(),
                    sales_order_review_id: review.base.id.clone(),
                    workflow_action_id: workflow.base.id.clone(),
                    sales_order_commercial_status: order.commercial_status.as_str().to_string(),
                }
            }
            (_, CardSalesReviewDecision::Terminate) => {
                let submission = self
                    .db
                    .sales_order_submissions()
                    .find_by_id(&guard.submission_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::Internal("终止审批后冻结提交不存在".to_string()))?;
                if approval.instance.status != ApprovalInstanceStatus::Terminated
                    || order.commercial_status != CommercialStatus::Draft
                    || order.review_status != ReviewStatus::NotSubmitted
                    || submission.stable.status != SubmissionStatus::Superseded
                {
                    return Err(Error::Internal("终止审批未形成销售草稿与提交终态".to_string()));
                }
                CardSalesApprovalBusinessResult::Terminated {
                    sales_order_id: order.base.id.clone(),
                    sales_order_submission_id: submission.base.id,
                    submission_status: submission.stable.status,
                    workflow_action_id: workflow.base.id.clone(),
                    sales_order_commercial_status: order.commercial_status.as_str().to_string(),
                }
            }
            _ => {
                return Err(Error::Internal(
                    "W05 强命令形成了不可能的业务结果组合".to_string(),
                ));
            }
        };
        Ok(SubmitCardSalesApprovalDecisionResult {
            approval_instance_status: approval.instance.status,
            work_item_id: guard.work_item_id.clone(),
            work_item_status: WorkItemStatus::Completed,
            business_result,
            approval,
        })
    }

    /// 在审批撤回事务提交后重读销售单、冻结提交和追加式动作事实。
    ///
    /// # 错误
    /// 运行时未进入取消终态，或事务提交后的业务/待办事实不完整时失败。
    pub async fn card_sales_cancel_result(
        &self,
        approval: ApprovalRuntimeView,
        guard: &CardSalesApprovalCancelGuard,
    ) -> Result<CancelCardSalesApprovalResult> {
        let approval = self.cancelled_runtime_result(approval, guard).await?;
        ensure_cancelled_runtime_result(&approval, guard)?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&approval.instance.business_object_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("撤回事务已提交但销售单不存在".to_string()))?;
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&approval.instance.subject_version, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("撤回事务已提交但冻结提交不存在".to_string()))?;
        ensure_cancelled_business_result(&order, &submission, &approval)?;
        let workflow = self
            .db
            .workflow_actions()
            .find_by_id(
                card_workflow_action_id(&guard.current_step_instance_id).as_ref(),
                &mut NoTransaction,
            )
            .await?
            .filter(|action| {
                action.actor_id == guard.actor_id
                    && action.document_id.to_string() == order.base.id
                    && action.action_type == WorkflowActionType::Complete
                    && action.from_status == ReviewStatus::PendingSalesLeader.as_str()
                    && action.to_status == CommercialStatus::Draft.as_str()
                    && action.actor_role == SALES_ROLE
            })
            .ok_or_else(|| Error::Internal("撤回事务已提交但工作流动作不存在".to_string()))?;
        let work_item_id = approval.work_item.as_ref().map(|item| item.id.clone());
        let work_item_status = approval.work_item.as_ref().map(|item| item.status);
        let business_result = CardSalesApprovalCancelledBusinessResult {
            outcome: "CANCELLED_TO_EDITABLE_DRAFT",
            sales_order_id: order.base.id.clone(),
            sales_order_version: order.base.version.to_string(),
            sales_order_commercial_status: order.commercial_status,
            sales_order_review_status: order.review_status,
            sales_order_submission_id: submission.base.id.clone(),
            submission_version: submission.base.version.to_string(),
            submission_status: submission.stable.status,
            workflow_action_id: workflow.base.id,
        };
        Ok(CancelCardSalesApprovalResult {
            approval_instance_status: approval.instance.status,
            work_item_id,
            work_item_status,
            business_result,
            approval,
        })
    }

    /// 按强命令锁定的原当前步骤与待办重建撤回终态视图。
    ///
    /// 通用运行时在幂等重放终态实例时可能选择最高序号的已取消步骤；业务响应
    /// 必须始终返回本次命令实际取消的原当前步骤及其关闭待办。
    async fn cancelled_runtime_result(
        &self,
        approval: ApprovalRuntimeView,
        guard: &CardSalesApprovalCancelGuard,
    ) -> Result<ApprovalRuntimeView> {
        let step = self
            .db
            .approval_step_instances()
            .find_by_id(&guard.current_step_instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("撤回事务已提交但原当前步骤不存在".to_string()))?;
        let work_item = match guard.work_item_id.as_deref() {
            Some(id) => {
                let item = self
                    .db
                    .work_items()
                    .find_by_id(id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::Internal("撤回事务已提交但原待办不存在".to_string()))?;
                if item.work_item_type != WorkItemType::CardSalesManagerApproval
                    || item.business_object_type != approval.instance.business_object_type
                    || item.business_object_id != approval.instance.business_object_id
                    || item.subject_version != approval.instance.subject_version
                    || item.approval_step_instance_id.as_deref()
                        != Some(guard.current_step_instance_id.as_str())
                {
                    return Err(Error::Internal(
                        "撤回事务已提交但原待办与审批对象不一致".to_string(),
                    ));
                }
                Some(item)
            }
            None => None,
        };
        Ok(ApprovalRuntimeView {
            instance: approval.instance,
            step: step.into(),
            work_item: work_item.map(Into::into),
        })
    }
}

/// 校验撤回响应仍锚定强命令锁定的原领导步骤及其可选关闭待办。
fn ensure_cancelled_runtime_result(
    approval: &ApprovalRuntimeView,
    guard: &CardSalesApprovalCancelGuard,
) -> Result<()> {
    if approval.instance.definition_key != CARD_SALES_APPROVAL
        || approval.instance.id != guard.approval_instance_id
        || approval.instance.subject_version != guard.expected_subject_version
        || approval.instance.started_by != guard.actor_id
        || approval.instance.status != ApprovalInstanceStatus::Cancelled
        || approval.step.id != guard.current_step_instance_id
        || approval.step.approval_instance_id != guard.approval_instance_id
        || approval.step.step_key != SALES_MANAGER_APPROVAL
        || approval.step.status != ApprovalStepStatus::Cancelled
        || approval.step.decision.is_some()
        || approval.step.decided_by.is_some()
        || approval.step.decided_at.is_some()
        || approval.instance.current_step_instance_id.is_some()
        || approval.work_item.as_ref().map(|item| item.id.as_str()) != guard.work_item_id.as_deref()
        || approval.work_item.as_ref().is_some_and(|item| {
            item.status != WorkItemStatus::Closed
                || item.work_item_type != WorkItemType::CardSalesManagerApproval
                || item.approval_step_instance_id.as_deref() != Some(guard.current_step_instance_id.as_str())
        })
    {
        return Err(Error::Internal(
            "卡券审批撤回未形成完整的实例、步骤和待办终态".to_string(),
        ));
    }
    Ok(())
}

/// 校验撤回领域动作已恢复销售草稿并终结原冻结提交。
fn ensure_cancelled_business_result(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    approval: &ApprovalRuntimeView,
) -> Result<()> {
    if order.base.id != approval.instance.business_object_id
        || submission.base.id != approval.instance.subject_version
        || submission.sales_order_id.to_string() != order.base.id
        || order.commercial_status != CommercialStatus::Draft
        || order.review_status != ReviewStatus::NotSubmitted
        || order.stable.current_revision_id.is_some()
        || submission.stable.status != SubmissionStatus::Superseded
    {
        return Err(Error::Internal(
            "卡券审批撤回未恢复为可重新编辑的销售草稿".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn command(decision: CardSalesReviewDecision) -> SubmitCardSalesApprovalDecisionCommand {
        SubmitCardSalesApprovalDecisionCommand {
            approval_instance_id: "instance-1".to_string(),
            expected_instance_version: "2".to_string(),
            approval_step_instance_id: "step-1".to_string(),
            expected_step_version: "3".to_string(),
            work_item_id: "task-1".to_string(),
            expected_task_version: "4".to_string(),
            expected_subject_version: "submission-1".to_string(),
            decision: CardSalesApprovalDecision {
                sales_order_id: "order-1".to_string(),
                sales_order_submission_id: "submission-1".to_string(),
                expected_sales_order_lock_version: 5,
                expected_submission_no: 1,
                work_item_type: WorkItemType::CardSalesManagerApproval,
                expected_review_status: CardSalesExpectedReviewStatus::PendingSalesLead,
                review_decision: decision,
                reason_code: matches!(
                    decision,
                    CardSalesReviewDecision::Reject | CardSalesReviewDecision::Terminate
                )
                .then(|| "COMMERCIAL_RISK".to_string()),
                comment: Some("已核对冻结提交".to_string()),
            },
            idempotency_key: "request-1".to_string(),
        }
    }

    fn cancel_command() -> CancelCardSalesApprovalCommand {
        CancelCardSalesApprovalCommand {
            approval_instance_id: "instance-1".to_string(),
            current_step_instance_id: "step-1".to_string(),
            work_item_id: Some("task-1".to_string()),
            expected_instance_version: "2".to_string(),
            expected_step_version: "3".to_string(),
            expected_task_version: Some("4".to_string()),
            expected_subject_version: "submission-1".to_string(),
            reason: "申请人撤回并继续修改".to_string(),
            idempotency_key: "cancel-request-1".to_string(),
        }
    }

    #[test]
    fn strong_command_injects_actor_and_maps_runtime_decision() {
        let (runtime, guard) = command(CardSalesReviewDecision::Approve)
            .into_runtime_command("leader-1")
            .unwrap();

        assert_eq!(runtime.actor_id, "leader-1");
        assert_eq!(runtime.decision, ApprovalDecision::Approve);
        assert_eq!(runtime.expected_task_version, 4);
        assert_eq!(guard.expected_sales_order_lock_version, 5);
        assert_eq!(guard.work_item_type, WorkItemType::CardSalesManagerApproval);
    }

    #[test]
    fn reject_requires_reason_and_keeps_structured_code() {
        let (runtime, _) = command(CardSalesReviewDecision::Reject)
            .into_runtime_command("leader-1")
            .unwrap();
        assert_eq!(runtime.reason.as_deref(), Some("COMMERCIAL_RISK: 已核对冻结提交"));

        let mut missing = command(CardSalesReviewDecision::Reject);
        missing.decision.reason_code = None;
        assert!(matches!(
            missing.into_runtime_command("leader-1"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn terminate_requires_reason_and_maps_to_distinct_runtime_decision() {
        let (runtime, guard) = command(CardSalesReviewDecision::Terminate)
            .into_runtime_command("leader-1")
            .unwrap();

        assert_eq!(runtime.decision, ApprovalDecision::TerminateApproval);
        assert_eq!(guard.review_decision, CardSalesReviewDecision::Terminate);
        assert_eq!(runtime.reason.as_deref(), Some("COMMERCIAL_RISK: 已核对冻结提交"));

        let mut missing = command(CardSalesReviewDecision::Terminate);
        missing.decision.reason_code = None;
        assert!(matches!(
            missing.into_runtime_command("leader-1"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn work_item_and_review_stage_are_a_fixed_pair() {
        let mut invalid = command(CardSalesReviewDecision::Approve);
        invalid.decision.expected_review_status = CardSalesExpectedReviewStatus::PendingOperations;

        assert!(matches!(
            invalid.into_runtime_command("leader-1"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn outer_subject_must_equal_nested_submission() {
        let mut invalid = command(CardSalesReviewDecision::Approve);
        invalid.decision.sales_order_submission_id = "submission-2".to_string();

        assert!(matches!(
            invalid.into_runtime_command("leader-1"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn cancel_command_injects_actor_and_preserves_versions_and_idempotency() {
        let (runtime, guard) = cancel_command().into_runtime_command("sales-1").unwrap();

        assert_eq!(runtime.actor_id, "sales-1");
        assert_eq!(runtime.expected_instance_version, 2);
        assert_eq!(runtime.expected_step_version, 3);
        assert_eq!(runtime.expected_task_version, Some(4));
        assert_eq!(runtime.expected_subject_version, "submission-1");
        assert_eq!(runtime.idempotency_key, "cancel-request-1");
        assert_eq!(guard.current_step_instance_id, "step-1");
        assert_eq!(guard.work_item_id.as_deref(), Some("task-1"));
    }

    #[test]
    fn cancel_retry_reuses_the_identical_runtime_command() {
        let (first, first_guard) = cancel_command().into_runtime_command("sales-1").unwrap();
        let (retry, retry_guard) = cancel_command().into_runtime_command("sales-1").unwrap();

        assert_eq!(retry, first);
        assert_eq!(retry_guard, first_guard);
        assert_eq!(retry.idempotency_key, "cancel-request-1");
    }

    #[test]
    fn cancel_command_requires_paired_task_identity_and_positive_versions() {
        let mut missing_task_version = cancel_command();
        missing_task_version.expected_task_version = None;
        assert!(matches!(
            missing_task_version.into_runtime_command("sales-1"),
            Err(Error::ValidationError(_))
        ));

        let mut zero_instance_version = cancel_command();
        zero_instance_version.expected_instance_version = "0".to_string();
        assert!(matches!(
            zero_instance_version.into_runtime_command("sales-1"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn cancel_http_contract_rejects_unknown_fields() {
        let value = json!({
            "approval_instance_id": "instance-1",
            "current_step_instance_id": "step-1",
            "work_item_id": null,
            "expected_instance_version": "2",
            "expected_step_version": "3",
            "expected_task_version": null,
            "expected_subject_version": "submission-1",
            "reason": "申请人撤回并继续修改",
            "idempotency_key": "cancel-request-1",
            "decision": "APPROVE"
        });

        assert!(serde_json::from_value::<CancelCardSalesApprovalCommand>(value).is_err());
    }
}
