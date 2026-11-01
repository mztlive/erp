//! 采购新一轮报价必须同时证明历史参与及当前责任，历史读取不授予写入权。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRule;
use erp_procurement::repository::ProcurementResponsibilityExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::service::procurement_responsibility::CandidateResolution;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_workflow::entity::work_item::{WorkItemStatus, inquiry_subject_version};
use persistence_core::Executor;

use super::SalesInquiryProcess;
use super::dto::{InquiryMutationView, InquiryTaskReasonCommand};
use super::execution::InquiryCommand;
use crate::{Error, Result};

impl SalesInquiryProcess {
    /// 从本人已完成的询价任务主动开启新一轮整体报价。
    /// # 参数
    /// `actor` 为采购，`id` 为询价，`request` 包含原任务、版本、原因及命令键。
    /// # 返回
    /// 原子更新询价并创建当前责任任务；重放返回原回执。
    /// # 错误
    /// 任务不是本人完成、当前责任已变化、需求或版本变化、状态不允许时拒绝。
    pub async fn renew(
        &self,
        actor: &AuditActor,
        id: &str,
        request: InquiryTaskReasonCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Renew(request)).await
    }

    pub(super) async fn renew_work(
        &self,
        actor: &AuditActor,
        id: &str,
        request: &InquiryTaskReasonCommand,
        executor: &mut dyn Executor,
    ) -> Result<(SalesInquiry, Vec<String>)> {
        self.access().ensure_procurement_permission(actor, "publish", executor).await?;
        let (mut inquiry, task) =
            self.access().require_read_task(actor, "detail", id, &request.task_id, executor).await?;
        if task.status != WorkItemStatus::Completed
            || task.base.version != request.task_version
            || task.subject_version != inquiry_subject_version(inquiry.requirement_version)
        {
            return Err(Error::ConflictError("询价需求或任务已变化，请从采购询价列表刷新后重试".into()));
        }
        let assignments = self.resolve_assignments(&inquiry, executor).await?;
        if !assignments.iter().any(|(assignment, _)| assignment.owner_user_id == actor.id())
            && !self.renewal_dispatcher(&inquiry, actor.id(), executor).await?
        {
            return Err(Error::Forbidden("当前已不负责该询价商品，不能发起新一轮报价".into()));
        }
        self.access().assignment_owner(actor.id(), executor).await?;
        inquiry.renew_quote(request.expected_version, actor.id(), request.reason.clone(), Instant::now())?;
        let tasks = self.replace_tasks(actor, &mut inquiry, executor).await?;
        Ok((inquiry, tasks))
    }

    /// 命中池内的历史采购可发起后续轮次，下一位调度人只决定新任务归属。
    async fn renewal_dispatcher(
        &self,
        inquiry: &SalesInquiry,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let candidates = self.assignment_candidates(inquiry, executor).await?;
        let rules = self
            .db
            .procurement_responsibility_rules()
            .list_active_procurement_responsibility_rules(executor)
            .await?;
        Ok(dispatcher_can_renew(actor, &candidates, &rules))
    }
}

/// 仅本单仍命中相同规则池且原采购仍在启用池内时保留发起资格。
fn dispatcher_can_renew(
    actor: &str,
    candidates: &[CandidateResolution],
    active_rules: &[ProcurementResponsibilityRule],
) -> bool {
    candidates.iter().any(|candidate| {
        active_rules.iter().any(|rule| {
            rule.is_active()
                && rule.base.id == candidate.rule_id
                && rule.owner_user_ids.iter().any(|owner| owner == actor)
        })
    })
}

#[cfg(test)]
mod tests {
    use erp_core::ids::ProcurementResponsibilityRuleId;
    use erp_procurement::entity::procurement_responsibility::{
        EnableStatus, ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
    };

    use super::*;

    #[test]
    fn default_rotation_does_not_revoke_previous_buyers_renewal_eligibility() {
        let rule = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("rule-a"),
            ProcurementResponsibilityRuleData {
                rule_type: ProcurementResponsibilityRuleType::DefaultDispatcher,
                sku_id: None,
                category_id: None,
                product_kind: None,
                owner_user_ids: vec!["buyer-a".into()],
                strategy: Default::default(),
                status: EnableStatus::Active,
            },
            "admin",
        )
        .unwrap();
        let mut candidates = vec![CandidateResolution {
            line_key: "line".into(),
            owner_user_id: "buyer-b".into(),
            owner_name: "采购乙".into(),
            rule_id: "rule-a".into(),
            rule_type: ProcurementResponsibilityRuleType::DefaultDispatcher,
        }];
        assert!(dispatcher_can_renew("buyer-a", &candidates, std::slice::from_ref(&rule)));
        assert!(!dispatcher_can_renew("removed-buyer", &candidates, std::slice::from_ref(&rule)));
        assert!(!dispatcher_can_renew("buyer-a", &candidates, &[]));
        candidates[0].rule_id = "different-pool".into();
        assert!(!dispatcher_can_renew("buyer-a", &candidates, &[rule]));
    }
}
