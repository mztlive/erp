//! 合同 A02—A06、A27—A28、A32 的存储无关授权矩阵。

use super::*;
use crate::access_control::{DataScopeData, DataScopeId, ScopeBinding};
use crate::entity::organization::*;
use entity_core::BaseModel;

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
                include_descendants: mode
                    .filter(|mode| *mode != ScopeTargetMode::ManagedOrgs)
                    .map(|_| false),
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

fn grant(user: &str, role: &str, org: &str, descendants: bool) -> OrgManagementAssignment {
    OrgManagementAssignment {
        base: BaseModel::new(format!("{user}-{role}-{org}")),
        user_id: user.into(),
        role_id: role.into(),
        org_unit_id: org.into(),
        include_descendants: descendants,
        validity: OrgValidity {
            valid_from: Instant::from_unix_secs(1),
            valid_to: None,
        },
        granted_by: "admin".into(),
        reason: "管理授权".into(),
    }
}

#[test]
fn same_manager_role_resolves_each_users_explicit_management_relationships() {
    let nodes = vec![
        node("sales", None),
        node("one", Some("sales")),
        node("two", Some("sales")),
        node("other", None),
    ];
    let tree = OrgTree::new(&nodes).unwrap();
    let roles = vec!["manager".into()];
    let rules = vec![rule(
        DataScopeSubjectType::Role,
        "manager",
        DataScopeType::Team,
        Some(ScopeTargetMode::ManagedOrgs),
        &[],
    )];
    let grants = vec![
        grant("alice", "manager", "one", false),
        grant("bob", "manager", "two", false),
    ];
    for (user, visible, hidden) in [("alice", "one", "two"), ("bob", "two", "one")] {
        let scope = ScopeResolution {
            user_id: user,
            eligible_role_ids: &roles,
            resource: "sales_order",
            action: "detail",
            required_dimensions: &[ScopeDimension::InternalOrg],
            rules: &rules,
            memberships: &[],
            management: &grants,
            tree: &tree,
            as_of: Instant::from_unix_secs(10),
        }
        .resolve()
        .unwrap();
        assert!(scope.allows(&object(visible), false));
        assert!(!scope.allows(&object(hidden), false));
        assert!(!scope.allows(&object("other"), false));
    }
}

#[test]
fn descendants_and_cross_team_management_are_explicit_and_role_bound() {
    let nodes = vec![
        node("sales", None),
        node("one", Some("sales")),
        node("other", None),
    ];
    let tree = OrgTree::new(&nodes).unwrap();
    let roles = vec!["manager".into()];
    let rules = vec![rule(
        DataScopeSubjectType::Role,
        "manager",
        DataScopeType::Team,
        Some(ScopeTargetMode::ManagedOrgs),
        &[],
    )];
    for include in [false, true] {
        let grants = vec![
            grant("alice", "manager", "sales", include),
            grant("alice", "unqualified-role", "other", true),
        ];
        let scope = ScopeResolution {
            user_id: "alice",
            eligible_role_ids: &roles,
            resource: "sales_order",
            action: "detail",
            required_dimensions: &[ScopeDimension::InternalOrg],
            rules: &rules,
            memberships: &[],
            management: &grants,
            tree: &tree,
            as_of: Instant::from_unix_secs(10),
        }
        .resolve()
        .unwrap();
        assert_eq!(scope.allows(&object("one"), false), include);
        assert!(!scope.allows(&object("other"), false));
    }
    let grants = vec![grant("alice", "manager", "other", false)];
    let scope = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        management: &grants,
        tree: &tree,
        as_of: Instant::from_unix_secs(10),
    }
    .resolve()
    .unwrap();
    assert!(scope.allows(&object("other"), false));
}

#[test]
fn company_scope_from_unqualified_role_cannot_supply_a_qualified_role() {
    let tree = OrgTree::new(&[]).unwrap();
    let roles = vec!["reader".into()];
    let rules = vec![
        rule(
            DataScopeSubjectType::Role,
            "other",
            DataScopeType::Company,
            None,
            &[],
        ),
        rule(
            DataScopeSubjectType::User,
            "alice",
            DataScopeType::Company,
            None,
            &[],
        ),
    ];
    let scope = ScopeResolution {
        user_id: "alice",
        eligible_role_ids: &roles,
        resource: "sales_order",
        action: "detail",
        required_dimensions: &[ScopeDimension::InternalOrg],
        rules: &rules,
        memberships: &[],
        management: &[],
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
        management: &[],
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
        management: &[],
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
    let only_org = ScopeClause {
        org_unit_ids: BTreeSet::from(["one".into()]),
        ..Default::default()
    };
    assert!(!only_org.complete(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]));
    let both = ScopeClause {
        warehouse_ids: BTreeSet::from(["warehouse".into()]),
        ..only_org
    };
    assert!(both.complete(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]));
    let mut facts = object("one");
    assert!(!both.covers(&facts));
    facts.warehouse_id = Some("warehouse");
    assert!(both.covers(&facts));
    facts.org_unit_id = Some("warehouse");
    assert!(!both.covers(&facts));
}
