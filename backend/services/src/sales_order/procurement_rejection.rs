//! W05 采购驳回后的三路强类型处置。新写入已失败关闭。
#![allow(dead_code)]

use database::{
    AccessControlExt, DocumentRegistryExt, FileAssetExt, NoTransaction, SalesOrderExt, SalesReviewExt,
    Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::document_registry::{WorkflowAction, WorkflowActionData, WorkflowActionType};
use entities::ids::{
    BusinessDocumentId, LowMarginManagerConfirmationId, ProcurementConfirmationId, SalesOrderId,
    SalesOrderSubmissionId, SalesOrderWorkingCopyId, WorkItemId, WorkflowActionId,
};
use entities::sales_order::{
    CommercialStatus, ReviewStatus, SalesOrderSubmission, SalesOrderSubmissionLine, SalesOrderWorkingCopy,
    SalesOrderWorkingCopyLine, SubmissionStatus, WorkingPurpose,
};
use entities::sales_review::{
    LowMarginManagerConfirmation, LowMarginManagerConfirmationData, ProcurementConfirmation,
    ProcurementConfirmationData, ProcurementConfirmationStatus,
};
use entities::work_item::{
    AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use id_generator::next_id;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::mapper::{build_submission, build_submission_lines};
use super::SalesOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

const RESOLUTION_ACTION: &str = "sales_order.resolve_procurement_rejection";
const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
const OPERATION_ID_MAX_LEN: usize = 128;
const COMMENT_MAX_LEN: usize = 512;

/// 采购驳回后的固定三路处置命令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ResolveProcurementRejectionCommand {
    /// 修改商品、服务或销售价格后冻结新提交并重进采购确认。
    ResubmitChangedTerms {
        /// 销售单。
        sales_order_id: String,
        /// 被驳回采购确认。
        rejected_procurement_confirmation_id: String,
        /// 被驳回提交。
        rejected_submission_id: String,
        /// 销售单期望锁版本。
        expected_sales_order_lock_version: u64,
        /// 当前工作副本期望锁版本。
        expected_draft_version: u64,
        /// 客户重新确认的已登记证据。
        customer_reconfirmation_evidence_ids: Vec<String>,
        /// 客户侧可追踪操作号。
        operation_id: String,
        /// 幂等键。
        idempotency_key: String,
    },
    /// 商业条件不变，申请销售上级承接低毛利。
    RequestLowMarginAcceptance {
        /// 销售单。
        sales_order_id: String,
        /// 被驳回采购确认。
        rejected_procurement_confirmation_id: String,
        /// 被驳回提交。
        rejected_submission_id: String,
        /// 销售单期望锁版本。
        expected_sales_order_lock_version: u64,
        /// 当前工作副本期望锁版本。
        expected_draft_version: u64,
        /// 承接理由。
        low_margin_acceptance_reason: String,
        /// 已登记证据引用。
        evidence_reference_ids: Vec<String>,
        /// 客户侧可追踪操作号。
        operation_id: String,
        /// 幂等键。
        idempotency_key: String,
    },
    /// 确认不做并作废生效前销售单。
    VoidAfterRejection {
        /// 销售单。
        sales_order_id: String,
        /// 被驳回采购确认。
        rejected_procurement_confirmation_id: String,
        /// 被驳回提交。
        rejected_submission_id: String,
        /// 销售单期望锁版本。
        expected_sales_order_lock_version: u64,
        /// 结构化作废原因代码。
        void_reason_code: String,
        /// 作废说明。
        comment: String,
        /// 客户侧可追踪操作号。
        operation_id: String,
        /// 幂等键。
        idempotency_key: String,
    },
}

impl ResolveProcurementRejectionCommand {
    fn sales_order_id(&self) -> &str {
        match self {
            Self::ResubmitChangedTerms { sales_order_id, .. }
            | Self::RequestLowMarginAcceptance { sales_order_id, .. }
            | Self::VoidAfterRejection { sales_order_id, .. } => sales_order_id,
        }
    }

    fn rejected_confirmation_id(&self) -> &str {
        match self {
            Self::ResubmitChangedTerms {
                rejected_procurement_confirmation_id,
                ..
            }
            | Self::RequestLowMarginAcceptance {
                rejected_procurement_confirmation_id,
                ..
            }
            | Self::VoidAfterRejection {
                rejected_procurement_confirmation_id,
                ..
            } => rejected_procurement_confirmation_id,
        }
    }

    fn rejected_submission_id(&self) -> &str {
        match self {
            Self::ResubmitChangedTerms {
                rejected_submission_id,
                ..
            }
            | Self::RequestLowMarginAcceptance {
                rejected_submission_id,
                ..
            }
            | Self::VoidAfterRejection {
                rejected_submission_id,
                ..
            } => rejected_submission_id,
        }
    }

    fn expected_order_version(&self) -> u64 {
        match self {
            Self::ResubmitChangedTerms {
                expected_sales_order_lock_version,
                ..
            }
            | Self::RequestLowMarginAcceptance {
                expected_sales_order_lock_version,
                ..
            }
            | Self::VoidAfterRejection {
                expected_sales_order_lock_version,
                ..
            } => *expected_sales_order_lock_version,
        }
    }

    fn operation_id(&self) -> &str {
        match self {
            Self::ResubmitChangedTerms { operation_id, .. }
            | Self::RequestLowMarginAcceptance { operation_id, .. }
            | Self::VoidAfterRejection { operation_id, .. } => operation_id,
        }
    }

    fn idempotency_key(&self) -> &str {
        match self {
            Self::ResubmitChangedTerms { idempotency_key, .. }
            | Self::RequestLowMarginAcceptance { idempotency_key, .. }
            | Self::VoidAfterRejection { idempotency_key, .. } => idempotency_key,
        }
    }

    fn validate(&self) -> Result<()> {
        validate_non_blank(self.sales_order_id(), "销售单不能为空", 128)?;
        validate_non_blank(self.rejected_confirmation_id(), "被驳回采购确认不能为空", 128)?;
        validate_non_blank(self.rejected_submission_id(), "被驳回提交不能为空", 128)?;
        validate_non_blank(self.operation_id(), "操作号不能为空", OPERATION_ID_MAX_LEN)?;
        validate_non_blank(self.idempotency_key(), "幂等键不能为空", IDEMPOTENCY_KEY_MAX_LEN)?;
        match self {
            Self::ResubmitChangedTerms {
                expected_draft_version,
                customer_reconfirmation_evidence_ids,
                ..
            } => {
                if *expected_draft_version == 0 {
                    return Err(Error::ValidationError("草稿版本必须大于 0".to_string()));
                }
                if customer_reconfirmation_evidence_ids.is_empty() {
                    return Err(Error::ValidationError(
                        "改品或改价后重提必须提供客户重新确认依据".to_string(),
                    ));
                }
                validate_references(customer_reconfirmation_evidence_ids)
            }
            Self::RequestLowMarginAcceptance {
                expected_draft_version,
                low_margin_acceptance_reason,
                evidence_reference_ids,
                ..
            } => {
                if *expected_draft_version == 0 {
                    return Err(Error::ValidationError("草稿版本必须大于 0".to_string()));
                }
                validate_non_blank(
                    low_margin_acceptance_reason,
                    "低毛利承接理由不能为空",
                    COMMENT_MAX_LEN,
                )?;
                validate_references(evidence_reference_ids)
            }
            Self::VoidAfterRejection {
                void_reason_code,
                comment,
                ..
            } => {
                validate_non_blank(void_reason_code, "作废原因代码不能为空", 64)?;
                validate_non_blank(comment, "作废说明不能为空", COMMENT_MAX_LEN)
            }
        }
    }
}

/// 三路处置形成的正式业务结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementRejectionBusinessResult {
    /// 新提交已经进入新的采购确认。
    ChangedTermsResubmitted {
        /// 销售单。
        sales_order_id: String,
        /// 新不可变提交。
        new_submission_id: String,
        /// 新提交序号。
        new_submission_no: u32,
        /// 追加工作流动作。
        workflow_action_id: String,
        /// 新采购确认。
        new_procurement_confirmation_id: String,
        /// 新采购待办。
        new_procurement_work_item_id: String,
    },
    /// 新提交已进入低毛利上级确认，尚未创建采购确认。
    LowMarginManagerConfirmationCreated {
        /// 销售单。
        sales_order_id: String,
        /// 新不可变提交。
        new_submission_id: String,
        /// 新提交序号。
        new_submission_no: u32,
        /// 追加工作流动作。
        workflow_action_id: String,
        /// 低毛利确认事实。
        low_margin_confirmation_id: String,
        /// 低毛利待办。
        low_margin_manager_work_item_id: String,
    },
    /// 销售单已经作废。
    VoidedAfterProcurementRejection {
        /// 销售单。
        sales_order_id: String,
        /// 追加工作流动作。
        workflow_action_id: String,
    },
}

