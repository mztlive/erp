//! 授权 BSON 的受限内存解释器；不替代真实 MongoDB 执行等价性验收。
use test_support::matches_filter as matches;

#[test]
fn public_sales_decision_matches_database_conditions() {
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use erp_identity::access_control::ResolvedScope;
    use erp_identity::access_control::ScopeClause;
    use erp_identity::service::access_control::resolve::AuthorizedDataScope;
    use erp_read_models::sales_center::access::{sales_scope, SalesAccess};
    use erp_sales::entity::sales_order::SalesOrder;
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrderData};
    use serde_json::json;
    use std::collections::BTreeSet;

    let clause = |mask| ScopeClause {
        company: mask & 1 != 0,
        self_owned: mask & 2 != 0,
        collaborative: mask & 4 != 0,
        org_unit_ids: if mask & 8 != 0 {
            BTreeSet::from(["org-a".into()])
        } else {
            BTreeSet::new()
        },
        ..Default::default()
    };
    let mut order = SalesOrder::new(
        SalesOrderId::new("order"),
        SalesOrderData {
            sales_owner_user_id: "actor".into(),
            business_org_unit_id: "org-a".into(),
            order_no: "SO-test".into(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("customer"),
            contract_id: None,
            settlement_party_id: PartyId::new("party"),
            source_status_code: None,
        },
        "creator",
    )
    .unwrap();
    for action in ["detail", "create", "update"] {
        for role in 0..16 {
            for second in [0, 2, 8] {
                for limit in -1..16 {
                    let access = AuthorizedDataScope {
                        user_id: "actor".into(),
                        resource: "sales_order".into(),
                        action: action.into(),
                        role_scopes: Default::default(),
                        scope: ResolvedScope {
                            role_clauses: vec![clause(role), clause(second)],
                            user_limit: (limit >= 0).then(|| clause(limit)),
                        },
                        organizations: Default::default(),
                        policy_version: 1,
                        scope_version: "v1".into(),
                        as_of: Instant::from_unix_secs(0),
                    };
                    for bits in 0..16 {
                        order.sales_owner_user_id = if bits & 1 != 0 { "actor" } else { "other" }.into();
                        order.business_org_unit_id = if bits & 4 != 0 { "org-a" } else { "org-b" }.into();
                        let collaborators = if bits & 2 != 0 {
                            vec!["customer".into()]
                        } else {
                            vec![]
                        };
                        let history = if action == "detail" && bits & 8 != 0 {
                            vec!["order".into()]
                        } else {
                            vec![]
                        };
                        let scope = sales_scope(&access, "actor", &collaborators, history);
                        let document = json!({ "id": "order", "sales_owner_user_id": &order.sales_owner_user_id,
                        "business_org_unit_id": &order.business_org_unit_id, "customer_id": "customer" });
                        assert_eq!(
                            SalesAccess::allows(&access, &scope, &order).unwrap(),
                            matches(&scope.document(), &document)
                        );
                    }
                }
            }
        }
    }
}
