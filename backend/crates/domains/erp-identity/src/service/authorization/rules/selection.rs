//! 选品入口的客户可见性与独立读取前提；所有业务入口共用固定组合。
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::slice;

use erp_permissions::{customer, sales_selection as selection};

use super::ScopeLookup;
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessFact, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::{Error, Permission, QualifiedPermissions, Result};

// 选品授权同时取证登记的附加读取权限，各入口可以由不同启用角色提供。
pub(crate) fn qualifications(permission: &Permission) -> Vec<Permission> {
    let mut requested = Vec::new();
    if is_list(permission) {
        requested.extend([customer::LIST, customer::ALL_AUTHORIZED_LIST]);
    } else if is_selection(permission) {
        requested.push(customer::DETAIL);
        if requires_read(permission) {
            requested.push(selection::ADMIN_IMAGE);
        }
    }
    requested
}

// 把客户和详情的范围分别绑定到自身入口，解析时不借用选品资格的角色集合。
pub(crate) fn scope_requests(permission: &Permission) -> Vec<(Permission, Permission)> {
    if is_list(permission) {
        return vec![(customer::LIST, customer::LIST)];
    }
    let mut requested = Vec::new();
    if is_selection(permission) {
        requested.push((customer::DETAIL, customer::DETAIL));
        if requires_read(permission) {
            requested.push((selection::ADMIN_IMAGE, selection::ADMIN_IMAGE));
        }
    }
    requested
}

// 选品根范围须与固定客户或详情前提同时成立；缺少任一入口资格或范围时不放行。
pub(in crate::service::authorization) fn condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    condition_with(permission, Some(qualified), scopes, base)
}

// 校验选品接入时保留客户本人条件，覆盖可选资格未授予时所需的事实能力。
pub(super) fn binding_condition(permission: &Permission, base: AccessRule) -> Result<AccessRule> {
    condition_with(permission, None, &ScopeLookup::Capability, base)
}

fn condition_with(
    permission: &Permission,
    qualified: Option<&QualifiedPermissions>,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    if is_list(permission) {
        require_qualification(qualified, &customer::LIST)?;
        let scope = find_scope(scopes, &customer::LIST)?;
        let ownership =
            if qualified.is_some_and(|qualified| qualified.same_role(&[customer::ALL_AUTHORIZED_LIST])) {
                AccessRule::Allow
            } else {
                AccessRule::Fact(AccessFact::Owned)
            };
        return Ok(AccessRule::all(vec![base, customer_condition(&scope, ownership)?]));
    }

    if !is_selection(permission) {
        return Ok(base);
    }
    require_qualification(qualified, &customer::DETAIL)?;
    let customer_scope = find_scope(scopes, &customer::DETAIL)?;
    let mut conditions = vec![base, customer_detail_condition(&customer_scope)?];
    if requires_read(permission) {
        require_qualification(qualified, &selection::ADMIN_IMAGE)?;
        let scope = find_scope(scopes, &selection::ADMIN_IMAGE)?;
        conditions.push(read_condition(&scope));
    }
    Ok(AccessRule::all(conditions))
}

fn is_selection(permission: &Permission) -> bool {
    matches!(permission.resource(), "sales_selection_booklet" | "sales_selection_proposal")
}
fn is_list(permission: &Permission) -> bool {
    is_selection(permission) && permission.action() == "list"
}

fn requires_read(permission: &Permission) -> bool {
    permission.resource() == "sales_selection_booklet"
        && !matches!(permission.action(), "list" | "get" | "create")
}

fn require_qualification(qualified: Option<&QualifiedPermissions>, permission: &Permission) -> Result<()> {
    if qualified.is_none_or(|qualified| qualified.same_role(slice::from_ref(permission))) {
        Ok(())
    } else {
        tracing::debug!(%permission, "缺少选品入口所需的独立读取资格");
        Err(Error::Forbidden("当前账号没有查看选品所需资料的权限，请联系权限管理员".into()))
    }
}

