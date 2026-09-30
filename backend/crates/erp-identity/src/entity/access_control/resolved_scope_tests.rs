//! 合同 A02—A06、A27—A28、A32 的存储无关授权矩阵。

use super::*;
use crate::access_control::{DataScopeData, DataScopeId, ScopeBinding};
use crate::entity::organization::*;

fn node(id: &str, parent: Option<&str>) -> OrgUnit {
    OrgUnit::new(
        id.into(),
        id.into(),
        parent.map(str::to_owned),
        OrgUnitKind::Department,
        "admin".into(),
        "初始化".into(),
    )
    .unwrap()
}

fn rule(
    subject_type: DataScopeSubjectType,
    subject: &str,
    kind: DataScopeType,
    mode: Option<ScopeTargetMode>,
    targets: &[&str],
) -> DataScope {
    DataScope::new(
        DataScopeId::new(format!("scope-{subject}")),
        DataScopeData {
            subject_type,
            subject_id: subject.into(),
            scope_type: kind,
            scope_targets: targets.iter().map(|value| (*value).into()).collect(),
            binding: ScopeBinding {
                schema_version: 2,
                resource: "sales_order".into(),
                actions: vec!["detail".into()],
                target_dimension: ScopeDimension::InternalOrg,
                target_mode: mode,
                include_descendants: mode.filter(|mode| *mode != ScopeTargetMode::ManagedOrgs).map(|_| false),
                enabled: true,
            },
        },
    )
    .unwrap()
}

fn object(org: &str) -> ScopedObject<'_> {
    ScopedObject {
        owned: false,
        collaborating: false,
        historical_read_participant: false,
        org_unit_id: Some(org),
        settlement_party_id: None,
        warehouse_id: None,
    }
}

#[test]
fn legacy_managed_org_scope_fails_closed() {
    let nodes = vec![node("sales", None)];
    let tree = OrgTree::new(&nodes).unwrap();
    let roles = vec!["manager".into()];
    let rules = vec![rule(
        DataScopeSubjectType::Role,
        "manager",
        DataScopeType::Team,
        Some(ScopeTargetMode::ManagedOrgs),
        &[],
    )];
    let result = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    }
    .resolve();
    assert!(matches!(result, Err(Error::ValidationError(_))));
}

#[test]
fn company_scope_from_unqualified_role_cannot_supply_a_qualified_role() {
    let tree = OrgTree::new(&[]).unwrap();
    let roles = vec!["reader".into()];
    let rules = vec![
        rule(DataScopeSubjectType::Role, "other", DataScopeType::Company, None, &[]),
        rule(DataScopeSubjectType::User, "alice", DataScopeType::Company, None, &[]),
    ];
    let scope = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    }
    .resolve()
    .unwrap();
    assert!(!scope.allows(&object("one"), true));
    let mut participated = object("one");
    participated.historical_read_participant = true;
    assert!(scope.allows(&participated, true));
    assert!(!scope.allows(&participated, false));
    assert!(!scope.has_role_scope());
}

#[test]
fn missing_role_scope_stays_empty_and_is_not_company() {
    let tree = OrgTree::new(&[]).unwrap();
    let roles = vec!["reader".into()];
    let scope = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "org_unit",
        action: "list",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &[],
        memberships: &[],
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    }
    .resolve()
    .unwrap();
    assert!(!scope.has_role_scope());
    assert!(!scope.allows(&object("one"), false));
}

#[test]
fn personal_limit_constrains_history_and_empty_own_organization_never_means_company() {
    let tree = OrgTree::new(&[]).unwrap();
    let roles = vec!["reader".into()];
    let rules = vec![rule(
        DataScopeSubjectType::User,
        "alice",
        DataScopeType::Organization,
        Some(ScopeTargetMode::OwnOrg),
        &[],
    )];
    let scope = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    }
    .resolve()
    .unwrap();
    let mut participated = object("one");
    participated.historical_read_participant = true;
    assert!(!scope.allows(&participated, true));
    assert_eq!(scope.user_limit, Some(ScopeClause::default()));
}