/// 已提交的采购驳回处置响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolveProcurementRejectionResult {
    /// 原操作号。
    pub operation_id: String,
    /// 固定为 `COMMITTED`。
    pub status: String,
    /// 提交时间。
    pub committed_at: u64,
    /// 正式业务结果。
    #[serde(flatten)]
    pub business_result: ProcurementRejectionBusinessResult,
}

impl SalesOrderService {
    /// 以固定三分支强类型命令处置当前采购驳回。
    ///
    /// 每个分支均在一个事务内锁定销售单、旧确认、旧提交、草稿和后继任务事实；
    /// 同一幂等键的精确重试在版本校验前返回首次正式结果。
    ///
    /// # Errors
    /// 身份、版本、状态、证据或分支不变式不成立时返回稳定业务错误。
    pub async fn resolve_procurement_rejection(
        &self,
        path_sales_order_id: &str,
        command: ResolveProcurementRejectionCommand,
        actor: &AuditActor,
    ) -> Result<ResolveProcurementRejectionResult> {
        let _ = (self, path_sales_order_id, command, actor);
        return Err(Error::ConflictError(
            "采购驳回处置已停止新写入，必须走销售单统一审批撤回或重新提交".to_string(),
        ));
        #[allow(unreachable_code)]
        if path_sales_order_id != command.sales_order_id() {
            return Err(Error::ConflictError("路径销售单与命令载荷不一致".to_string()));
        }
        let fingerprint = resolution_fingerprint(actor.id(), &command)?;
        let audit_id = resolution_audit_id(actor.id(), command.idempotency_key());
        if let Some(receipt) = self
            .replay_procurement_rejection_resolution(&audit_id, &fingerprint, &command)
            .await?
        {
            return Ok(receipt);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let command_for_tx = command.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let service = SalesOrderService::new(db.clone());
                    let context = service
                        .load_procurement_rejection_context(&command_for_tx, &actor_id, session)
                        .await?;
                    service
                        .ensure_resolution_evidence(&command_for_tx, session)
                        .await?;
                    let result = match &command_for_tx {
                        ResolveProcurementRejectionCommand::ResubmitChangedTerms { .. } => {
                            service
                                .resubmit_changed_terms(context, &command_for_tx, &actor_owned, session)
                                .await?
                        }
                        ResolveProcurementRejectionCommand::RequestLowMarginAcceptance {
                            low_margin_acceptance_reason,
                            evidence_reference_ids,
                            ..
                        } => {
                            service
                                .request_low_margin_acceptance(
                                    context,
                                    low_margin_acceptance_reason,
                                    evidence_reference_ids,
                                    command_for_tx.operation_id(),
                                    &actor_owned,
                                    session,
                                )
                                .await?
                        }
                        ResolveProcurementRejectionCommand::VoidAfterRejection {
                            void_reason_code,
                            comment,
                            ..
                        } => {
                            service
                                .void_after_procurement_rejection(
                                    context,
                                    void_reason_code,
                                    comment,
                                    command_for_tx.operation_id(),
                                    &actor_owned,
                                    session,
                                )
                                .await?
                        }
                    };
                    let audit = actor_owned.clone().resource_log_with_id(
                        audit_id_for_tx,
                        RESOLUTION_ACTION,
                        "sales_order",
                        command_for_tx.sales_order_id().to_string(),
                        Some(resolution_receipt_message(&fingerprint_for_tx, &result)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ResolveProcurementRejectionResult, crate::errors::Error>(result)
                })
            })
            .await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                if let Some(receipt) = self
                    .replay_procurement_rejection_resolution(&audit_id, &fingerprint, &command)
                    .await?
                {
                    return Ok(receipt);
                }
                Err(error)
            }
        }
    }

    async fn load_procurement_rejection_context(
        &self,
        command: &ResolveProcurementRejectionCommand,
        actor_id: &str,
        executor: &mut dyn database::Executor,
    ) -> Result<ProcurementRejectionContext> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(command.sales_order_id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != command.expected_order_version() {
            return Err(Error::ConflictError("销售单版本已变化，请刷新后重试".to_string()));
        }
        if order.commercial_status != CommercialStatus::Draft
            || order.review_status != ReviewStatus::NotSubmitted
        {
            return Err(Error::ConflictError(
                "销售单已不在采购驳回后的待处理状态".to_string(),
            ));
        }
        let rejected_confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(command.rejected_confirmation_id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("被驳回采购确认不存在".to_string()))?;
        if rejected_confirmation.sales_order_id.as_ref() != command.sales_order_id()
            || rejected_confirmation.submission_id.as_ref() != command.rejected_submission_id()
            || rejected_confirmation.stable.status != ProcurementConfirmationStatus::Rejected
        {
            return Err(Error::ConflictError("采购驳回链与当前销售单不一致".to_string()));
        }
        let rejected_submission = self
            .db
            .sales_order_submissions()
            .find_by_id(command.rejected_submission_id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("被驳回销售提交不存在".to_string()))?;
        if rejected_submission.sales_order_id.as_ref() != command.sales_order_id()
            || rejected_submission.stable.status != SubmissionStatus::Rejected
        {
            return Err(Error::ConflictError("被驳回提交已不属于当前处理链".to_string()));
        }
        self.ensure_rejected_work_item_completed(&rejected_confirmation, executor)
            .await?;
        self.ensure_no_active_successor(&order, executor).await?;

        let working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(
                &SalesOrderId::new(order.base.id.clone()),
                WorkingPurpose::FirstSubmission,
                executor,
            )
            .await?;
        if matches!(
            command,
            ResolveProcurementRejectionCommand::VoidAfterRejection { .. }
        ) {
            let is_owner = working_copy
                .as_ref()
                .map_or(order.stable.created_by == actor_id, |copy| {
                    copy.editor_user_id == actor_id
                });
            if !is_owner {
                return Err(Error::Forbidden(
                    "只有当前销售责任人可以作废采购驳回销售单".to_string(),
                ));
            }
        } else {
            let copy = working_copy
                .as_ref()
                .ok_or_else(|| Error::NotFound("当前销售草稿不存在".to_string()))?;
            let expected = match command {
                ResolveProcurementRejectionCommand::ResubmitChangedTerms {
                    expected_draft_version,
                    ..
                }
                | ResolveProcurementRejectionCommand::RequestLowMarginAcceptance {
                    expected_draft_version,
                    ..
                } => *expected_draft_version,
                ResolveProcurementRejectionCommand::VoidAfterRejection { .. } => unreachable!(),
            };
            if copy.base.version != expected {
                return Err(Error::ConflictError(
                    "销售草稿版本已变化，请刷新后重试".to_string(),
                ));
            }
            if copy.editor_user_id != actor_id {
                return Err(Error::Forbidden(
                    "只有当前销售草稿责任人可以处理采购驳回".to_string(),
                ));
            }
        }
        let rejected_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                &[SalesOrderSubmissionId::new(rejected_submission.base.id.clone())],
                executor,
            )
            .await?;
        let working_lines = match &working_copy {
            Some(copy) => {
                self.db
                    .sales_order_working_copy_lines()
                    .list_lines_by_working_copy(&SalesOrderWorkingCopyId::new(copy.base.id.clone()), executor)
                    .await?
            }
            None => Vec::new(),
        };
        Ok(ProcurementRejectionContext {
            order,
            rejected_confirmation,
            rejected_submission,
            rejected_lines,
            working_copy,
            working_lines,
        })
    }

    async fn ensure_rejected_work_item_completed(
        &self,
        confirmation: &ProcurementConfirmation,
        executor: &mut dyn database::Executor,
    ) -> Result<()> {
        let items = self
            .db
            .work_items()
            .find_many(
                doc! {
                    "business_object_type": "procurement_confirmation",
                    "business_object_id": &confirmation.base.id,
                    "work_item_type": WorkItemType::ProcurementConfirmation.as_str(),
                },
                executor,
            )
            .await?;
        if items.iter().any(|item| {
            item.subject_version == confirmation.submission_id.as_ref()
                && item.status == entities::work_item::WorkItemStatus::Completed
        }) {
            return Ok(());
        }
        Err(Error::ConflictError("原采购确认待办尚未形成完成事实".to_string()))
    }

    async fn ensure_no_active_successor(
        &self,
        order: &entities::sales_order::SalesOrder,
        executor: &mut dyn database::Executor,
    ) -> Result<()> {
        if self
            .db
            .procurement_confirmations()
            .find_pending_by_sales_order(&SalesOrderId::new(order.base.id.clone()), executor)
            .await?
            .is_some()
        {
            return Err(Error::ConflictError("销售单已有新的采购确认待处理".to_string()));
        }
        let active = self
            .db
            .work_items()
            .list_active_by_object("sales_order", &order.base.id, executor)
            .await?;
        if active
            .iter()
            .any(|item| item.work_item_type == WorkItemType::LowMarginManagerConfirmation)
        {
            return Err(Error::ConflictError("销售单已有低毛利上级确认待处理".to_string()));
        }
        Ok(())
    }

    async fn ensure_resolution_evidence(
        &self,
        command: &ResolveProcurementRejectionCommand,
        executor: &mut dyn database::Executor,
    ) -> Result<()> {
        let ids = match command {
            ResolveProcurementRejectionCommand::ResubmitChangedTerms {
                customer_reconfirmation_evidence_ids,
                ..
            } => customer_reconfirmation_evidence_ids,
            ResolveProcurementRejectionCommand::RequestLowMarginAcceptance {
                evidence_reference_ids,
                ..
            } => evidence_reference_ids,
            ResolveProcurementRejectionCommand::VoidAfterRejection { .. } => return Ok(()),
        };
        for id in ids {
            if self.db.file_assets().find_by_id(id, executor).await?.is_none() {
                return Err(Error::ValidationError(format!("证据引用不存在：{id}")));
            }
        }
        Ok(())
    }

    async fn resubmit_changed_terms(
        &self,
        mut context: ProcurementRejectionContext,
        command: &ResolveProcurementRejectionCommand,
        actor: &AuditActor,
        executor: &mut dyn database::Executor,
    ) -> Result<ResolveProcurementRejectionResult> {
        let mut copy = context
            .working_copy
            .take()
            .ok_or_else(|| Error::NotFound("当前销售草稿不存在".to_string()))?;
        if !changed_item_or_price(&context.working_lines, &context.rejected_lines) {
            return Err(Error::BusinessLogicError(
                "当前草稿未修改商品、服务或销售价格，不能走改品改价重提".to_string(),
            ));
        }
        self.ensure_sellable_refs(
            &Self::sellable_working_copy_refs(&context.working_lines)?,
            executor,
        )
        .await?;
        let submission = build_rejection_submission(&copy, &context.working_lines, &context, actor)?;
        let submission_lines = build_submission_lines(&submission, &context.working_lines)?;
        context.order.submit_for_review(actor.id())?;
        copy.submit()?;
        let confirmation = new_procurement_confirmation(&context.order, &submission, actor.id())?;
        let work_item = new_procurement_work_item(&confirmation, &submission)?;
        let workflow = new_resolution_workflow(
            &context.order,
            WorkflowActionType::Submit,
            "DRAFT",
            "PENDING_REVIEW",
            actor,
            Some("采购驳回后改品或改价重提".to_string()),
        )?;
        self.db
            .sales_order()
            .submit_working_copy(&mut copy, &submission, &submission_lines, executor)
            .await?;
        self.db
            .sales_orders()
            .update(&mut context.order, executor)
            .await?;
        self.db
            .sales_review()
            .create_procurement_confirmation_with_lines(&confirmation, &[], executor)
            .await?;
        self.db.work_items().create(&work_item, executor).await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(committed_result(
            command.operation_id(),
            ProcurementRejectionBusinessResult::ChangedTermsResubmitted {
                sales_order_id: context.order.base.id,
                new_submission_id: submission.base.id,
                new_submission_no: submission.submission_no,
                workflow_action_id: workflow.base.id,
                new_procurement_confirmation_id: confirmation.base.id,
                new_procurement_work_item_id: work_item.base.id,
            },
        ))
    }

    async fn request_low_margin_acceptance(
        &self,
        mut context: ProcurementRejectionContext,
        acceptance_reason: &str,
        evidence_reference_ids: &[String],
        operation_id: &str,
        actor: &AuditActor,
        executor: &mut dyn database::Executor,
    ) -> Result<ResolveProcurementRejectionResult> {
        let mut copy = context
            .working_copy
            .take()
            .ok_or_else(|| Error::NotFound("当前销售草稿不存在".to_string()))?;
        if !commercial_terms_unchanged(
            &copy,
            &context.working_lines,
            &context.rejected_submission,
            &context.rejected_lines,
        ) {
            return Err(Error::BusinessLogicError(
                "当前商业条件已变化，请改用改品或改价后重提".to_string(),
            ));
        }
        self.ensure_sellable_refs(
            &Self::sellable_working_copy_refs(&context.working_lines)?,
            executor,
        )
        .await?;
        let submission = build_rejection_submission(&copy, &context.working_lines, &context, actor)?;
        let submission_lines = build_submission_lines(&submission, &context.working_lines)?;
        context.order.submit_for_review(actor.id())?;
        context
            .order
            .transition_review(ReviewStatus::PendingLowMarginSuperior, actor.id())?;
        copy.submit()?;
        let now = Instant::now();
        let confirmation = LowMarginManagerConfirmation::new(
            LowMarginManagerConfirmationId::new(next_id()),
            LowMarginManagerConfirmationData {
                sales_order_id: SalesOrderId::new(context.order.base.id.clone()),
                rejected_procurement_confirmation_id: ProcurementConfirmationId::new(
                    context.rejected_confirmation.base.id.clone(),
                ),
                rejected_submission_id: SalesOrderSubmissionId::new(
                    context.rejected_submission.base.id.clone(),
                ),
                low_margin_submission_id: SalesOrderSubmissionId::new(submission.base.id.clone()),
                acceptance_reason: acceptance_reason.to_string(),
                evidence_reference_ids: evidence_reference_ids.to_vec(),
                requested_by: actor.id().to_string(),
                requested_at: now,
            },
        )?;
        let work_item = WorkItem::new_at(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::LowMarginManagerConfirmation,
                approval_step_instance_id: None,
                business_object_type: "sales_order".to_string(),
                business_object_id: context.order.base.id.clone(),
                subject_version: submission.base.id.clone(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-sales-leader".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("procurement_rejection_low_margin_requested".to_string()),
                impact_summary: Some(format!("销售单 {} 低毛利承接确认", context.order.order_no)),
            },
            now,
        )?;
        let workflow = new_resolution_workflow(
            &context.order,
            WorkflowActionType::Submit,
            "DRAFT",
            "PENDING_REVIEW",
            actor,
            Some("采购驳回后申请低毛利承接".to_string()),
        )?;
        self.db
            .sales_order()
            .submit_working_copy(&mut copy, &submission, &submission_lines, executor)
            .await?;
        self.db
            .sales_orders()
            .update(&mut context.order, executor)
            .await?;
        self.db
            .low_margin_manager_confirmations()
            .create(&confirmation, executor)
            .await?;
        self.db.work_items().create(&work_item, executor).await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(committed_result(
            operation_id,
            ProcurementRejectionBusinessResult::LowMarginManagerConfirmationCreated {
                sales_order_id: context.order.base.id,
                new_submission_id: submission.base.id,
                new_submission_no: submission.submission_no,
                workflow_action_id: workflow.base.id,
                low_margin_confirmation_id: confirmation.base.id,
                low_margin_manager_work_item_id: work_item.base.id,
            },
        ))
    }

    async fn void_after_procurement_rejection(
        &self,
        mut context: ProcurementRejectionContext,
        reason_code: &str,
        comment: &str,
        operation_id: &str,
        actor: &AuditActor,
        executor: &mut dyn database::Executor,
    ) -> Result<ResolveProcurementRejectionResult> {
        context.order.void(actor.id())?;
        if let Some(copy) = &mut context.working_copy {
            copy.abandon()?;
            self.db
                .sales_order_working_copies()
                .update(copy, executor)
                .await?;
        }
        let workflow = new_resolution_workflow(
            &context.order,
            WorkflowActionType::Void,
            "DRAFT",
            "VOIDED",
            actor,
            Some(format!("{}：{}", reason_code.trim(), comment.trim())),
        )?;
        self.db
            .sales_orders()
            .update(&mut context.order, executor)
            .await?;
        self.db.workflow_actions().create(&workflow, executor).await?;
        Ok(committed_result(
            operation_id,
            ProcurementRejectionBusinessResult::VoidedAfterProcurementRejection {
                sales_order_id: context.order.base.id,
                workflow_action_id: workflow.base.id,
            },
        ))
    }

    async fn replay_procurement_rejection_resolution(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        command: &ResolveProcurementRejectionCommand,
    ) -> Result<Option<ResolveProcurementRejectionResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != RESOLUTION_ACTION
            || audit.resource_type != "sales_order"
            || audit.resource_id.as_deref() != Some(command.sales_order_id())
            || !audit.success
        {
            return Err(Error::Internal("采购驳回处置幂等收据身份非法".to_string()));
        }
        let business_result = parse_resolution_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("采购驳回处置幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        Ok(Some(ResolveProcurementRejectionResult {
            operation_id: command.operation_id().to_string(),
            status: "COMMITTED".to_string(),
            committed_at: audit.base.created_at,
            business_result,
        }))
    }
}

