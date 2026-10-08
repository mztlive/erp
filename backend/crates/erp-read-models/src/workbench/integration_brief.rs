//! 集成异常与对账差异的可读事项简报。

use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_integration::dto::{DifferenceView, ErrorTaskView};
use erp_integration::entity::integration_ops::{ErrorClass, IntegrationErrorTask, ReconciliationDifference};
use erp_workflow::WorkflowAuthorizationPort;
use persistence_core::Executor;

use super::integration_brief_labels::{
    business_type_label, difference_type_label, evidence_category, external_reference,
};
use super::{
    ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchReadService, brief, dto, object_ids,
};
use crate::errors::Result;
use crate::integration_center::display::object_labels;
use crate::integration_center::error_labels::{IntegrationTaskLabels, integration_error_labels};

/// 组装集成错误任务的结构化简报。
///
/// # 参数
/// * `task` - 集成错误任务正式事实
///
/// # 返回
/// 返回错误分类、关联参考号、发生时间、重试证据、脱敏摘要和处理结果。
///
/// # 错误
/// 无。
fn integration_error_brief_source(
    task: &IntegrationErrorTask,
    labels: &IntegrationTaskLabels,
) -> brief::ObjectBriefSource {
    let occurred_at = base_created_at_datetime(task.base.created_at);
    let last_attempt_at = task.last_attempt_at.map(brief::format_instant_datetime);
    let attempt_count = format!("{} 次", task.attempt_count);
    let resolved_at = task.resolved_at.map(brief::format_instant_datetime);
    let resolution_type = task.resolution_type.map(|value| value.label().to_string());
    let reference = labels.business_object_label.clone().or_else(|| labels.message_label.clone());
    let mut sections = Vec::new();
    brief::push_section(&mut sections, "错误分类", Some(task.error_class.label()), false);
    brief::push_section(&mut sections, "状态", Some(task.status.label()), false);
    let business_label = labels
        .business_object_label
        .as_deref()
        .or(task.business_object_id.as_ref().map(|_| "对象名称未维护"));
    brief::push_section(&mut sections, "业务对象", business_label, false);
    let message_label =
        labels.message_label.as_deref().or(task.message_id.as_ref().map(|_| "消息名称未维护"));
    brief::push_section(&mut sections, "关联消息", message_label, false);
    brief::push_section(&mut sections, "发生时间", occurred_at.as_deref(), false);
    brief::push_section(&mut sections, "重试记录", Some(attempt_count.as_str()), false);
    brief::push_section(&mut sections, "最近尝试", last_attempt_at.as_deref(), false);
    brief::push_section(&mut sections, "错误摘要", task.last_attempt_summary.as_deref(), false);
    let owner_role = task.owner_role.as_deref().map(dto::role_label);
    brief::push_section(&mut sections, "责任角色", owner_role.as_deref(), false);
    brief::push_section(&mut sections, "责任人", task.owner_user_id.as_deref(), false);
    brief::push_section(
        &mut sections,
        "安全下一步",
        Some(integration_error_next_step(task.error_class)),
        false,
    );
    brief::push_section(&mut sections, "解决方式", resolution_type.as_deref(), false);
    let resolution = evidence_category(task.resolution.as_deref());
    brief::push_section(&mut sections, "处理证据类别", resolution.as_deref(), false);
    brief::push_section(&mut sections, "完成时间", resolved_at.as_deref(), false);
    brief::ObjectBriefSource {
        customer: None,
        amount_label: None,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
        list_summary: brief::join_list_summary([
            Some(task.error_class.label().to_string()),
            reference,
            Some(format!("重试 {attempt_count}")),
            task.last_attempt_summary.as_deref().and_then(brief::non_empty),
        ]),
        extra_sections: sections,
    }
}