#[test]
fn missing_required_dimension_denies_and_independent_dimensions_intersect() {
    let only_org = ScopeClause { org_unit_ids: BTreeSet::from(["one".into()]), ..Default::default() };
    assert!(!only_org.complete(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]));
    let both = ScopeClause { warehouse_ids: BTreeSet::from(["warehouse".into()]), ..only_org };
    assert!(both.complete(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]));
    let mut facts = object("one");
    assert!(!both.covers(&facts));
    facts.warehouse_id = Some("warehouse");
    assert!(both.covers(&facts));
    facts.org_unit_id = Some("warehouse");
    assert!(!both.covers(&facts));
}

fn personal_grant() -> PersonalBusinessGrant {
    use crate::entity::access_control::personal_grant::{PersonalBusinessGrantData, PersonalBusinessGrantId};
    PersonalBusinessGrant::new(
        PersonalBusinessGrantId::new("grant"),
        "alice",
        PersonalBusinessGrantData {
            role_id: "sales".into(),
            resource: "sales_order".into(),
            actions: vec!["detail".into(), "update".into()],
            org_unit_ids: vec!["one".into()],
            include_descendants: true,
        },
    )
    .unwrap()
}

#[test]
fn personal_department_grant_extends_only_matching_user_business_action_and_role() {
    let nodes = vec![node("one", None), node("child", Some("one")), node("two", None)];
    let tree = OrgTree::new(&nodes).unwrap();
    let roles = vec!["sales".into()];
    let rules = vec![rule(DataScopeSubjectType::Role, "sales", DataScopeType::SelfOwned, None, &[])];
    let grants = vec![personal_grant()];
    let input = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    };
    let (scope, evidence) = input.resolve_with_grants(&grants).unwrap();
    assert!(scope.allows(&object("one"), false));
    assert!(scope.allows(&object("child"), false));
    assert!(!scope.allows(&object("two"), false));
    assert!(evidence["sales"].org_unit_ids.contains("one"));
    let mut owned = object("two");
    owned.owned = true;
    assert!(scope.allows(&owned, false));
    assert!(
        !ScopeResolution { user_id: "bob", ..input }
            .resolve_with_grants(&grants)
            .unwrap()
            .0
            .allows(&object("one"), false)
    );
    assert!(
        !ScopeResolution { resource: "purchase_order", ..input }
            .resolve_with_grants(&grants)
            .unwrap()
            .0
            .allows(&object("one"), false)
    );
    assert!(
        !ScopeResolution { action: "delete", ..input }
            .resolve_with_grants(&grants)
            .unwrap()
            .0
            .allows(&object("one"), false)
    );
    let other_roles = vec!["other".into()];
    assert!(
        !ScopeResolution { eligible_role_ids: &other_roles, ..input }
            .resolve_with_grants(&grants)
            .unwrap()
            .0
            .allows(&object("one"), false)
    );
    assert!(ScopeResolution { eligible_role_ids: &[], ..input }.resolve_with_grants(&grants).is_err());
}

#[test]
fn personal_limit_still_intersects_and_revocation_restores_default() {
    let nodes = vec![node("one", None), node("two", None)];
    let tree = OrgTree::new(&nodes).unwrap();
    let roles = vec!["sales".into()];
    let rules = vec![
        rule(DataScopeSubjectType::Role, "sales", DataScopeType::SelfOwned, None, &[]),
        rule(DataScopeSubjectType::User, "alice", DataScopeType::SelfOwned, None, &[]),
    ];
    let input = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    };
    assert!(!input.resolve_with_grants(&[personal_grant()]).unwrap().0.allows(&object("one"), false));
    let mut revoked = personal_grant();
    revoked.base.deleted_at = 10;
    let (scope, evidence) =
        ScopeResolution { rules: &rules[..1], ..input }.resolve_with_grants(&[revoked]).unwrap();
    assert!(!scope.allows(&object("one"), false));
    assert!(evidence["sales"].self_owned);
    let mut owned = object("two");
    owned.owned = true;
    assert!(scope.allows(&owned, false));
}