fn find_scope<'a>(scopes: &ScopeLookup<'a>, permission: &Permission) -> Result<Cow<'a, ResolvedScope>> {
    scopes.get(permission, permission, || {
        tracing::debug!(%permission, "选品缺少独立范围");
        Error::ValidationError("当前账号尚未配置选品所需的数据范围，请联系权限管理员".into())
    })
}

fn customer_condition(scope: &ResolvedScope, ownership: AccessRule) -> Result<AccessRule> {
    let facts = BTreeSet::from([FactKind::Owner, FactKind::Organization]);
    let rule = AccessRule::all(vec![AccessRule::scope(scope, false), ownership]).requiring(&facts);
    Ok(AccessRule::Independent {
        permission: customer::LIST,
        rule: Box::new(AccessRule::Related {
            relation: RelationKey::parse("customer")?,
            resource: "customer".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(rule),
        }),
    })
}

fn customer_detail_condition(scope: &ResolvedScope) -> Result<AccessRule> {
    Ok(AccessRule::Independent {
        permission: customer::DETAIL,
        rule: Box::new(AccessRule::Related {
            relation: RelationKey::parse("customer")?,
            resource: "customer".into(),
            mode: RelationMatch::ExactlyOne,
            rule: Box::new(super::data_condition(&customer::DETAIL, Some(scope))?),
        }),
    })
}

