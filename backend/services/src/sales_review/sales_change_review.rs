use database::{AccessControlExt, Executor, NoTransaction, SalesReviewExt, WorkItemExt};
use entities::ids::SalesChangeSubmissionId;
use entities::sales_review::{SalesChangeReview, SalesChangeReviewStage};
use entities::work_item::{WorkItem, WorkItemType};
use sha2::{Digest, Sha256};

use super::{ChangeReviewDecisionRequest, SalesChangeOrderDetailView, SalesReviewService};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

const CHANGE_REVIEW_RECEIPT_PREFIX: &str = "sales-change-review-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// 履约影响确认与财务复核不得充当审批流程节点。
///
/// # 错误
/// 恒返回冲突。
fn fail_closed_change_review_node() -> Result<SalesChangeOrderDetailView> {
    super::adapter::reject_legacy_change_review_node()?;
    Err(Error::ConflictError(
        "销售变更履约影响确认与财务复核不得充当审批流程节点".to_string(),
    ))
}

impl SalesReviewService {
    /// 通过变更履约影响确认（进入财务复核；卡券变更完成运营确认后同样走财务复核）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或待处理复核不存在
    /// * `ConflictError` - 状态机拒绝
    pub async fn confirm_impact(
        &self,
        _id: &str,
        _req: ChangeReviewDecisionRequest,
        _actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        fail_closed_change_review_node()
    }

    pub async fn reject_impact(
        &self,
        _id: &str,
        _req: ChangeReviewDecisionRequest,
        _actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        fail_closed_change_review_node()
    }

    /// 通过财务复核（§8.1.3 变更生效：新版本 + 应收差额 + 当前版本切换）。
    ///
    /// 校验基准版本仍为当前版本（防并发覆盖）后，在单事务内追加不可变销售版本、
    /// 更新销售单当前版本指针、追加应收差额分录（新金额减旧金额，零差额不写）、
    /// 完成财务复核待办并写审计；不修改已发生事实。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单/待处理复核/销售单不存在
    /// * `ConflictError` - 基准版本已不是当前版本
    pub async fn confirm_finance(
        &self,
        _id: &str,
        _req: ChangeReviewDecisionRequest,
        _actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        fail_closed_change_review_node()
    }

    pub async fn reject_finance(
        &self,
        _id: &str,
        _req: ChangeReviewDecisionRequest,
        _actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        fail_closed_change_review_node()
    }

    /// 变更复核驳回共用编排（影响确认/财务复核阶段）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求
    /// * `approved` - 恒为 `false`（保留签名复用决策入口）
    /// * `actor` - 操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// 查询失败或状态机拒绝时返回错误。
    async fn decide_change_review(
        &self,
        _id: &str,
        _req: ChangeReviewDecisionRequest,
        _approved: bool,
        _actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        let _ = self;
        fail_closed_change_review_node()
    }

    async fn find_pending_change_review(
        &self,
        submission_id: &SalesChangeSubmissionId,
    ) -> Result<SalesChangeReview> {
        self.db
            .sales_change_reviews()
            .find_one(
                mongodb::bson::doc! {
                    "sales_change_submission_id": submission_id.to_string(),
                    "status": entities::sales_review::SalesReviewStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("待处理变更复核不存在".to_string()))
    }

    /// 加载并校验销售变更强类型决策所锁定的待办。
    async fn load_change_review_work_item(
        &self,
        review: &SalesChangeReview,
        submission_id: &SalesChangeSubmissionId,
        req: &ChangeReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<WorkItem> {
        let item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更复核待办不存在".to_string()))?;
        validate_change_review_work_item(&item, review, submission_id, req, actor)?;
        Ok(item)
    }

    /// 重放已落库的销售变更复核命令，并拒绝幂等键载荷漂移。
    async fn replay_change_review_command(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        sales_change_order_id: &str,
    ) -> Result<Option<SalesChangeOrderDetailView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.resource_id.as_deref() != Some(sales_change_order_id) {
            return Err(Error::Internal(
                "销售变更复核幂等收据与业务对象不一致".to_string(),
            ));
        }
        let fingerprint = audit
            .message
            .as_deref()
            .and_then(change_review_receipt_fingerprint)
            .ok_or_else(|| Error::Internal("销售变更复核幂等收据格式非法".to_string()))?;
        if fingerprint != expected_fingerprint {
            return Err(Error::ConflictError(
                "幂等键已用于不同的销售变更复核命令".to_string(),
            ));
        }
        self.sales_change_order_detail(sales_change_order_id)
            .await
            .map(Some)
    }
}

/// 生成不暴露原始幂等键的销售变更复核审计主键。
fn change_review_audit_id(actor_id: &str, action: &str, work_item_id: &str, key: &str) -> String {
    format!(
        "{CHANGE_REVIEW_RECEIPT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{work_item_id}|{key}"))
    )
}

