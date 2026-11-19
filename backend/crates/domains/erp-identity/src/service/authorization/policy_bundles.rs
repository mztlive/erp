//! 授权文件的固定完整资格及文档附加要求，仅供 identity 内部文件用例使用。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{access_control, admin, organization};
use persistence_core::Executor;

use super::{AuthorizationService, SnapshotScope, operations};
use crate::entity::authorization::AccessContext;
use crate::entity::authorization_bundle::PolicyDocument;
use crate::{Error, Permission, Result};

impl SnapshotScope {
    /// 授权文件交付前核对原快照及范围期限，保留文件入口的错误信息。
    pub(crate) fn ensure_policy_bundle_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.check(executor, "授权文件读取跨越执行阶段", "DATA_SCOPE_CHANGED：授权文件读取已到期")
    }
}

/// 按文件动作列出完整资格；预览和应用共用基础权限，再按文档实际涉及的角色、绑定和人员范围增加写资格。
pub(super) fn requirements(
    permission: &Permission,
    document: Option<&PolicyDocument>,
) -> Option<Vec<Permission>> {
    let mut required = if permission == &access_control::POLICY_BUNDLE_EXPORT {
        vec![access_control::POLICY_BUNDLE_EXPORT, organization::LIST]
    } else if [access_control::POLICY_BUNDLE_PREVIEW, access_control::POLICY_BUNDLE_APPLY]
        .contains(permission)
    {
        vec![access_control::POLICY_BUNDLE_PREVIEW, access_control::POLICY_BUNDLE_APPLY, organization::CHANGE]
    } else {
        return None;
    };
    required.extend([admin::ACCOUNT_LIST_ADMINS, admin::ROLE_LIST_ROLES, access_control::PERSON_SCOPE_LIST]);
    if let Some(document) = document {
        if !document.roles.is_empty() {
            required.extend([admin::ROLE_CREATE_ROLE, admin::ROLE_UPDATE_ROLE]);
        }
        if !document.bindings.is_empty() {
            required.push(admin::ACCOUNT_UPDATE_ADMIN);
        }
        if !document.data_scopes.is_empty() {
            required.push(access_control::PERSON_SCOPE_SAVE);
        }
    }
    Some(required)
}

impl AuthorizationService {
    /// 在原快照检查固定文件入口、同角色完整资格和公司组织范围，返回文件计划使用的策略与组织事实。
    ///
    /// 导出不带文档，预览和应用传规范化文档。入口或目标错配、非快照、资格不足、范围收窄、阶段变化或到期均拒绝。
    pub(crate) async fn require_policy_bundle(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        document: Option<&PolicyDocument>,
        executor: &mut dyn Executor,
    ) -> Result<SnapshotScope> {
        executor.next_read();
        let required = requirements(permission, document)
            .ok_or_else(|| Error::ValidationError("未知授权文件入口".into()))?;
        if (permission == &access_control::POLICY_BUNDLE_EXPORT) != document.is_none() {
            return Err(Error::ValidationError("授权文件入口与文档目标错配".into()));
        }

        let boundary = executor
            .boundary()
            .filter(|boundary| boundary.is_snapshot())
            .ok_or_else(|| Error::ValidationError("授权文件必须使用快照事务".into()))?;
        let qualified = self
            .permissions(actor, &required, executor)
            .await?
            .filter(|qualified| qualified.same_role(&required))
            .ok_or_else(|| Error::Forbidden("账号缺少授权文件的同角色完整资格".into()))?;

        let source = operations::source(permission).expect("固定文件入口具有组织范围来源");
        // 预览和应用必须沿用同一个组织目标，否则同一文件的审核摘要无法在应用时复验。
        let context = AccessContext {
            actor: actor.clone(),
            permission: source.clone(),
            resource: source.resource().into(),
            model: None,
            as_of: Instant::now(),
        };
        let access = self.company_operation_scope(&context, &source, &required, &qualified, executor).await?;

        if !boundary.is_current(executor) {
            return Err(Error::ValidationError("授权文件检查跨越执行阶段".into()));
        }
        let result = SnapshotScope::new(access, boundary);
        result.ensure_policy_bundle_current(executor)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::access_control::ResolvedScope;
    use crate::entity::organization_change::OrganizationState;
    use crate::service::access_control::resolve::AuthorizedDataScope;

    // 验证文件凭据在范围到期、新读取阶段或事务重启后均不能交付。
    #[test]
    fn file_delivery_rejects_expiry_new_read_phases_and_restarted_transactions() {
        let mut executor = SnapshotExecutor::default();
        let scope = AuthorizedDataScope {
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "1".into(),
            as_of: Instant::from_unix_secs(1),
        };
        let mut access = SnapshotScope::new(scope, executor.boundary().unwrap());
        access.ensure_policy_bundle_current(&mut executor).unwrap();

        access.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(access.ensure_policy_bundle_current(&mut executor), Err(Error::ConflictError(_))));

        access.valid_until = None;
        executor.next_read();
        assert!(matches!(access.ensure_policy_bundle_current(&mut executor), Err(Error::ValidationError(_))));

        access.boundary = executor.boundary().unwrap();
        access.ensure_policy_bundle_current(&mut executor).unwrap();
        executor.restart();
        assert!(matches!(access.ensure_policy_bundle_current(&mut executor), Err(Error::ValidationError(_))));
    }
}
