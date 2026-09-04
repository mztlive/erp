//! W18 确认任务与动作工厂（INT-E26：WorkItem/WorkflowAction 合同归属领域层）。
//!
//! Service 只注入任务 ID、批次、主体版本、操作人与时间，本模块独占
//! 任务类型、责任角色、组织、原因与动作映射。通用 BPM 不拥有 W18 与
//! WorkItem。无 I/O、时钟（时间由调用方注入）或密钥。

use crate::document_registry::{
    BusinessDocumentId, WorkflowAction, WorkflowActionData, WorkflowActionId, WorkflowActionType,
};
use crate::errors::Result;
use crate::ids::WorkItemId;
use crate::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

use super::{ConfirmationDecision, ConfirmationScope, LegacyImportBatchId, LegacyImportConfirmation};

/// 确认任务业务对象类型（`LEGACY_IMPORT_BATCH`）。
const CONFIRMATION_OBJECT_TYPE: &str = "LEGACY_IMPORT_BATCH";
/// 确认任务责任组织。
const CONFIRMATION_ORGANIZATION: &str = "company";
/// 确认任务派生原因码。
const CONFIRMATION_REASON_CODE: &str = "IMPORT_TRIAL_CONFIRMATION";

/// 构造采用固定责任范围维度的 W18 正常导入确认任务（INT-E26 领域工厂）。
///
/// 开放任务必须在创建时指定唯一个人责任人，责任角色仍由已注册
/// `confirmation_scope` 决定。任务 ID、主体版本与个人责任人由 Service
/// 显式注入。
///
/// # 参数
/// * `work_item_id` - 任务主键
/// * `batch_id` - 导入批次
/// * `subject_version` - 确认任务对应的试算版本
/// * `confirmation_scope` - 已注册确认范围
/// * `owner_user_id` - 当前个人责任人
///
/// # 返回
/// 返回带冻结 `responsibility_key` 的开放任务。
///
/// # 错误
/// 确认范围未注册、责任角色无法解析，或任务字段校验失败时返回错误。
///
/// # 约束
/// 纯确定性构造；不访问 MongoDB、时钟、ID 生成器或密钥。
pub fn confirmation_work_item(
    work_item_id: WorkItemId,
    batch_id: &LegacyImportBatchId,
    subject_version: String,
    confirmation_scope: &str,
    owner_user_id: &str,
) -> Result<WorkItem> {
    let scope = ConfirmationScope::parse(confirmation_scope)?;
    let owner_role = scope.owner_role();
    let scope_code = scope.as_str();
    WorkItem::new_with_responsibility_key(
        work_item_id,
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: CONFIRMATION_OBJECT_TYPE.to_string(),
            business_object_id: batch_id.to_string(),
            subject_version,
            owner_role: owner_role.to_string(),
            owner_organization_id: CONFIRMATION_ORGANIZATION.to_string(),
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some(CONFIRMATION_REASON_CODE.to_string()),
            impact_summary: Some(format!("{scope_code}范围导入试算待业务确认")),
        },
        scope_code,
    )
}