/// 对销售变更复核命令的全部业务锁与意见生成摘要。
fn change_review_fingerprint(id: &str, req: &ChangeReviewDecisionRequest) -> String {
    command_fingerprint(&[
        id,
        &req.work_item_id,
        &req.expected_task_version.to_string(),
        &req.expected_subject_version,
        req.decision_reason.as_deref().unwrap_or_default(),
    ])
}

/// 构造销售变更复核幂等收据消息。
fn change_review_receipt_message(fingerprint: &str) -> String {
    format!("{COMMAND_FINGERPRINT_PREFIX}{fingerprint}")
}

/// 从收据消息提取命令指纹。
fn change_review_receipt_fingerprint(message: &str) -> Option<&str> {
    message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .filter(|value| value.len() == 64)
}

/// 对各字段分别加长度前缀后计算命令摘要，避免拼接歧义。
fn command_fingerprint(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

/// 计算稳定 SHA-256 十六进制摘要。
fn stable_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// 校验强类型销售变更决策的待办、提交版本与当前责任。
fn validate_change_review_work_item(
    item: &WorkItem,
    review: &SalesChangeReview,
    submission_id: &SalesChangeSubmissionId,
    req: &ChangeReviewDecisionRequest,
    actor: &AuditActor,
) -> Result<()> {
    if item.base.version != req.expected_task_version {
        return Err(Error::ConflictError(
            "待办责任或版本已变化，请刷新后重试".to_string(),
        ));
    }
    let subject_version = submission_id.to_string();
    if req.expected_subject_version != subject_version || item.subject_version != subject_version {
        return Err(Error::ConflictError(
            "销售变更提交版本已变化，请刷新后重试".to_string(),
        ));
    }
    if item.approval_step_instance_id.is_some()
        || item.business_object_type != "sales_change_review"
        || item.business_object_id != review.base.id
        || item.work_item_type != change_review_work_item_type(review.review_stage)
    {
        return Err(Error::BusinessLogicError(
            "待办与当前销售变更复核不匹配".to_string(),
        ));
    }
    if !item.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是该待办责任人，或处理权已变化".to_string(),
        ));
    }
    Ok(())
}

/// 返回销售变更复核阶段注册的待办类型。
fn change_review_work_item_type(stage: SalesChangeReviewStage) -> WorkItemType {
    match stage {
        SalesChangeReviewStage::ProcurementImpact | SalesChangeReviewStage::OperationsImpact => {
            WorkItemType::SalesChangeImpactReview
        }
        SalesChangeReviewStage::FinanceReview => WorkItemType::SalesChangeFinanceReview,
    }
}

