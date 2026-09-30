//! 演示准备流程显式初始化人员范围，不参与常规角色或账号保存。
use std::collections::HashMap;

use application_core::AuditActor;
use erp_identity::access_control::DataScopeType;
#[cfg(test)]
use erp_identity::access_control::{ScopeDimension, ScopeTargetMode};
use erp_identity::dto::person_scope::{PersonScopeGrant, SavePersonScopeRequest};

use super::DemoMasterDataService;
use super::spec::DemoPersonScope;
use crate::Result;
use crate::adapters::scope_configuration;

impl DemoMasterDataService {
    /// 对明确演示人员补齐尚未配置的操作，保留已有配置。
    /// # 参数
    /// 操作人及演示账号登录名到身份映射。
    /// # 返回
    /// 全部人员初次配置完成。
    /// # 错误
    /// 资格、范围或版本失败立即停止，不隐藏错误。
    pub(super) async fn ensure_person_scopes(
        &self,
        actor: &AuditActor,
        accounts: &HashMap<String, String>,
    ) -> Result<()> {
        let service = scope_configuration(self.db.clone(), self.rbac.clone());
        for (login, user) in accounts {
            let mut view = service.person_scopes(user, actor).await?;
            for business in std::mem::take(&mut view.businesses) {
                let actions = business
                    .actions
                    .iter()
                    .filter(|action| {
                        !view
                            .items
                            .iter()
                            .any(|scope| scope.resource == business.resource && scope.action == **action)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if actions.is_empty() {
                    continue;
                }
                let term = DemoPersonScope::term(login, &business.dimensions);
                if business.default_self && term.scope_type == DataScopeType::SelfOwned {
                    continue;
                }
                service
                    .save_person_scope(
                        user,
                        SavePersonScopeRequest {
                            resource: business.resource,
                            actions: actions.clone(),
                            grants: vec![PersonScopeGrant { actions, terms: vec![term] }],
                            replace_legacy: false,
                            expected_policy_version: view.policy_version,
                        },
                        actor,
                    )
                    .await?;
                view = service.person_scopes(user, actor).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn procurement_product_starts_at_self_without_role_scope() {
        assert_eq!(
            DemoPersonScope::term("caigou", &[ScopeDimension::InternalOrg]).scope_type,
            DataScopeType::SelfOwned
        );
        assert_eq!(
            DemoPersonScope::term("lisiyong", &[ScopeDimension::InternalOrg]).target_mode,
            Some(ScopeTargetMode::OwnOrg)
        );
        assert_eq!(
            DemoPersonScope::term("cangchu", &[ScopeDimension::Warehouse]).scope_type,
            DataScopeType::Company
        );
    }
}