struct ProcurementRejectionContext {
    order: entities::sales_order::SalesOrder,
    rejected_confirmation: ProcurementConfirmation,
    rejected_submission: SalesOrderSubmission,
    rejected_lines: Vec<SalesOrderSubmissionLine>,
    working_copy: Option<SalesOrderWorkingCopy>,
    working_lines: Vec<SalesOrderWorkingCopyLine>,
}

fn build_rejection_submission(
    copy: &SalesOrderWorkingCopy,
    lines: &[SalesOrderWorkingCopyLine],
    context: &ProcurementRejectionContext,
    actor: &AuditActor,
) -> Result<SalesOrderSubmission> {
    build_submission(
        copy,
        lines,
        context.rejected_submission.submission_no + 1,
        actor,
        None,
        None,
    )
}

fn new_procurement_confirmation(
    order: &entities::sales_order::SalesOrder,
    submission: &SalesOrderSubmission,
    actor_id: &str,
) -> Result<ProcurementConfirmation> {
    ProcurementConfirmation::new(
        ProcurementConfirmationId::new(next_id()),
        ProcurementConfirmationData {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            submission_id: SalesOrderSubmissionId::new(submission.base.id.clone()),
            reject_reason_code: None,
            comment: None,
        },
        actor_id,
    )
    .map_err(Error::Logic)
}

