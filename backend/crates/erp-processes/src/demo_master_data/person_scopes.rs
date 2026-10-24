//! 演示准备流程显式初始化人员范围，不参与常规角色或账号保存。
use std::collections::HashMap;

use application_core::AuditActor;
#[cfg(test)]
use erp_identity::access_control::DataScopeType;
#[cfg(test)]
use erp_identity::access_control::{ScopeDimension, ScopeTargetMode};

use super::DemoMasterDataProcess;
use super::spec::DemoPersonScope;
use crate::Result;
use crate::adapters::scope_configuration;

impl DemoMasterDataProcess {
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
                let Some(request) =
                    DemoPersonScope::request(login, &business, &view.items, view.policy_version)?
                else {
                    continue;
                };
                service.save_person_scope(user, request, actor).await?;
                view = service.person_scopes(user, actor).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_identity::dto::person_scope::PersonBusinessOption;
    use erp_identity::entity::access_control::person_scope::PersonDataScope;

    use super::*;

    #[test]
    fn task_inherited_and_shared_resources_never_get_independent_scopes() {
        for (resource, action) in [
            ("approval_instance", "decide"),
            ("contract", "list"),
            ("customer_receipt", "list"),
            ("product", "list"),
            ("supplier", "update"),
            ("purchase_order", "create"),
            ("supplier_offering", "list"),
            ("supplier_settlement_statement", "list"),
        ] {
            let business = PersonBusinessOption::from_granted(resource, vec![action.into()], &[]).unwrap();
            assert!(DemoPersonScope::request("fukuan", &business, &[], 7).unwrap().is_none());
        }
    }

    #[test]
    fn explicit_cross_department_reads_and_directories_have_valid_initial_scopes() {
        for (login, resource) in [("caigou", "sales_order"), ("xiaoshou", "business_person")] {
            let business = PersonBusinessOption::from_granted(
                resource,
                vec!["list".into()],
                &[ScopeDimension::InternalOrg],
            )
            .unwrap();
            let request = DemoPersonScope::request(login, &business, &[], 7).unwrap().unwrap();
            assert_eq!(request.grants[0].terms[0].scope_type, DataScopeType::Company);
            assert_eq!(request.expected_policy_version, 7);
            assert!(!request.replace_legacy);
        }
    }

    #[test]
    fn existing_sales_revocation_is_preserved_and_shared_actions_are_excluded() {
        let business = PersonBusinessOption::from_granted(
            "sales_order",
            vec!["list".into(), "detail".into()],
            &[ScopeDimension::InternalOrg],
        )
        .unwrap();
        let mut existing = PersonDataScope::default_for("finance", "sales_order", "list");
        existing.expression.alternatives.clear();
        let request = DemoPersonScope::request("caiwu", &business, &[existing], 3).unwrap().unwrap();
        assert_eq!(request.actions, ["detail"]);
        assert!(DemoPersonScope::term("caigou", "product", true, &[]).is_err());
        let product = PersonBusinessOption::from_granted(
            "product",
            vec!["list".into()],
            &[ScopeDimension::InternalOrg],
        )
        .unwrap();
        assert!(DemoPersonScope::request("caigou", &product, &[], 3).unwrap().is_none());
    }
    #[test]
    fn sales_starts_at_self_and_inventory_keeps_warehouse_scope() {
        assert_eq!(
            DemoPersonScope::term("xiaoshou", "sales_order", true, &[ScopeDimension::InternalOrg])
                .unwrap()
                .scope_type,
            DataScopeType::SelfOwned
        );
        assert_eq!(
            DemoPersonScope::term("lisiyong", "sales_order", true, &[ScopeDimension::InternalOrg])
                .unwrap()
                .target_mode,
            Some(ScopeTargetMode::OwnOrg)
        );
        assert_eq!(
            DemoPersonScope::term("cangchu", "stock_balance", false, &[ScopeDimension::Warehouse])
                .unwrap()
                .scope_type,
            DataScopeType::Company
        );
    }
}