/// 按固定错误分类返回可执行且安全的下一步。
///
/// # 参数
/// * `error_class` - 错误分类
///
/// # 返回
/// 返回不泄露内部实现的处理指引。
///
/// # 错误
/// 无。
fn integration_error_next_step(error_class: ErrorClass) -> &'static str {
    match error_class {
        ErrorClass::CapabilityGap => "确认目标系统能力后转人工补偿或补齐能力",
        ErrorClass::MappingError => "修复映射并验证业务键后再重放",
        ErrorClass::BusinessRejected => "核对拒绝原因并修正业务输入后重新提交",
        ErrorClass::TransientFailure | ErrorClass::RateLimited => "核对最近尝试摘要，按原幂等业务键重试",
        ErrorClass::ResultUnknown => "先查询原请求结果，确认无结果后才允许重放",
        ErrorClass::AuthSignature => "修复鉴权或签名配置，验证通过后再重试",
        ErrorClass::OutOfOrder => "补齐前置事实并确认顺序后再重放",
    }
}

/// 组装对账差异的结构化业务异常简报。
///
/// # 参数
/// * `difference` - 不可变对账差异事实
///
/// # 返回
/// 返回异常对象、差异类型、发现时间与两侧证据引用。
///
/// # 错误
/// 无。
fn reconciliation_difference_brief_source(
    difference: &ReconciliationDifference,
    object_label: Option<&str>,
) -> brief::ObjectBriefSource {
    let occurred_at = base_created_at_datetime(difference.base.created_at);
    let evidence_count = usize::from(difference.left_fact_reference.is_some())
        + usize::from(difference.right_fact_reference.is_some());
    let evidence_summary = format!("{evidence_count} 侧证据");
    let object_type = business_type_label(&difference.business_object_type);
    let difference_type = difference_type_label(&difference.difference_type);
    let external = external_reference(&difference.business_object_type, &difference.business_object_id);
    let object_label = object_label.or(external.as_deref()).unwrap_or("对象名称未维护");
    let left_evidence = evidence_category(difference.left_fact_reference.as_deref());
    let right_evidence = evidence_category(difference.right_fact_reference.as_deref());
    let mut sections = Vec::new();
    brief::push_section(&mut sections, "异常对象", Some(object_type), false);
    brief::push_section(&mut sections, "业务对象", Some(object_label), false);
    brief::push_section(&mut sections, "差异类型", Some(difference_type), false);
    brief::push_section(&mut sections, "发现时间", occurred_at.as_deref(), false);
    brief::push_section(&mut sections, "左侧证据类别", left_evidence.as_deref(), false);
    brief::push_section(&mut sections, "右侧证据类别", right_evidence.as_deref(), false);
    brief::push_section(
        &mut sections,
        "关闭条件",
        Some("两侧事实已核对，并引用正式处理结果或无需处理的证据"),
        false,
    );
    brief::ObjectBriefSource {
        customer: None,
        amount_label: None,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
        list_summary: brief::join_list_summary([
            Some(object_type.to_string()),
            Some(object_label.to_string()),
            Some(difference_type.to_string()),
            Some(evidence_summary),
        ]),
        extra_sections: sections,
    }
}

/// 把实体基础时间转换为业务时区展示；非法或测试零值不上屏。
///
/// # 参数
/// * `created_at` - 实体 Unix 秒级创建时间
///
/// # 返回
/// 返回分钟级时间；零值或超出 `i64` 时返回 `None`。
///
/// # 错误
/// 无。
fn base_created_at_datetime(created_at: u64) -> Option<String> {
    (created_at > 0)
        .then(|| i64::try_from(created_at).ok())
        .flatten()
        .map(Instant::from_unix_secs)
        .map(brief::format_instant_datetime)
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    pub(super) async fn load_integration_error_task_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::IntegrationErrorTask);
        if ids.is_empty() {
            return Ok(());
        }
        let tasks = self.facts_reader().read_integration_errors(&ids, executor).await?;
        let views = tasks.iter().cloned().map(ErrorTaskView::from).collect::<Vec<_>>();
        let mut labels = integration_error_labels(&self.db, &views, executor).await?;
        for task in tasks {
            let mut fact =
                WorkbenchObjectFact::from_authority(super::authority::command::integration_error_fact(&task));
            fact.display.brief_source = Some(integration_error_brief_source(
                &task,
                &labels.remove(&task.base.id).unwrap_or_default(),
            ));
            facts.insert((ObjectKind::IntegrationErrorTask, task.base.id.clone()), fact);
        }
        Ok(())
    }

    pub(super) async fn load_reconciliation_difference_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReconciliationDifference);
        if ids.is_empty() {
            return Ok(());
        }
        let differences = self.facts_reader().read_reconciliation_differences(&ids, executor).await?;
        let views = differences.iter().cloned().map(DifferenceView::from).collect::<Vec<_>>();
        let labels = object_labels(&self.db, &views, executor).await?;
        for difference in differences {
            let mut fact = WorkbenchObjectFact::from_authority(
                super::authority::command::reconciliation_difference_fact(&difference),
            );
            fact.display.label = format!("业务异常 · {}", difference_type_label(&difference.difference_type));
            fact.display.brief_source = Some(reconciliation_difference_brief_source(
                &difference,
                labels
                    .get(&(difference.business_object_type.clone(), difference.business_object_id.clone()))
                    .map(String::as_str),
            ));
            facts.insert((ObjectKind::ReconciliationDifference, difference.base.id.clone()), fact);
        }
        Ok(())
    }
}