fn new_procurement_work_item(
    confirmation: &ProcurementConfirmation,
    submission: &SalesOrderSubmission,
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
            reason_code: Some("procurement_confirmation_resubmitted".to_string()),
            impact_summary: Some(format!("采购二次确认：销售提交 {}", submission.submission_no)),
        },
    )
    .map_err(Error::Logic)
}

fn new_resolution_workflow(
    order: &entities::sales_order::SalesOrder,
    action_type: WorkflowActionType,
    from_status: &str,
    to_status: &str,
    actor: &AuditActor,
    comment: Option<String>,
) -> Result<WorkflowAction> {
    WorkflowAction::new(
        WorkflowActionId::new(next_id()),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(order.base.id.clone()),
            action_type,
            from_status: from_status.to_string(),
            to_status: to_status.to_string(),
            actor_id: actor.id().to_string(),
            actor_role: "role-sales".to_string(),
            comment,
        },
    )
    .map_err(Error::Logic)
}

fn committed_result(
    operation_id: &str,
    business_result: ProcurementRejectionBusinessResult,
) -> ResolveProcurementRejectionResult {
    ResolveProcurementRejectionResult {
        operation_id: operation_id.to_string(),
        status: "COMMITTED".to_string(),
        committed_at: Instant::now().unix_secs() as u64,
        business_result,
    }
}

