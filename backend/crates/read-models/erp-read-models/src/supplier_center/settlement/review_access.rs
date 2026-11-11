//! 复核决定原任务的完整功能与指定复核人事实。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactKind, FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets,
    Result as IdentityResult, SharedRbacService,
};
use erp_workflow::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

struct ReviewerFacts(WorkItem);
#[async_trait]
impl AccessFactsPort for ReviewerFacts {
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        Ok(vec![ObjectFacts {
            resource: "work_item".into(),
            id: self.0.base.id.clone(),
            version: self.0.base.version.to_string(),
            assignee_id: FactValue::Known(self.0.owner_user_id.clone()),
            ..Default::default()
        }])
    }
}

/// 已完成任务保留原复核人事实，重放仍由 identity 检查当前角色与任务分配。
/// # 参数
/// db、identity 为原业务设施；actor、task 和 executor 为原重放读取上下文。
/// # 返回
/// 原任务指定人及同角色完整复核功能资格都成立时成功。
/// # 错误
/// 任务分配、账号或权限失效时拒绝；设施错误透传。
pub async fn require_reviewer(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse("work_item:detail")?;
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![
            Permission::parse("supplier_settlement_statement:detail")?,
            Permission::parse("supplier_settlement_statement:confirm")?,
        ],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Assignee].into(),
                policy: AccessPolicy::Assigned,
            },
            facts: Arc::new(ReviewerFacts(task.clone())),
        },
    }])?;
    Ok(AuthorizationService::new(db.clone(), identity.clone(), registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records {
                subject: AccessSubject::Direct,
                ids: ResourceTargets::new(&[task.base.id.as_str()])?,
            },
            executor,
        )
        .await?)
}