/// 构造确认事实对应的追加式 `workflow_action`（INT-E26 领域工厂）。
///
/// 确认映射为 `Confirm/CONFIRMED`，退回映射为 `Reject/REJECTED` 并携带
/// 退回原因；无决策事实失败关闭。动作 ID 与操作人由 Service 显式注入。
///
/// # 参数
/// * `id` - 动作主键
/// * `confirmation` - 已决策的确认事实
/// * `actor_id` - 实际确认或退回人
///
/// # 返回
/// 返回追加式工作流动作。
///
/// # 错误
/// 导入确认动作缺少领域决策或动作字段校验失败时返回错误。
///
/// # 约束
/// 纯确定性构造；不访问 MongoDB、时钟、ID 生成器或密钥。
pub fn confirmation_workflow_action(
    id: WorkflowActionId,
    confirmation: &LegacyImportConfirmation,
    actor_id: &str,
) -> Result<WorkflowAction> {
    let (action_type, to_status, comment) = match confirmation.decision {
        Some(ConfirmationDecision::ConfirmScope) => (WorkflowActionType::Confirm, "CONFIRMED", None),
        Some(ConfirmationDecision::ReturnForFix) => (
            WorkflowActionType::Reject,
            "REJECTED",
            confirmation.reason_code.clone(),
        ),
        None => {
            return Err(crate::errors::Error::from("导入确认动作缺少领域决策"));
        }
    };
    WorkflowAction::new(
        id,
        WorkflowActionData {
            document_id: BusinessDocumentId::new(confirmation.batch_id.to_string()),
            action_type,
            from_status: "PENDING".to_string(),
            to_status: to_status.to_string(),
            actor_id: actor_id.to_string(),
            actor_role: confirmation.owner_role.clone(),
            comment,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{confirmation_work_item, confirmation_workflow_action};
    use crate::document_registry::WorkflowActionType;
    use crate::ids::{LegacyImportBatchId, LegacyImportConfirmationId, WorkItemId};
    use crate::legacy_import::LegacyImportConfirmationData;
    use crate::work_item::WorkItemType;

    fn confirmation(
        decision: crate::legacy_import::ConfirmationDecision,
    ) -> crate::legacy_import::LegacyImportConfirmation {
        use crate::legacy_import::LegacyImportConfirmation;
        let mut confirmation = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("confirmation-1"),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: "SALES".to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: 2,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new("work-item-1"),
            },
        )
        .unwrap();
        match decision {
            crate::legacy_import::ConfirmationDecision::ConfirmScope => {
                confirmation
                    .decide(
                        decision,
                        "user-1",
                        crate::common::time::Instant::from_unix_secs(1_700_000_000),
                        None,
                        Some("确认".to_string()),
                    )
                    .unwrap();
            }
            crate::legacy_import::ConfirmationDecision::ReturnForFix => {
                confirmation
                    .decide(
                        decision,
                        "user-1",
                        crate::common::time::Instant::from_unix_secs(1_700_000_000),
                        Some("REWORK".to_string()),
                        None,
                    )
                    .unwrap();
            }
        }
        confirmation
    }

    #[test]
    fn work_item_pins_type_org_subject_and_all_scopes() {
        for scope in ["SALES", "PROCUREMENT", "OPERATIONS", "WAREHOUSE", "FINANCE"] {
            let item = confirmation_work_item(
                WorkItemId::new(format!("work-item-{scope}")),
                &LegacyImportBatchId::new("batch-1"),
                "subject-1".to_string(),
                &format!(" {scope} "),
                "user-1",
            )
            .unwrap();
            assert_eq!(item.work_item_type, WorkItemType::ImportBusinessConfirmation);
            assert_eq!(item.business_object_id, "batch-1");
            assert_eq!(item.subject_version, "subject-1");
            assert_eq!(item.responsibility_key(), Some(scope));
            assert_eq!(item.owner_organization_id, "company");
        }
        assert!(confirmation_work_item(
            WorkItemId::new("work-item-1"),
            &LegacyImportBatchId::new("batch-1"),
            "subject-1".to_string(),
            "UNKNOWN",
            "user-1",
        )
        .is_err());
        assert!(confirmation_work_item(
            WorkItemId::new("work-item-1"),
            &LegacyImportBatchId::new("batch-1"),
            "subject-1".to_string(),
            "SALES",
            "   ",
        )
        .is_err());
    }

    #[test]
    fn workflow_action_maps_both_decisions() {
        use crate::document_registry::WorkflowActionId;
        let confirmed = confirmation(crate::legacy_import::ConfirmationDecision::ConfirmScope);
        let action =
            confirmation_workflow_action(WorkflowActionId::new("action-1"), &confirmed, "user-1").unwrap();
        assert_eq!(action.action_type, WorkflowActionType::Confirm);
        assert_eq!(action.to_status, "CONFIRMED");

        let rejected = confirmation(crate::legacy_import::ConfirmationDecision::ReturnForFix);
        let action =
            confirmation_workflow_action(WorkflowActionId::new("action-2"), &rejected, "user-1").unwrap();
        assert_eq!(action.action_type, WorkflowActionType::Reject);
        assert_eq!(action.to_status, "REJECTED");
        assert_eq!(action.comment.as_deref(), Some("REWORK"));
    }
}
