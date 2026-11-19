//! 客户质量当前态：销售当前范围与真实客户当前范围取交集。
use std::collections::BTreeSet;

use erp_permissions::{customer, internal};

use super::ScopeLookup;
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessRule, FactKind, RelationKey, RelationMatch};
use crate::{Error, Permission, QualifiedPermissions, Result};

// 为该类质量查询独立取证客户列表资格；它不要求和销售列表资格出自同一启用角色。
pub(crate) fn qualifications(permission: &Permission) -> Vec<Permission> {
    if current(permission) { vec![customer::LIST] } else { vec![] }
}

// 解析质量查询涉及的客户范围时只选择授予客户入口的角色，不沿用销售列表的合格角色。
pub(crate) fn scope_requests(permission: &Permission) -> Vec<(Permission, Permission)> {
    if current(permission) { vec![(customer::LIST, customer::LIST)] } else { vec![] }
}

// 在销售基础数据条件上追加真实客户来源的完整覆盖要求；客户必须落入本次独立客户范围。
pub(in crate::service::authorization) fn condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    condition_with(permission, Some(qualified), scopes, base)
}

// 启动时核对质量查询对销售根模型和真实客户关联所需的全部事实能力。
pub(super) fn binding_condition(permission: &Permission, base: AccessRule) -> Result<AccessRule> {
    condition_with(permission, None, &ScopeLookup::Capability, base)
}

fn condition_with(
    permission: &Permission,
    qualified: Option<&QualifiedPermissions>,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    if !current(permission) {
        return Ok(base);
    }
    if qualified.is_some_and(|qualified| !qualified.same_role(&[customer::LIST])) {
        return Err(Error::Forbidden("客户质量查询缺少客户列表资格".into()));
    }
    let scope = scopes.get(&customer::LIST, &customer::LIST, || {
        Error::ValidationError("客户质量查询缺少独立客户范围".into())
    })?;
    Ok(AccessRule::all(vec![base, customer_condition(&scope)?]))
}

fn current(permission: &Permission) -> bool {
    permission == &internal::CUSTOMER_QUALITY_CURRENT
}

fn customer_condition(scope: &ResolvedScope) -> Result<AccessRule> {
    let facts = BTreeSet::from([FactKind::Owner, FactKind::Organization]);
    Ok(AccessRule::Independent {
        permission: customer::LIST,
        rule: Box::new(AccessRule::Related {
            relation: RelationKey::parse("customer")?,
            resource: "customer".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(AccessRule::scope(scope, false).requiring(&facts)),
        }),
    })
}

#[cfg(test)]
mod tests {
    use erp_permissions::sales_order;

    use super::*;
    use crate::entity::access_control::ScopeClause;
    use crate::entity::authorization::{FactValue, ObjectFacts, RelatedObjects};

    fn scope(company: bool) -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { company, self_owned: !company, ..Default::default() }],
            user_limit: None,
        }
    }

    fn customer(owner: &str, history: bool) -> ObjectFacts {
        ObjectFacts {
            resource: "customer".into(),
            id: "customer".into(),
            version: "1".into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(None),
            history_participants: FactValue::Known(if history {
                ["actor".into()].into()
            } else {
                BTreeSet::new()
            }),
            ..Default::default()
        }
    }

    fn order(owner: &str, child: ObjectFacts, history: bool) -> ObjectFacts {
        ObjectFacts {
            resource: "sales_order".into(),
            id: "order".into(),
            version: "1".into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(None),
            history_participants: FactValue::Known(if history {
                ["actor".into()].into()
            } else {
                BTreeSet::new()
            }),
            relations: [(
                RelationKey::parse("customer").unwrap(),
                RelatedObjects { expected_ids: [child.id.clone()].into(), objects: vec![child] },
            )]
            .into(),
            ..Default::default()
        }
    }

    fn rule(sales_company: bool, customer_company: bool) -> AccessRule {
        let qualified = QualifiedPermissions::test_grants(&[
            ("sales", &[sales_order::LIST]),
            ("customer", &[customer::LIST]),
        ]);
        let scopes = [(customer::LIST, customer::LIST, scope(customer_company))];
        condition(
            &internal::CUSTOMER_QUALITY_CURRENT,
            &qualified,
            &ScopeLookup::Resolved(&scopes),
            AccessRule::scope(&scope(sales_company), false),
        )
        .unwrap()
    }

    #[test]
    fn binding_matches_runtime_with_capability_scope() {
        let permission = internal::CUSTOMER_QUALITY_CURRENT;
        let qualified = QualifiedPermissions::test_grants(&[("customer", &[customer::LIST])]);
        let base = AccessRule::scope(&scope(false), false);
        let scope = super::super::capability_scope(&customer::LIST).unwrap();
        let scopes = [(customer::LIST, customer::LIST, scope)];
        assert_eq!(
            binding_condition(&permission, base.clone()).unwrap(),
            condition(&permission, &qualified, &ScopeLookup::Resolved(&scopes), base).unwrap()
        );
    }

    #[test]
    fn both_scopes_apply_and_company_customer_range_does_not_add_an_owner_restriction() {
        let rule = rule(false, true);
        assert!(
            rule.allows_object("actor", &order("actor", customer("other", false), false), &BTreeSet::new())
                .unwrap()
        );
        assert!(
            !rule
                .allows_object("actor", &order("other", customer("actor", false), false), &BTreeSet::new())
                .unwrap()
        );

        let rule = self::rule(true, false);
        assert!(
            rule.allows_object("actor", &order("other", customer("actor", false), false), &BTreeSet::new())
                .unwrap()
        );
        assert!(
            !rule
                .allows_object("actor", &order("actor", customer("other", false), false), &BTreeSet::new())
                .unwrap()
        );
    }

    #[test]
    fn historical_participation_does_not_supply_current_sales_or_current_customer_access() {
        let rule = rule(false, false);
        assert!(
            !rule
                .allows_object("actor", &order("other", customer("actor", false), true), &BTreeSet::new())
                .unwrap()
        );
        assert!(
            !rule
                .allows_object("actor", &order("actor", customer("other", true), false), &BTreeSet::new())
                .unwrap()
        );
    }

    #[test]
    fn missing_customer_qualification_scope_or_real_source_fails_closed() {
        let qualified = QualifiedPermissions::test_grants(&[("sales", &[sales_order::LIST])]);
        assert!(matches!(
            condition(
                &internal::CUSTOMER_QUALITY_CURRENT,
                &qualified,
                &ScopeLookup::Resolved(&[]),
                AccessRule::Allow
            ),
            Err(Error::Forbidden(_))
        ));

        let qualified = QualifiedPermissions::test_grants(&[("customers", &[customer::LIST])]);
        assert!(matches!(
            condition(
                &internal::CUSTOMER_QUALITY_CURRENT,
                &qualified,
                &ScopeLookup::Resolved(&[]),
                AccessRule::Allow
            ),
            Err(Error::ValidationError(_))
        ));

        let rule = rule(true, true);
        let mut root = order("actor", customer("other", false), false);
        root.relations.values_mut().next().unwrap().objects.clear();
        assert!(!rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
        root.relations.clear();
        assert!(rule.allows_object("actor", &root, &BTreeSet::new()).is_err());
    }
}
