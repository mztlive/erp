//! 授权文件的固定完整资格及文档附加要求，仅供 identity 内部文件用例使用。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{access_control, admin, organization};
use persistence_core::{ExecutionBoundary, Executor};

use super::resolve::Resolution;
use super::{AuthorizationService, operations};
use crate::entity::authorization::AccessContext;
use crate::entity::authorization_bundle::PolicyDocument;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, Result};

/// 文件读取保留原范围、事务阶段和到期边界，交付检查不重读同一快照。
pub(crate) struct PolicyBundleAccess {
    pub(crate) scope: AuthorizedDataScope,
    boundary: ExecutionBoundary,
    valid_until: Option<Instant>,
}

impl PolicyBundleAccess {
    /// 核对文件读取仍属于授权时的原快照、阶段和有效期。
    /// # 参数
    /// executor 为文件用例的原执行器。
    /// # 返回
    /// 全部边界仍有效时成功。
    /// # 错误
    /// 事务、读取阶段或期限失效时拒绝交付。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        if !self.boundary.is_current(executor) {
            return Err(Error::ValidationError("授权文件读取跨越执行阶段".into()));
        }
        if self.valid_until.is_some_and(|until| until <= Instant::now()) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权文件读取已到期".into()));
        }
        Ok(())
    }
}

/// 文件预览和应用使用相同基础资格，实际文档只能追加必要写配置能力。
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
    /// 在原快照一次证明文件所需的同角色资格和公司组织边界。
    /// # 参数
    /// actor、permission 为真实账号及固定文件入口；document 为预览或应用的规范化文件。
    /// executor 必须属于文件用例的原快照；导出传入 None。
    /// # 返回
    /// 已授权的策略版本及组织事实，供文件计划校验使用。
    /// # 错误
    /// 入口错配、非快照、完整资格不足、范围收窄或阶段到期时拒绝。
    pub(crate) async fn require_policy_bundle(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        document: Option<&PolicyDocument>,
        executor: &mut dyn Executor,
    ) -> Result<PolicyBundleAccess> {
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
        let mut resolution = Resolution::default();
        resolution.include(&access);
        let result = PolicyBundleAccess { scope: access, boundary, valid_until: resolution.valid_until };
        result.ensure_current(executor)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::access_control::ResolvedScope;
    use crate::entity::organization_change::OrganizationState;
    use crate::service::access_control::resolve::DataScopeFacts;

    #[test]
    fn file_delivery_rejects_expiry_new_read_phases_and_restarted_transactions() {
        let mut executor = SnapshotExecutor::default();
        let scope = AuthorizedDataScope::test_fixture(DataScopeFacts {
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "1".into(),
            as_of: Instant::from_unix_secs(1),
        });
        let mut access =
            PolicyBundleAccess { scope, boundary: executor.boundary().unwrap(), valid_until: None };
        access.ensure_current(&mut executor).unwrap();

        access.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(access.ensure_current(&mut executor), Err(Error::ConflictError(_))));

        access.valid_until = None;
        executor.next_read();
        assert!(matches!(access.ensure_current(&mut executor), Err(Error::ValidationError(_))));

        access.boundary = executor.boundary().unwrap();
        access.ensure_current(&mut executor).unwrap();
        executor.restart();
        assert!(matches!(access.ensure_current(&mut executor), Err(Error::ValidationError(_))));
    }
}
