//! W29 独立人工任务的服务侧装配（INT-E19）。
//!
//! 责任角色、任务类型、优先级与任务装配规则由 `entities::integration_ops`
//! 的 W29 政策与工厂独占；本模块只生成任务主键、传入当前责任人与时间并
//! 映射错误，不维护第二份规则。

use entities::common::time::Instant;
use entities::ids::WorkItemId;
use entities::integration_ops::{
    difference_owner_role, new_difference_work_item, new_error_work_item, IntegrationErrorTask,
    ReconciliationDifference,
};
use entities::work_item::WorkItem;
use id_generator::next_id;

use crate::errors::{Error, Result};

/// 构造指定到人的错误处理任务（主键与时间由服务注入）。
///
/// # 参数
/// * `task` - 集成错误事实
/// * `owner_user_id` - 创建时明确解析的当前责任人
///
/// # 返回
/// 返回新建的开放正式责任任务。
///
/// # 错误
/// 责任字段或关联字段不满足任务实体不变式时返回领域逻辑错误。
///
/// # 约束
/// 责任政策归领域，本函数只做 ID/时间注入与错误透传。
pub(crate) fn error_work_item(task: &IntegrationErrorTask, owner_user_id: &str) -> Result<WorkItem> {
    new_error_work_item(WorkItemId::new(next_id()), task, owner_user_id, Instant::now()).map_err(Into::into)
}

/// 构造指定到人的对账差异任务（主键与时间由服务注入）。
///
/// # 参数
/// * `difference` - 对账差异事实
/// * `owner_user_id` - 创建时明确解析的当前责任人
///
/// # 返回
/// 返回新建的开放正式责任任务。
///
/// # 错误
/// 差异类型未注册责任规则时返回 `BusinessLogicError`（wire 不变），
/// 其余不变式失败返回领域逻辑错误。注册表预检只做错误类别映射，
/// 规则本身仍由领域工厂独占（失败关闭）。
///
/// # 约束
/// 责任政策归领域，本函数只做 ID/时间注入与错误映射。
pub(super) fn difference_work_item(
    difference: &ReconciliationDifference,
    owner_user_id: &str,
) -> Result<WorkItem> {
    difference_owner_role(&difference.difference_type)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    new_difference_work_item(
        WorkItemId::new(next_id()),
        difference,
        owner_user_id,
        Instant::now(),
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use entities::ids::{IntegrationErrorTaskId, ReconciliationDifferenceId};
    use entities::integration_ops::{
        ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, ReconciliationDifference,
        ReconciliationDifferenceData,
    };

    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("producer.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 分层守卫（INT-E19）：责任矩阵与任务装配归领域，服务只注入 ID/时间。
    ///
    /// 锁定旧规则源（角色/类型/优先级矩阵与 `WorkItemData` 装配）已删除；
    /// 服务只保留主键生成、责任人/时间注入与错误映射。
    #[test]
    fn responsibility_tables_are_owned_by_domain() {
        let source = production_source();
        assert!(!source.contains("fn error_owner_role"));
        assert!(!source.contains("fn error_work_item_type"));
        assert!(!source.contains("fn difference_owner_role"));
        assert!(!source.contains("fn error_priority"));
        assert!(!source.contains("WorkItemData {"));
        assert!(!source.contains("role-operations"));
        assert!(source.contains("new_error_work_item("));
        assert!(source.contains("new_difference_work_item("));
        assert!(source.contains("next_id()"));
    }

    #[test]
    fn error_ctor_delegates_formal_fields_to_domain() {
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("task-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("so-1".to_string()),
                error_class: ErrorClass::ResultUnknown,
                owner_role: None,
                owner_user_id: None,
            },
        )
        .unwrap();

        let item = super::error_work_item(&task, "user-1").unwrap();
        assert_eq!(item.owner_role.as_str(), "role-sysadmin");
        assert_eq!(item.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(item.business_object_id.as_str(), "task-1");
    }

    #[test]
    fn difference_ctor_keeps_business_error_for_unknown_type() {
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-1"),
            ReconciliationDifferenceData {
                business_object_type: "mall_order".to_string(),
                business_object_id: "MO-1".to_string(),
                difference_type: "free_form_type".to_string(),
                left_fact_reference: Some("mall_order_fact://f-1".to_string()),
                right_fact_reference: None,
            },
        )
        .unwrap();

        let error = super::difference_work_item(&difference, "user-1").unwrap_err();
        assert!(matches!(error, crate::errors::Error::BusinessLogicError(_)));

        let known = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-2"),
            ReconciliationDifferenceData {
                business_object_type: "mall_order".to_string(),
                business_object_id: "MO-2".to_string(),
                difference_type: "amount_mismatch".to_string(),
                left_fact_reference: Some("mall_order_fact://f-2".to_string()),
                right_fact_reference: None,
            },
        )
        .unwrap();
        let item = super::difference_work_item(&known, "user-2").unwrap();
        assert_eq!(item.owner_role.as_str(), "role-finance");
    }
}