#[cfg(test)]
mod integration_brief_tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{IntegrationErrorTaskId, ReconciliationDifferenceId};
    use erp_integration::entity::integration_ops::{
        ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, ReconciliationDifference,
        ReconciliationDifferenceData,
    };

    use super::{
        IntegrationTaskLabels, integration_error_brief_source, reconciliation_difference_brief_source,
    };

    #[test]
    fn integration_brief_exposes_retry_and_redacted_error_evidence() {
        let mut task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("integration-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("EXT-2026-001".to_string()),
                error_class: ErrorClass::ResultUnknown,
                owner_role: Some("integration-operator".to_string()),
                owner_user_id: Some("operator-1".to_string()),
                owner_org_unit_id: "org-ops".to_string(),
            },
        )
        .unwrap();
        task.attempt_count = 2;
        task.last_attempt_at = Some(Instant::from_unix_secs(1_787_457_600));
        task.last_attempt_summary = Some("目标系统超时，未取得业务结果".to_string());

        let brief = integration_error_brief_source(
            &task,
            &IntegrationTaskLabels {
                business_object_label: Some("供应商订单 SF-1001".to_string()),
                message_label: None,
            },
        );

        assert!(brief.extra_sections.iter().any(|section| {
            section.label == "业务对象" && section.value == "供应商订单 SF-1001"
        }));
        assert!(
            brief.extra_sections.iter().any(|section| section.label == "重试记录" && section.value == "2 次")
        );
        assert!(brief.list_summary.contains("目标系统超时"));
        let missing_names = integration_error_brief_source(&task, &IntegrationTaskLabels::default());
        assert!(
            missing_names
                .extra_sections
                .iter()
                .any(|section| section.label == "业务对象" && section.value == "对象名称未维护")
        );
        assert!(!missing_names.list_summary.contains("EXT-2026-001"));
        assert!(!missing_names.extra_sections.iter().any(|section| section.value == "EXT-2026-001"));
    }

    #[test]
    fn reconciliation_brief_exposes_both_immutable_evidence_references() {
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("difference-1"),
            ReconciliationDifferenceData {
                business_object_type: "商城订单".to_string(),
                business_object_id: "MALL-1001".to_string(),
                difference_type: "金额不一致".to_string(),
                left_fact_reference: Some("mall-snapshot:7".to_string()),
                right_fact_reference: Some("erp-revision:9".to_string()),
                owner_user_id: "operator-1".to_string(),
                owner_org_unit_id: "org-finance".to_string(),
            },
        )
        .unwrap();

        let brief = reconciliation_difference_brief_source(&difference, None);

        assert!(brief.extra_sections.iter().any(|section| section.label == "左侧证据类别"));
        assert!(brief.extra_sections.iter().any(|section| section.label == "右侧证据类别"));
        assert!(brief.list_summary.contains("2 侧证据"));
        assert!(
            brief
                .extra_sections
                .iter()
                .any(|section| section.label == "业务对象" && section.value == "MALL-1001")
        );
        assert!(
            brief
                .extra_sections
                .iter()
                .any(|section| section.label == "左侧证据类别" && section.value == "商城订单事实证据")
        );
        assert!(!brief.list_summary.contains("mall-snapshot"));
    }
}
