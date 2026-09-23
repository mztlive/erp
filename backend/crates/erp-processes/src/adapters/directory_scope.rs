//! 对象目录组合授权；角色范围并集与个人上限求交，不从业务记录反推身份。
use std::collections::BTreeSet;

use application_core::directory::{DIRECTORY_LIMIT, DirectoryScope};
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopeDimension, ScopedObject};
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_identity::{Error, Result};

/// 将已校验的单身份维度范围映射为领域目录输入。
pub(super) fn directory_scope(
    access: AuthorizedDataScope,
    dimension: ScopeDimension,
) -> Result<DirectoryScope> {
    let ids = allowed_ids(&access.scope, dimension);
    if ids.as_ref().is_some_and(|ids| ids.len() > DIRECTORY_LIMIT) {
        return Err(Error::ValidationError("目录授权目标超过10000项，请收窄范围".into()));
    }
    let no_scope = ids.as_ref().is_some_and(Vec::is_empty);
    Ok(DirectoryScope {
        ids,
        scope_version: access.scope_version,
        policy_version: access.policy_version,
        organization_version: access.organizations.version,
        as_of: access.as_of.as_utc().to_rfc3339(),
        no_scope,
    })
}

/// 将同角色已证明的目标并集与个人上限求交；空范围绝不变成公司范围。
fn allowed_ids(scope: &ResolvedScope, dimension: ScopeDimension) -> Option<Vec<String>> {
    let company = scope.role_clauses.iter().any(|clause| clause.company);
    if company && scope.user_limit.as_ref().is_none_or(|limit| limit.company) {
        return None;
    }
    let candidates = if company {
        scope.user_limit.as_ref().map(|limit| targets(limit, dimension).clone()).unwrap_or_default()
    } else {
        scope
            .role_clauses
            .iter()
            .flat_map(|clause| targets(clause, dimension).iter().cloned())
            .collect::<BTreeSet<_>>()
    };
    Some(
        candidates
            .into_iter()
            .filter(|id| {
                let object = ScopedObject {
                    owned: false,
                    collaborating: false,
                    historical_read_participant: false,
                    org_unit_id: (dimension == ScopeDimension::InternalOrg).then_some(id.as_str()),
                    settlement_party_id: (dimension == ScopeDimension::SettlementParty)
                        .then_some(id.as_str()),
                    warehouse_id: (dimension == ScopeDimension::Warehouse).then_some(id.as_str()),
                };
                scope.allows(&object, false)
            })
            .collect(),
    )
}

/// 返回当前目录的目标维度；内部组织不参与本模块装配。
fn targets(clause: &ScopeClause, dimension: ScopeDimension) -> &BTreeSet<String> {
    match dimension {
        ScopeDimension::SettlementParty => &clause.settlement_party_ids,
        ScopeDimension::Warehouse => &clause.warehouse_ids,
        ScopeDimension::InternalOrg => &clause.org_unit_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_union_and_user_ceiling_match_scope_evaluation() {
        let scopes = [
            ResolvedScope { role_clauses: vec![], user_limit: None },
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            ResolvedScope {
                role_clauses: vec![
                    ScopeClause { warehouse_ids: ["a".into()].into(), ..Default::default() },
                    ScopeClause { warehouse_ids: ["b".into()].into(), ..Default::default() },
                ],
                user_limit: Some(ScopeClause { warehouse_ids: ["b".into()].into(), ..Default::default() }),
            },
        ];
        for scope in scopes {
            let allowed = allowed_ids(&scope, ScopeDimension::Warehouse);
            for id in ["a", "b", "c"] {
                let object = erp_identity::access_control::ScopedObject {
                    owned: false,
                    collaborating: false,
                    historical_read_participant: false,
                    org_unit_id: None,
                    settlement_party_id: None,
                    warehouse_id: Some(id),
                };
                assert_eq!(
                    allowed.as_ref().is_none_or(|ids| ids.iter().any(|v| v == id)),
                    scope.allows(&object, false)
                );
            }
        }
    }
}

#[cfg(test)]
mod matrix_tests {
    use super::*;
    #[test]
    fn object_directory_matrix_matches_canonical_evaluator() {
        for dimension in [ScopeDimension::SettlementParty, ScopeDimension::Warehouse] {
            let empty = ScopeClause::default();
            let company = ScopeClause { company: true, ..Default::default() };
            let a = ScopeClause {
                settlement_party_ids: ["a".into()].into(),
                warehouse_ids: ["a".into()].into(),
                ..Default::default()
            };
            let b = ScopeClause {
                settlement_party_ids: ["b".into()].into(),
                warehouse_ids: ["b".into()].into(),
                ..Default::default()
            };
            let single = match dimension {
                ScopeDimension::SettlementParty => {
                    ScopeClause { settlement_party_ids: ["a".into()].into(), ..Default::default() }
                },
                _ => ScopeClause { warehouse_ids: ["a".into()].into(), ..Default::default() },
            };
            for roles in [
                vec![],
                vec![empty.clone()],
                vec![company.clone()],
                vec![a.clone()],
                vec![a.clone(), b.clone()],
                vec![single.clone()],
                vec![single.clone(), company.clone()],
            ] {
                for ceiling in [
                    None,
                    Some(empty.clone()),
                    Some(company.clone()),
                    Some(a.clone()),
                    Some(b.clone()),
                    Some(single.clone()),
                ] {
                    let scope = ResolvedScope { role_clauses: roles.clone(), user_limit: ceiling };
                    let ids = allowed_ids(&scope, dimension);
                    for id in ["a", "b", "outside"] {
                        let object = ScopedObject {
                            owned: false,
                            collaborating: false,
                            historical_read_participant: false,
                            org_unit_id: None,
                            settlement_party_id: (dimension == ScopeDimension::SettlementParty).then_some(id),
                            warehouse_id: (dimension == ScopeDimension::Warehouse).then_some(id),
                        };
                        assert_eq!(
                            ids.as_ref().is_none_or(|ids| ids.iter().any(|v| v == id)),
                            scope.allows(&object, false)
                        );
                    }
                }
            }
        }
    }
}
