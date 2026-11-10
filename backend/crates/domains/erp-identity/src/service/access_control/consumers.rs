//! 范围消费者准入的公开兼容导出；规则由实体层统一声明。
pub(super) use crate::entity::access_control::consumer_spec::scope_registration;
pub use crate::entity::access_control::consumer_spec::{
    configurable_registration, registration, validate_binding, validate_scope_type,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Permission;
    use crate::access_control::{DataScopeType, ScopeBinding, ScopeDimension};
    use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
    use crate::entity::access_control::consumer_spec::CONSUMERS;

    #[test]
    fn deposit_resolution_and_release_require_the_account_scope() {
        for action in ["resolve", "release"] {
            let consumer = registration("supplier_deposit", action).unwrap();
            assert_eq!(
                consumer.required_dimensions(),
                &[ScopeDimension::InternalOrg, ScopeDimension::SettlementParty]
            );
            assert!(!consumer.default_self());
            assert!(!consumer.allows_history());
        }
        assert!(registration("supplier_deposit", "force_release").is_err());
    }

    #[test]
    fn inquiry_sales_scope_and_procurement_task_grants_remain_separate() {
        let sales = configurable_registration("sales_inquiry", "detail").unwrap();
        assert!(sales.default_self());
        assert!(!sales.allows_history());
        assert_eq!(sales.required_dimensions(), &[ScopeDimension::InternalOrg]);
        for action in ["detail", "publish", "internal_quote", "convert_expenses"] {
            let task = registration("procurement_inquiry", action).unwrap();
            assert!(!task.default_self());
            assert!(!task.allows_history());
            assert!(configurable_registration("procurement_inquiry", action).is_err());
            assert!(scope_registration("procurement_inquiry", action).is_err());
        }
        assert!(registration("sales_inquiry", "internal_quote").is_err());
        assert!(registration("procurement_inquiry", "accept").is_err());
    }

    #[test]
    fn shared_businesses_are_runtime_sources_but_never_person_configuration() {
        for resource in [
            "product",
            "supplier",
            "supplier_offering",
            "purchase_order",
            "supplier_fulfillment_order",
            "supplier_settlement_statement",
        ] {
            let registration = scope_registration(resource, "list").unwrap();
            assert!(!registration.default_self());
            assert!(configurable_registration(resource, "list").is_err());
        }
        for (resource, action) in [
            ("approval_instance", "decide"),
            ("supplier_settlement_statement", "confirm"),
            ("purchase_payment_request", "pay"),
            ("invoice", "list"),
            ("product", "unknown"),
        ] {
            assert!(scope_registration(resource, action).is_err());
        }
        assert!(configurable_registration("sales_order", "list").unwrap().default_self());
        assert!(configurable_registration("stock_balance", "list").is_ok());
    }

    #[test]
    fn wired_runtime_catalog_does_not_admit_retired_scope_configuration() {
        for spec in CONSUMERS {
            let resource = spec.resource();
            for action in spec.actions().iter().map(|action| action.action()) {
                let policy = AuthorizationPolicy::for_action(resource, action).unwrap();
                let runtime = registration(resource, action).unwrap();
                assert_eq!(configurable_registration(resource, action).is_ok(), policy.configurable());
                if !policy.configurable() {
                    assert!(!runtime.default_self());
                }
            }
        }
        for (resource, action, expected) in [
            ("contract", "update", "customer:list"),
            ("sales_selection_proposal", "get", "sales_selection_booklet:get"),
        ] {
            assert!(configurable_registration(resource, action).is_err());
            let permission = Permission::parse(format!("{resource}:{action}")).unwrap();
            let source = AuthorizationPolicy::scope_source(&permission).unwrap();
            assert_eq!(source, Permission::parse(expected).unwrap());
            assert!(configurable_registration(source.resource(), source.action()).is_ok());
        }
        let permission = Permission::parse("SALES_ORDER:DETAIL").unwrap();
        assert_eq!(AuthorizationPolicy::scope_source(&permission).unwrap(), permission);
    }

    #[test]
    fn binding_checks_every_action_and_dimension_even_when_disabled() {
        let mut binding = ScopeBinding {
            schema_version: 2,
            resource: "customer".into(),
            actions: vec!["list".into()],
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: None,
            include_descendants: None,
            enabled: false,
        };
        assert!(validate_binding(&binding).is_ok());
        binding.actions.push("resume".into());
        assert!(validate_binding(&binding).is_err());
        binding.actions.pop();
        binding.target_dimension = ScopeDimension::Warehouse;
        assert!(validate_binding(&binding).is_err());
        assert!(registration("customer", "detail").unwrap().allows_history());
        assert!(!registration("customer", "update").unwrap().allows_history());
        assert!(!registration("org_unit", "list").unwrap().allows_history());
    }

    /// 维持原采购沿采购责任范围授权，不从历史参与取得写权限。
    #[test]
    fn purchase_source_maintain_uses_shared_policy_without_history() {
        let registered = registration("purchase_order", "maintain_source").unwrap();
        assert!(!registered.default_self());
        assert!(!registered.allows_history());
        assert_eq!(registered.supported_dimensions(), &[ScopeDimension::InternalOrg]);
        assert_eq!(registered.required_dimensions(), &[ScopeDimension::InternalOrg]);
        assert!(registration("sales_order", "maintain_source").is_err());
    }

    /// 合同与采购主单已接线可解析；未接线动作与工作流资源必须拒绝。
    #[test]
    fn wired_contract_and_purchase_are_admitted_unwired_actions_are_not() {
        assert!(registration("contract", "list").is_ok());
        assert!(registration("contract", "detail").is_ok());
        assert!(registration("purchase_order", "list").is_ok());
        assert!(registration("purchase_order", "detail").is_ok());
        assert!(registration("purchase_order", "create").is_ok());
        assert!(registration("purchase_order", "update").is_ok());
        assert!(registration("purchase_order", "delete").is_ok());
        assert!(registration("purchase_order", "submit").is_ok());
        assert!(registration("purchase_order", "cancel_approval").is_ok());
        assert!(registration("sales_person", "list").is_ok());
        assert!(!registration("sales_person", "list").unwrap().allows_history());
        assert!(registration("procurement_person", "list").is_ok());
        assert!(registration("sales_person", "create").is_err());
        assert!(validate_scope_type("sales_person", DataScopeType::Collaborative).is_err());
        assert!(validate_scope_type("sales_person", DataScopeType::SelfOwned).is_ok());
        assert!(validate_scope_type("customer", DataScopeType::Collaborative).is_ok());
        assert!(registration("sales_selection_booklet", "list").is_ok());
        assert!(registration("sales_selection_booklet", "create").is_ok());
        assert!(registration("sales_selection_proposal", "list").is_ok());
        assert!(registration("sales_selection_proposal", "get").is_ok());
        assert!(registration("sales_selection_proposal", "detail").is_err());
        assert!(registration("purchase_change_order", "list").is_err());
        assert!(registration("purchase_return_order", "list").is_err());
        assert!(registration("contract", "delete").is_err());
        assert!(registration("purchase_order", "transfer").is_err());
        assert!(registration("work_item", "list").is_ok());
        assert!(registration("work_item", "detail").is_ok());
        assert!(registration("approval_instance", "decide").is_ok());
        assert!(registration("work_item", "manage").is_ok());
    }

    /// S3-03 资金资源已接线可解析；写动作与历史参与均不在本次接线内。
    #[test]
    fn wired_funds_resources_admit_reads_without_history_or_commands() {
        for resource in [
            "receivable_account",
            "customer_receipt",
            "invoice",
            "sales_invoice_request",
            "payable_account",
            "supplier_payment",
        ] {
            let list = registration(resource, "list").unwrap();
            assert!(!list.allows_history());
            let detail = registration(resource, "detail").unwrap();
            assert!(!detail.allows_history());
            assert!(registration(resource, "submit").is_err());
            assert!(registration(resource, "commit").is_err());
        }
        let allocation = registration("purchase_invoice_allocation", "list").unwrap();
        assert!(!allocation.allows_history());
        assert!(registration("purchase_invoice_allocation", "detail").is_err());
    }

    /// S3-08 新增消费动作已接线可解析；未接线动作与资源必须失败关闭。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；接线与关闭均符合登记时通过。
    ///
    /// # 关键业务约束
    /// 不得以初始化清单代替本登记；历史参与仅四类读取动作。
    #[test]
    fn wired_s3_actions_admit_reads_unwired_actions_fail_closed() {
        for action in ["list", "get", "create", "maintain", "prepare", "publish", "copy_link"] {
            assert!(registration("sales_selection_booklet", action).is_ok());
        }
        for action in ["list", "get"] {
            assert!(registration("sales_selection_proposal", action).is_ok());
            assert!(!registration("sales_selection_proposal", action).unwrap().allows_history());
        }
        assert!(registration("sales_selection_proposal", "create").is_err());
        assert!(registration("sales_selection_booklet", "approve").is_err());
        for action in ["list", "detail"] {
            assert!(registration("cost_entry", action).is_ok());
            assert!(!registration("cost_entry", action).unwrap().allows_history());
        }
        assert!(registration("cost_allocation", "list").is_ok());
        assert!(registration("cost_allocation", "detail").is_err());
        assert!(registration("fulfillment_queue", "list").is_err());
        assert!(registration("customer_quality", "list").is_err());
        assert!(registration("handover", "update").is_err());
        for action in ["list", "detail", "create", "update", "delete"] {
            let consumer = registration("supplier", action).unwrap();
            assert!(!consumer.allows_history());
        }
        assert!(registration("supplier", "submit").is_err());
    }

    #[test]
    fn wired_integration_handlers_reject_history_writes() {
        for action in ["list", "detail", "create"] {
            let consumer = registration("integration_error_task", action).unwrap();
            assert!(!consumer.allows_history());
            assert_eq!(consumer.required_dimensions(), &[ScopeDimension::InternalOrg]);
        }
        for action in ["list", "detail", "create", "decide"] {
            let consumer = registration("reconciliation_difference", action).unwrap();
            assert!(!consumer.allows_history());
        }
        assert!(registration("integration_error_task", "decide").is_err());
        assert!(registration("reconciliation_difference", "update").is_err());
    }

    #[test]
    fn wired_supplier_offering_admits_list_and_writes_without_history() {
        for action in ["list", "create", "update"] {
            let consumer = registration("supplier_offering", action).unwrap();
            assert!(!consumer.allows_history());
            assert_eq!(consumer.required_dimensions(), &[ScopeDimension::InternalOrg]);
        }
        assert!(registration("supplier_offering", "detail").is_err());
        assert!(registration("supplier_offering", "delete").is_err());
    }
    #[test]
    fn settlement_directory_does_not_accept_person_scope_configuration() {
        assert!(registration("settlement_party", "list").is_err());
        assert!(configurable_registration("settlement_party", "list").is_err());
        assert!(configurable_registration("warehouse", "list").is_ok());
    }

    #[test]
    fn defaults_only_apply_to_registered_business_ownership_dimensions() {
        for spec in CONSUMERS {
            let resource = spec.resource();
            for action in spec.actions().iter().map(|action| action.action()) {
                let consumer = registration(resource, action).unwrap();
                if consumer.default_self() {
                    assert_eq!(consumer.required_dimensions(), &[ScopeDimension::InternalOrg]);
                }
            }
        }
        for (resource, action) in [
            ("approval_instance", "decide"),
            ("work_item", "manage"),
            ("org_unit", "list"),
            ("person_query_qualification", "manage"),
            ("warehouse", "list"),
            ("sales_person", "list"),
            ("procurement_person", "list"),
            ("business_person", "list"),
        ] {
            assert!(!registration(resource, action).unwrap().default_self());
        }
        assert!(!registration("product", "list").unwrap().default_self());
        assert!(!registration("supplier_payment", "detail").unwrap().default_self());
        assert!(registration("integration_error_task", "list").unwrap().default_self());
    }
    /// 工作台默认本人只应用于已登记读取动作。
    #[test]
    fn task_scope_defaults_are_action_specific() {
        for action in ["list", "detail"] {
            let scope = configurable_registration("work_item", action).unwrap();
            assert!(scope.default_self());
            assert!(!scope.allows_history());
            assert_eq!(scope.required_dimensions(), &[ScopeDimension::InternalOrg]);
        }
        assert!(!configurable_registration("work_item", "manage").unwrap().default_self());
        assert!(registration("work_item", "reassign").is_err());
    }
}