fn read_condition(scope: &ResolvedScope) -> AccessRule {
    AccessRule::Independent {
        permission: selection::ADMIN_IMAGE,
        rule: Box::new(
            AccessRule::scope(scope, false)
                .requiring(&BTreeSet::from([FactKind::Owner, FactKind::Organization])),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;
    use crate::entity::access_control::ScopeClause;
    use crate::entity::authorization::{FactValue, ObjectFacts, RelatedObjects};

    fn scope(company: bool) -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { company, self_owned: !company, ..Default::default() }],
            user_limit: None,
        }
    }

    fn object(owner: &str) -> ObjectFacts {
        ObjectFacts {
            resource: "sales_selection_booklet".into(),
            id: "book".into(),
            version: "1".into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(None),
            history_participants: FactValue::Known(BTreeSet::new()),
            ..Default::default()
        }
    }

    fn list_object(owner: &str) -> ObjectFacts {
        let mut root = object("actor");
        let mut child = object(owner);
        child.resource = "customer".into();
        child.id = "customer".into();
        root.relations.insert(
            RelationKey::parse("customer").unwrap(),
            RelatedObjects { expected_ids: [child.id.clone()].into(), objects: vec![child] },
        );
        root
    }

    #[test]
    fn write_requires_independent_get_qualification_and_get_scope() {
        let permission = selection::BOOKLET_PUBLISH;
        let qualified = QualifiedPermissions::test_grants(&[("write", slice::from_ref(&permission))]);
        assert!(matches!(
            condition(&permission, &qualified, &ScopeLookup::Resolved(&[]), AccessRule::Allow),
            Err(Error::Forbidden(_))
        ));
        let qualified = QualifiedPermissions::test_grants(&[
            ("write", slice::from_ref(&permission)),
            ("read", &[selection::ADMIN_IMAGE]),
            ("customer", &[customer::DETAIL]),
        ]);
        let scopes = [
            (selection::ADMIN_IMAGE, selection::ADMIN_IMAGE, scope(false)),
            (customer::DETAIL, customer::DETAIL, scope(true)),
        ];
        let rule =
            condition(&permission, &qualified, &ScopeLookup::Resolved(&scopes), AccessRule::Allow).unwrap();
        let mut root = list_object("actor");
        assert!(rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
        root.owner_id = FactValue::Known(Some("other".into()));
        assert!(!rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
        assert!(condition(&permission, &qualified, &ScopeLookup::Resolved(&[]), AccessRule::Allow).is_err());
    }

    #[test]
    fn binding_matches_runtime_with_capability_scopes_and_no_optional_qualification() {
        let qualified = QualifiedPermissions::test_grants(&[
            ("customer-list", &[customer::LIST]),
            ("customer-detail", &[customer::DETAIL]),
            ("booklet-read", &[selection::ADMIN_IMAGE]),
        ]);
        for permission in [selection::BOOKLET_LIST, selection::BOOKLET_CREATE, selection::BOOKLET_PUBLISH] {
            let scopes: Vec<_> = scope_requests(&permission)
                .into_iter()
                .map(|(entry, source)| {
                    let scope = super::super::capability_scope(&source).unwrap();
                    (entry, source, scope)
                })
                .collect();
            let base = AccessRule::Fact(AccessFact::Owned);
            assert_eq!(
                binding_condition(&permission, base.clone()).unwrap(),
                condition(&permission, &qualified, &ScopeLookup::Resolved(&scopes), base).unwrap(),
                "{permission}"
            );
        }
    }

    #[test]
    fn customer_list_requires_scope_and_owned_customer_unless_optional_qualification_exists() {
        let permission = selection::BOOKLET_LIST;
        let scopes = [(customer::LIST, customer::LIST, scope(true))];
        let ordinary = QualifiedPermissions::test_grants(&[("read", &[customer::LIST])]);
        let rule =
            condition(&permission, &ordinary, &ScopeLookup::Resolved(&scopes), AccessRule::Allow).unwrap();
        assert!(rule.allows_object("actor", &list_object("actor"), &BTreeSet::new()).unwrap());
        assert!(!rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());

        let all = QualifiedPermissions::test_grants(&[
            ("read", &[customer::LIST]),
            ("all", &[customer::ALL_AUTHORIZED_LIST]),
        ]);
        let rule = condition(&permission, &all, &ScopeLookup::Resolved(&scopes), AccessRule::Allow).unwrap();
        assert!(rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());
        assert!(rule.allows_object("actor", &object("actor"), &BTreeSet::new()).is_err());

        let limited = [(customer::LIST, customer::LIST, scope(false))];
        let rule = condition(&permission, &all, &ScopeLookup::Resolved(&limited), AccessRule::Allow).unwrap();
        assert!(!rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());
    }

    #[test]
    fn create_and_detail_require_customer_read_without_an_additional_booklet_get() {
        let qualified = QualifiedPermissions::test_grants(&[("customer", &[customer::DETAIL])]);
        let scopes = [(customer::DETAIL, customer::DETAIL, scope(true))];
        for permission in [selection::BOOKLET_CREATE, selection::ADMIN_IMAGE, selection::PROPOSAL_DETAIL] {
            let rule = condition(&permission, &qualified, &ScopeLookup::Resolved(&scopes), AccessRule::Allow)
                .unwrap();
            assert!(rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());
            assert_eq!(qualifications(&permission), vec![customer::DETAIL]);
            let missing = QualifiedPermissions::test_grants(&[]);
            assert!(matches!(
                condition(&permission, &missing, &ScopeLookup::Resolved(&scopes), AccessRule::Allow),
                Err(Error::Forbidden(_))
            ));
        }
    }

    #[test]
    fn customer_detail_preserves_history_before_the_personal_limit() {
        let qualified = QualifiedPermissions::test_grants(&[("customer", &[customer::DETAIL])]);
        let mut root = list_object("other");
        root.relations.get_mut(&RelationKey::parse("customer").unwrap()).unwrap().objects[0]
            .history_participants = FactValue::Known(["actor".into()].into());
        let mut customer_scope = scope(false);
        let scopes = [(customer::DETAIL, customer::DETAIL, customer_scope.clone())];
        let rule = condition(
            &selection::ADMIN_IMAGE,
            &qualified,
            &ScopeLookup::Resolved(&scopes),
            AccessRule::Allow,
        )
        .unwrap();
        assert!(rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());

        customer_scope.user_limit = Some(ScopeClause::default());
        let scopes = [(customer::DETAIL, customer::DETAIL, customer_scope)];
        let rule = condition(
            &selection::ADMIN_IMAGE,
            &qualified,
            &ScopeLookup::Resolved(&scopes),
            AccessRule::Allow,
        )
        .unwrap();
        assert!(!rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
    }
}
