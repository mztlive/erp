//! 普通角色新获业务动作时，事务内显式初始化本人范围。
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::access_control::{
    DataScope, DataScopeData, DataScopeId, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
};
use crate::entity::access_control::role_defaults::new_default_actions;
use crate::entity::rbac::PermissionSet;
use crate::repository::prelude::*;
use crate::service::access_control::consumers::department_resources;
use crate::{AccessControlExt, Result};

/// 仅为新增动作补默认范围，任何既有或撤销范围历史都不自动恢复。
/// # 参数
/// * `db`、`executor` - 角色保存事务。
/// * `role_id` - 被保存角色。
/// * `previous`、`next` - 保存前后的明确动作资格。
/// # 返回
/// 缺失且无配置历史的新动作写入本人规则。
/// # 错误
/// 规则构造或持久化失败时由外层事务回滚。
pub(super) async fn initialize_defaults(
    db: &Database,
    role_id: &str,
    previous: &PermissionSet,
    next: &PermissionSet,
    executor: &mut dyn Executor,
) -> Result<()> {
    for (resource, actions) in department_resources() {
        let actions = new_default_actions(resource, actions, previous, next)?;
        let mut fresh = Vec::new();
        for action in actions {
            if !db.data_scopes().has_role_action_history(role_id, resource, &action, executor).await? {
                fresh.push(action);
            }
        }
        if fresh.is_empty() {
            continue;
        }
        let scope = DataScope::new(
            DataScopeId::new(next_id()),
            DataScopeData {
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: fresh,
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
                subject_type: DataScopeSubjectType::Role,
                subject_id: role_id.into(),
                scope_type: DataScopeType::SelfOwned,
                scope_targets: vec![],
            },
        )?;
        db.data_scopes().create(&scope, executor).await?;
    }
    Ok(())
}
