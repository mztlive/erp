//! W17 映射差异正式责任任务的强类型政策与工厂。
//!
//! 映射类型到责任角色、SLA、原因码与影响摘要的固定映射属于领域政策，
//! 由本域独占；任务主键与创建时间由调用方注入，责任人解析与持久化仍归服务。

use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::WorkItemId;
use crate::mall_sync::MappingTaskType;

use super::entity::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

/// W17 映射任务正式责任的业务对象类型。
pub const MAPPING_WORK_ITEM_OBJECT_TYPE: &str = "MASTER_MAPPING_TASK";
/// W17 映射任务正式责任的归属组织。
pub const MAPPING_WORK_ITEM_ORGANIZATION: &str = "company";

/// W17 映射任务正式责任的确定性规格。
///
/// 任务主键与创建时间由调用方注入；责任角色与责任人由服务按映射类型解析传入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingWorkItemSpec {
    /// 当前映射差异类型，决定 SLA、原因码与影响摘要。
    pub mapping_type: MappingTaskType,
    /// 映射任务主键。
    pub mapping_task_id: String,
    /// 映射任务冻结的业务主题版本。
    pub subject_version: String,
    /// 业务责任角色。
    pub owner_role: String,
    /// 当前个人责任人。
    pub owner_user_id: String,
}

/// 为已确定责任角色的映射差异构造唯一正式任务。
///
/// # 参数
/// * `id` - 正式任务主键（调用方生成的稳定 ID）
/// * `spec` - 映射类型、任务身份与责任归属规格
/// * `now` - 调用方时间，用于派生 SLA 时限
///
/// # 返回
/// 返回新建的开放正式责任任务。
///
/// # 错误
/// 责任字段为空/超长，或开放任务缺少个人责任人时返回错误。
///
/// # 约束
/// 纯领域构造，不访问数据库、不生成 ID、不读取全局时钟；
/// 时限为 `now + 固定 SLA`，同输入必得同时限。
pub fn new_mapping_work_item(id: WorkItemId, spec: MappingWorkItemSpec, now: Instant) -> Result<WorkItem> {
    let due_at = Instant::from_unix_secs(now.unix_secs().saturating_add(spec.mapping_type.sla_seconds()));
    WorkItem::new_at(
        id,
        WorkItemData {
            work_item_type: WorkItemType::BusinessException,
            business_object_type: MAPPING_WORK_ITEM_OBJECT_TYPE.to_string(),
            business_object_id: spec.mapping_task_id,
            subject_version: spec.subject_version,
            owner_role: spec.owner_role,
            owner_organization_id: MAPPING_WORK_ITEM_ORGANIZATION.to_string(),
            owner_user_id: spec.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(due_at),
            reason_code: Some(format!(
                "MALL_MAPPING_{}",
                spec.mapping_type.as_str().to_uppercase()
            )),
            impact_summary: Some(format!("{}主数据映射差异待确认", spec.mapping_type.label())),
        },
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::{new_mapping_work_item, MappingWorkItemSpec, MAPPING_WORK_ITEM_OBJECT_TYPE};
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;
    use crate::mall_sync::MappingTaskType;
    use crate::work_item::WorkItemType;

    fn spec(mapping_type: MappingTaskType) -> MappingWorkItemSpec {
        MappingWorkItemSpec {
            mapping_type,
            mapping_task_id: "task-1".to_string(),
            subject_version: "7".to_string(),
            owner_role: "role-sales".to_string(),
            owner_user_id: "user-1".to_string(),
        }
    }

    #[test]
    fn every_mapping_type_forms_typed_responsibility_task() {
        let now = Instant::from_unix_secs(1_700_000_000);
        for mapping_type in [
            MappingTaskType::Customer,
            MappingTaskType::Contract,
            MappingTaskType::SettlementEntity,
            MappingTaskType::VoucherCategory,
            MappingTaskType::UniqueLineItem,
            MappingTaskType::AmountFormat,
        ] {
            let item = new_mapping_work_item(
                WorkItemId::new(format!("wi-{}", mapping_type.as_str())),
                spec(mapping_type),
                now,
            )
            .unwrap();
            assert_eq!(item.work_item_type, WorkItemType::BusinessException);
            assert_eq!(item.business_object_type.as_str(), MAPPING_WORK_ITEM_OBJECT_TYPE);
            assert_eq!(item.business_object_id.as_str(), "task-1");
            assert_eq!(item.subject_version.as_str(), "7");
            assert_eq!(item.owner_user_id.as_deref(), Some("user-1"));
            assert_eq!(
                item.due_at,
                Some(Instant::from_unix_secs(
                    now.unix_secs() + mapping_type.sla_seconds()
                ))
            );
            assert!(item
                .reason_code
                .as_deref()
                .is_some_and(|code| code.starts_with("MALL_MAPPING_")));
        }
    }

    #[test]
    fn sla_matches_registered_policy_per_type() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let voucher = new_mapping_work_item(
            WorkItemId::new("wi-voucher"),
            spec(MappingTaskType::VoucherCategory),
            now,
        )
        .unwrap();
        assert_eq!(
            voucher.due_at,
            Some(Instant::from_unix_secs(now.unix_secs() + 4 * 60 * 60))
        );
        let customer = new_mapping_work_item(
            WorkItemId::new("wi-customer"),
            spec(MappingTaskType::Customer),
            now,
        )
        .unwrap();
        assert_eq!(
            customer.due_at,
            Some(Instant::from_unix_secs(now.unix_secs() + 24 * 60 * 60))
        );
    }

    #[test]
    fn missing_owner_is_rejected() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let without_user = MappingWorkItemSpec {
            owner_user_id: "   ".to_string(),
            ..spec(MappingTaskType::Customer)
        };
        assert!(new_mapping_work_item(WorkItemId::new("wi-no-user"), without_user, now).is_err());
        let without_role = MappingWorkItemSpec {
            owner_role: String::new(),
            ..spec(MappingTaskType::Customer)
        };
        assert!(new_mapping_work_item(WorkItemId::new("wi-no-role"), without_role, now).is_err());
    }
}
