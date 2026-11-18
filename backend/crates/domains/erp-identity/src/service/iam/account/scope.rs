//! 新建超级管理员的人员范围初始化。
use entity_core::BaseModel;
use mongodb::Database;
use persistence_core::Executor;

use crate::access_control::DataScopeType;
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression, PersonScopeTerm};
use crate::entity::access_control::scope_spec;
use crate::{AccessControlExt, Result};

// 显式初始化新超级管理员的公司范围；已有配置或撤销记录不重建。
// 明确初始化完成。
//
// 数据库失败拒绝原事务。
//
// 数据库、人员及调用方事务。
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
                    target_dimension: spec.supported_dimensions()[0],
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