fn commercial_terms_unchanged(
    copy: &SalesOrderWorkingCopy,
    working_lines: &[SalesOrderWorkingCopyLine],
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
) -> bool {
    copy.business_type == submission.business_type
        && copy.customer_id == submission.customer_id
        && copy.contract_revision_id == submission.contract_revision_id
        && copy.settlement_party_id == submission.settlement_party_id
        && copy.customer_snapshot == submission.customer_snapshot
        && copy.contract_snapshot == submission.contract_snapshot
        && copy.settlement_party_snapshot == submission.settlement_party_snapshot
        && copy.payment_term_snapshot == submission.payment_term_snapshot
        && copy.invoice_requirement_snapshot == submission.invoice_requirement_snapshot
        && copy.project_name == submission.project_name
        && copy.business_remark == submission.business_remark
        && copy.gross_amount == submission.gross_amount
        && copy.net_amount == submission.net_amount
        && copy.tax_amount == submission.tax_amount
        && lines_equal(working_lines, submission_lines)
}

fn lines_equal(working: &[SalesOrderWorkingCopyLine], submitted: &[SalesOrderSubmissionLine]) -> bool {
    if working.len() != submitted.len() {
        return false;
    }
    working.iter().all(|left| {
        submitted
            .iter()
            .find(|right| right.line_no == left.line_no)
            .is_some_and(|right| line_equal(left, right))
    })
}

