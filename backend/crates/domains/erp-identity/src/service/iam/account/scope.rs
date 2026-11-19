//! 新建超级管理员的人员范围初始化。
use entity_core::BaseModel;
use mongodb::Database;
use persistence_core::Executor;

use crate::access_control::DataScopeType;
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression, PersonScopeTerm};
use crate::entity::access_control::scope_spec;
use crate::{AccessControlExt, Result};

/// 仅在新建超级管理员的事务内，为每个可配置动作初始化公司人员范围；已有账号的配置或撤销记录保留原样。
/// 复用创建账号的执行器，任一范围写入失败即返回错误，由调用方回滚原事务。
pub(crate) async fn seed_company(db: &Database, user: &str, executor: &mut dyn Executor) -> Result<()> {
    for spec in scope_spec::specs() {
        if !spec.configurable() {
            continue;
        }
        let scope = PersonDataScope {
            base: BaseModel::new(id_generator::next_id()),
            user_id: user.into(),
            resource: spec.permission().resource().into(),
            action: spec.permission().action().into(),
            expression: PersonScopeExpression {
                additive: false,
                history_read: false,
                alternatives: vec![vec![PersonScopeTerm {
                    scope_type: DataScopeType::Company,
                    target_dimension: spec.dimensions()[0],
                    target_mode: None,
                    include_descendants: None,
                    scope_targets: vec![],
                }]],
                condition: None,
            },
        };
        db.person_data_scopes().create(&scope, executor).await?;
    }
    Ok(())
}
