//! 原经办人作为完整所有者事实交给 identity 判断。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactKind, ObjectFacts};
use erp_identity::ports::AccessFactsPort;
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget, AuthorizationRegistry,
    AuthorizationService, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

struct LoadedOwnerFacts(ObjectFacts);

#[async_trait]
impl AccessFactsPort for LoadedOwnerFacts {
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        Ok(vec![self.0.clone()])
    }
}

/// 在原事务内按退款或冲正单的真实经办人事实证明提交资格。
/// # 参数
/// db、rbac 为身份设施，actor 为当前账号，facts 为本事务读取的完整所有者事实。
/// executor 为原业务事务。
/// # 返回
/// 功能权限和原经办人身份均成立时成功。
/// # 错误
/// 账号、功能权限、所有者事实或对象身份不满足时返回 identity 原错误。
pub(super) async fn require_owner(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    facts: ObjectFacts,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse(format!("{}:submit", facts.resource))?;
    let ids = ResourceTargets::new(&[facts.id.as_str()])?;
    let registration = AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner].into(),
                policy: AccessPolicy::Owned,
            },
            facts: Arc::new(LoadedOwnerFacts(facts)),
        },
    };
    AuthorizationService::new(db, rbac, AuthorizationRegistry::new(vec![registration])?)
        .require(actor, &permission, AccessTarget::Records { subject: AccessSubject::Direct, ids }, executor)
        .await?;
    Ok(())
}