fn line_equal(left: &SalesOrderWorkingCopyLine, right: &SalesOrderSubmissionLine) -> bool {
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

fn changed_item_or_price(
    working: &[SalesOrderWorkingCopyLine],
    submitted: &[SalesOrderSubmissionLine],
) -> bool {
    if working.len() != submitted.len() {
        return true;
    }
    working.iter().any(|left| {
        let Some(right) = submitted.iter().find(|right| right.line_no == left.line_no) else {
            return true;
        };
        left.sales_order_line_id != right.sales_order_line_id
            || left.line_type != right.line_type
            || left.item_name_snapshot != right.item_name_snapshot
            || left.spec_snapshot != right.spec_snapshot
            || left.sku_id != right.sku_id
            || left.sku_revision_id != right.sku_revision_id
            || left.quantity != right.quantity
            || left.unit_price_gross != right.unit_price_gross
            || left.face_value != right.face_value
            || left.card_count != right.card_count
            || left.transaction_amount != right.transaction_amount
    })
}

fn resolution_fingerprint(actor_id: &str, command: &ResolveProcurementRejectionCommand) -> Result<String> {
    let payload = serde_json::to_vec(&(RESOLUTION_ACTION, actor_id, command))
        .map_err(|error| Error::Internal(format!("采购驳回处置命令序列化失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn resolution_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    format!(
        "w05-pr-{:x}",
        Sha256::digest(format!("{RESOLUTION_ACTION}|{actor_id}|{}", idempotency_key.trim()).as_bytes())
    )
}

fn resolution_receipt_message(fingerprint: &str, result: &ResolveProcurementRejectionResult) -> String {
    let encoded = match &result.business_result {
        ProcurementRejectionBusinessResult::ChangedTermsResubmitted {
            sales_order_id,
            new_submission_id,
            new_submission_no,
            workflow_action_id,
            new_procurement_confirmation_id,
            new_procurement_work_item_id,
        } => format!(
            "R|{sales_order_id}|{new_submission_id}|{new_submission_no}|{workflow_action_id}|{new_procurement_confirmation_id}|{new_procurement_work_item_id}"
        ),
        ProcurementRejectionBusinessResult::LowMarginManagerConfirmationCreated {
            sales_order_id,
            new_submission_id,
            new_submission_no,
            workflow_action_id,
            low_margin_confirmation_id,
            low_margin_manager_work_item_id,
        } => format!(
            "L|{sales_order_id}|{new_submission_id}|{new_submission_no}|{workflow_action_id}|{low_margin_confirmation_id}|{low_margin_manager_work_item_id}"
        ),
        ProcurementRejectionBusinessResult::VoidedAfterProcurementRejection {
            sales_order_id,
            workflow_action_id,
        } => format!("V|{sales_order_id}|{workflow_action_id}"),
    };
    format!("fp={fingerprint};result={encoded}")
}

fn parse_resolution_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ProcurementRejectionBusinessResult> {
    let (fingerprint, encoded) = message
        .strip_prefix("fp=")
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("采购驳回处置幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(
            "同一幂等键已用于不同的采购驳回处置命令".to_string(),
        ));
    }
    let fields = encoded.split('|').collect::<Vec<_>>();
    match fields.as_slice() {
        ["R", order_id, submission_id, submission_no, workflow_id, confirmation_id, work_item_id] => {
            Ok(ProcurementRejectionBusinessResult::ChangedTermsResubmitted {
                sales_order_id: (*order_id).to_string(),
                new_submission_id: (*submission_id).to_string(),
                new_submission_no: parse_submission_no(submission_no)?,
                workflow_action_id: (*workflow_id).to_string(),
                new_procurement_confirmation_id: (*confirmation_id).to_string(),
                new_procurement_work_item_id: (*work_item_id).to_string(),
            })
        }
        ["L", order_id, submission_id, submission_no, workflow_id, confirmation_id, work_item_id] => Ok(
            ProcurementRejectionBusinessResult::LowMarginManagerConfirmationCreated {
                sales_order_id: (*order_id).to_string(),
                new_submission_id: (*submission_id).to_string(),
                new_submission_no: parse_submission_no(submission_no)?,
                workflow_action_id: (*workflow_id).to_string(),
                low_margin_confirmation_id: (*confirmation_id).to_string(),
                low_margin_manager_work_item_id: (*work_item_id).to_string(),
            },
        ),
        ["V", order_id, workflow_id] => Ok(
            ProcurementRejectionBusinessResult::VoidedAfterProcurementRejection {
                sales_order_id: (*order_id).to_string(),
                workflow_action_id: (*workflow_id).to_string(),
            },
        ),
        _ => Err(Error::Internal("采购驳回处置幂等收据结果非法".to_string())),
    }
}

fn parse_submission_no(value: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| Error::Internal("采购驳回处置收据提交序号非法".to_string()))
}