/// 在领域事实持久化前重验角色、数据范围与岗位分离。
async fn ensure_change_reviewer_eligible(
    db: &mongodb::Database,
    item: &WorkItem,
    submission: &entities::sales_review::SalesChangeSubmission,
    current_review_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let resolver = crate::approval::ApprovalAssigneeResolver::new(db.clone());
    if !resolver
        .user_is_eligible_for_assignment(actor_id, &item.owner_role, &item.owner_organization_id, executor)
        .await?
    {
        return Err(Error::Forbidden(
            "当前账号已不具备该待办的角色或数据范围".to_string(),
        ));
    }
    if submission.submitted_by == actor_id {
        return Err(Error::Forbidden("销售变更提交人不得复核自己的提交".to_string()));
    }
    let reviews = db
        .sales_change_reviews()
        .find_many(
            mongodb::bson::doc! { "sales_change_submission_id": submission.base.id.clone() },
            executor,
        )
        .await?;
    if reviews
        .iter()
        .any(|review| review.base.id != current_review_id && review.reviewer_id.as_deref() == Some(actor_id))
    {
        return Err(Error::Forbidden(
            "同一人不得同时承担履约影响确认与财务复核".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::ids::{SalesChangeReviewId, SalesChangeSubmissionId, WorkItemId};
    use entities::sales_review::{SalesChangeReview, SalesChangeReviewData, SalesChangeReviewStage};
    use entities::work_item::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };

    use super::{
        change_review_audit_id, change_review_fingerprint, change_review_receipt_fingerprint,
        change_review_receipt_message, validate_change_review_work_item,
    };
    use crate::audit::AuditActor;
    use crate::sales_review::ChangeReviewDecisionRequest;

    fn actor(id: &str) -> AuditActor {
        AuditActor::new(
            id.to_string(),
            format!("{id}@example.test"),
            entities::AccountKind::Admin,
        )
    }

    fn review() -> SalesChangeReview {
        SalesChangeReview::new(
            SalesChangeReviewId::new("review-1"),
            SalesChangeReviewData {
                sales_change_submission_id: SalesChangeSubmissionId::new("submission-1"),
                review_stage: SalesChangeReviewStage::FinanceReview,
            },
            "system",
        )
        .unwrap()
    }

    fn owned_task() -> WorkItem {
        let mut task = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::SalesChangeFinanceReview,
                approval_step_instance_id: None,
                business_object_type: "sales_change_review".to_string(),
                business_object_id: "review-1".to_string(),
                subject_version: "submission-1".to_string(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap();
        task.reassign("reviewer-1", Instant::from_unix_secs(2)).unwrap();
        task.base.version = 2;
        task
    }

    fn request(task_version: u64, subject_version: &str) -> ChangeReviewDecisionRequest {
        ChangeReviewDecisionRequest {
            work_item_id: "wi-1".to_string(),
            expected_task_version: task_version,
            expected_subject_version: subject_version.to_string(),
            decision_reason: None,
            idempotency_key: "op-1".to_string(),
        }
    }

    #[test]
    fn change_review_requires_current_task_and_subject_versions() {
        let task = owned_task();
        let review = review();
        let submission_id = SalesChangeSubmissionId::new("submission-1");
        let reviewer = actor("reviewer-1");

        assert!(validate_change_review_work_item(
            &task,
            &review,
            &submission_id,
            &request(2, "submission-1"),
            &reviewer,
        )
        .is_ok());
        assert!(validate_change_review_work_item(
            &task,
            &review,
            &submission_id,
            &request(1, "submission-1"),
            &reviewer,
        )
        .is_err());
        assert!(validate_change_review_work_item(
            &task,
            &review,
            &submission_id,
            &request(2, "submission-old"),
            &reviewer,
        )
        .is_err());
    }

    #[test]
    fn change_review_receipt_is_stable_and_hides_raw_key() {
        let req = request(2, "submission-1");
        let fingerprint = change_review_fingerprint("change-1", &req);
        let message = change_review_receipt_message(&fingerprint);

        assert_eq!(
            change_review_receipt_fingerprint(&message),
            Some(fingerprint.as_str())
        );
        let audit_id = change_review_audit_id(
            "reviewer-1",
            "sales_change_order.effective",
            "wi-1",
            "raw-secret-key",
        );
        assert!(!audit_id.contains("raw-secret-key"));
        assert!(message.len() <= 256);
    }

    /// 履约影响确认与财务复核不得充当流程节点。
    #[test]
    fn confirm_and_reject_actions_fail_closed() {
        let error = super::fail_closed_change_review_node().unwrap_err();
        assert!(error.to_string().contains("不得充当审批流程节点"));
        assert!(super::super::adapter::reject_legacy_change_review_node().is_err());
    }
}
