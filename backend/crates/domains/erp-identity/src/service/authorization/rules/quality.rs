//! 客户质量当前态：销售当前范围与真实客户当前范围取交集。
use std::collections::BTreeSet;

use erp_permissions::{customer, internal};

use super::ScopeLookup;
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessRule, FactKind, RelationKey, RelationMatch};
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 为客户质量当前态独立取证客户列表权限，不要求与销售列表出自同一启用角色。
pub(crate) fn qualifications(permission: &Permission) -> Vec<Permission> {
    if current(permission) { vec![customer::LIST] } else { vec![] }
}

/// 为质量当前态请求独立客户列表范围，只选授予该入口的角色，不借销售角色。
pub(crate) fn scope_requests(permission: &Permission) -> Vec<(Permission, Permission)> {
    if current(permission) { vec![(customer::LIST, customer::LIST)] } else { vec![] }
}

/// 运行时把销售基础条件与全部真实客户的当前范围取交集。
pub(in crate::service::authorization) fn condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    condition_with(permission, Some(qualified), scopes, base)
}

/// 启动时展开质量当前态的客户能力条件，校验销售根和客户来源所需事实。
pub(super) fn binding_condition(permission: &Permission, base: AccessRule) -> Result<AccessRule> {
    condition_with(permission, None, &ScopeLookup::Capability, base)
}

// 为质量当前态叠加客户独立资格及范围；启动可不检查具体账号，其他动作保留原销售条件。
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

// 识别必须使用当前销售和客户范围的质量入口，不混入历史质量查询。
fn current(permission: &Permission) -> bool {
    permission == &internal::CUSTOMER_QUALITY_CURRENT
}

// 构造独立客户列表条件，要求真实客户引用非空、全部命中范围且负责人和组织事实齐备，不接纳历史参与。
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

    // 生成公司或本人范围样本，供分别限制销售与客户。
    fn scope(company: bool) -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { company, self_owned: !company, ..Default::default() }],
            user_limit: None,
        }
    }

    // 构造指定负责人及历史参与状态的真实客户事实。
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

    // 构造归属和参与状态可控、带真实客户引用的销售单事实。
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

    // 用分属不同角色的销售与客户权限构造当前质量条件，支持分别设定两侧范围。
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

    // 验证质量当前态在能力范围下启动绑定与运行时规则一致。
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

    // 验证销售和客户范围均须命中，客户公司范围不额外要求本人归属。
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

    // 验证历史参与不能替代销售侧或客户侧的当前范围。
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

    // 验证缺少客户列表资格、独立范围或真实客户来源均不能放行。
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