fn validate_non_blank(value: &str, message: &str, max_len: usize) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    if trimmed.chars().count() > max_len {
        return Err(Error::ValidationError(format!("{message}或长度超过上限")));
    }
    Ok(())
}

fn validate_references(values: &[String]) -> Result<()> {
    if values.len() > 20 {
        return Err(Error::ValidationError("证据引用数量超过上限".to_string()));
    }
    for (index, value) in values.iter().enumerate() {
        validate_non_blank(value, "证据引用不能为空", 128)?;
        if values[..index].contains(value) {
            return Err(Error::ValidationError("证据引用不得重复".to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(key: &str) -> ResolveProcurementRejectionCommand {
        ResolveProcurementRejectionCommand::VoidAfterRejection {
            sales_order_id: "so-1".to_string(),
            rejected_procurement_confirmation_id: "pc-1".to_string(),
            rejected_submission_id: "sub-1".to_string(),
            expected_sales_order_lock_version: 2,
            void_reason_code: "CUSTOMER_CANCELLED".to_string(),
            comment: "客户确认不再继续".to_string(),
            operation_id: "op-1".to_string(),
            idempotency_key: key.to_string(),
        }
    }

    #[test]
    fn idempotency_identity_is_stable_and_payload_bound() {
        let left = resolution_fingerprint("sales-1", &command("key-1")).unwrap();
        assert_eq!(
            left,
            resolution_fingerprint("sales-1", &command("key-1")).unwrap()
        );
        assert_ne!(
            left,
            resolution_fingerprint("sales-1", &command("key-2")).unwrap()
        );
        assert!(!resolution_audit_id("sales-1", "secret-key").contains("secret-key"));
    }

    #[test]
    fn receipt_round_trip_preserves_real_result_references() {
        let result = committed_result(
            "op-1",
            ProcurementRejectionBusinessResult::ChangedTermsResubmitted {
                sales_order_id: "so-1".to_string(),
                new_submission_id: "sub-2".to_string(),
                new_submission_no: 2,
                workflow_action_id: "wa-1".to_string(),
                new_procurement_confirmation_id: "pc-2".to_string(),
                new_procurement_work_item_id: "wi-2".to_string(),
            },
        );
        let message = resolution_receipt_message("abc", &result);
        assert_eq!(
            parse_resolution_receipt(&message, "abc").unwrap(),
            result.business_result
        );
        assert!(parse_resolution_receipt(&message, "different").is_err());
    }

    #[test]
    fn transport_contract_accepts_only_three_strong_branches() {
        for action in [
            "RESUBMIT_CHANGED_TERMS",
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
            "VOID_AFTER_REJECTION",
        ] {
            let branch = match action {
                "RESUBMIT_CHANGED_TERMS" => serde_json::json!({
                    "action": action,
                    "sales_order_id": "so-1",
                    "rejected_procurement_confirmation_id": "pc-1",
                    "rejected_submission_id": "sub-1",
                    "expected_sales_order_lock_version": 2,
                    "expected_draft_version": 3,
                    "customer_reconfirmation_evidence_ids": ["asset-1"],
                    "operation_id": "op-1",
                    "idempotency_key": "key-1"
                }),
                "REQUEST_LOW_MARGIN_ACCEPTANCE" => serde_json::json!({
                    "action": action,
                    "sales_order_id": "so-1",
                    "rejected_procurement_confirmation_id": "pc-1",
                    "rejected_submission_id": "sub-1",
                    "expected_sales_order_lock_version": 2,
                    "expected_draft_version": 3,
                    "low_margin_acceptance_reason": "公司承接",
                    "evidence_reference_ids": [],
                    "operation_id": "op-1",
                    "idempotency_key": "key-1"
                }),
                _ => serde_json::json!({
                    "action": action,
                    "sales_order_id": "so-1",
                    "rejected_procurement_confirmation_id": "pc-1",
                    "rejected_submission_id": "sub-1",
                    "expected_sales_order_lock_version": 2,
                    "void_reason_code": "CUSTOMER_CANCELLED",
                    "comment": "客户确认不再继续",
                    "operation_id": "op-1",
                    "idempotency_key": "key-1"
                }),
            };
            let command: ResolveProcurementRejectionCommand = serde_json::from_value(branch).unwrap();
            command.validate().unwrap();
        }

        let invalid = serde_json::json!({
            "action": "VOID_AFTER_REJECTION",
            "sales_order_id": "so-1",
            "rejected_procurement_confirmation_id": "pc-1",
            "rejected_submission_id": "sub-1",
            "expected_sales_order_lock_version": 2,
            "void_reason_code": "CUSTOMER_CANCELLED",
            "comment": "客户确认不再继续",
            "operation_id": "op-1",
            "idempotency_key": "key-1",
            "reference": "fake-reference"
        });
        assert!(serde_json::from_value::<ResolveProcurementRejectionCommand>(invalid).is_err());
    }
}
