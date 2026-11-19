//! 组织配置边界、幂等回放与预览/提交分流；不触达仓储。

use entity_core::BaseModel;
use erp_core::common::time::Instant;

use crate::access_control::ScopedObject;
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::{
    OrganizationChangeReceipt, OrganizationChangeRequest, OrganizationOperation, OrganizationState,
};
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Result};

// 用组织 ID 检查配置授权是否覆盖目标。空目标用于创建根节点，须具备公司范围；缺少范围时拒绝。
fn covers(access: &AuthorizedDataScope, id: Option<&str>) -> bool {
    access.scope.allows(
        &ScopedObject {
            owned: false,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: id,
            settlement_party_id: None,
            warehouse_id: None,
        },
        false,
    )
}

/// 提交组织命令前校验全部受影响的组织。移动覆盖整棵子树，调岗同时检查原组织与目标组织；越界时拒绝整笔操作，旧管理关系命令返回参数错误。
pub(crate) fn ensure_targets(change: &OrganizationOperation, access: &AuthorizedDataScope) -> Result<()> {
    use OrganizationOperation::*;
    let state = &access.organizations;
    let mut targets = Vec::<Option<&str>>::new();
    match change {
        UpdatePersonProfile { profile } => {
            if !covers(access, state.own_org(&profile.user_id, access.as_of)?) {
                return Err(Error::Forbidden("人员超出组织配置管理边界".into()));
            }
            for operation in profile.operations()? {
                ensure_targets(&operation, access)?;
            }
        },
        CreateUnit { parent_id, .. } => targets.push(parent_id.as_deref()),
        MoveUnit { org_unit_id, parent_id } => {
            targets.push(parent_id.as_deref());
            add_subtree_targets(state, org_unit_id, &mut targets)?;
        },
        SetLeader { org_unit_id, .. } | RenameUnit { org_unit_id, .. } | DisableUnit { org_unit_id } => {
            targets.push(Some(org_unit_id))
        },
        TransferMember { user_id, org_unit_id } => {
            targets.push(Some(org_unit_id));
            targets.push(state.own_org(user_id, access.as_of)?);
        },
        TransferMembers { user_ids, org_unit_id } => {
            targets.push(Some(org_unit_id));
            for user_id in user_ids {
                targets.push(state.own_org(user_id, access.as_of)?);
            }
        },
        EndMembership { user_id } => targets.push(state.own_org(user_id, access.as_of)?),
        GrantManagement { .. } | RevokeManagement { .. } => {
            return Err(Error::ValidationError("部门管理关系已停用，请在人员数据范围中设置授权".into()));
        },
    }

    if targets.into_iter().any(|id| !covers(access, id)) {
        return Err(Error::Forbidden("目标超出组织配置管理边界".into()));
    }
    Ok(())
}

// 从完整组织树收集移动会影响的全部节点，供后续逐一校验权限。树不合法时返回错误。
fn add_subtree_targets<'a>(
    state: &'a OrganizationState,
    id: &str,
    targets: &mut Vec<Option<&'a str>>,
) -> Result<()> {
    let tree = OrgTree::new(&state.units)?;
    let ids = tree.expand(id, true)?;
    targets.extend(state.units.iter().filter(|u| ids.contains(&u.base.id)).map(|u| Some(u.base.id.as_str())));
    Ok(())
}

/// 核对重复提交的命令载荷和当前管理范围，然后裁剪已保存的回执。载荷冲突或当前越界时拒绝回放。
pub(crate) fn replay(
    receipt: OrganizationChangeReceipt,
    request: &OrganizationChangeRequest,
    access: &AuthorizedDataScope,
) -> Result<OrganizationChangeReceipt> {
    if receipt.request != *request {
        return Err(Error::ConflictError("幂等键已用于不同变更".into()));
    }
    ensure_targets(&request.change, access)?;
    Ok(visible_receipt(receipt, access))
}

/// 只投影当前可管理的节点和相关成员、管理关系，供页面或回执展示。隐藏不可见父节点的 ID；无范围时返回空集。
pub(crate) fn visible_state(mut state: OrganizationState, access: &AuthorizedDataScope) -> OrganizationState {
    state.units.retain(|u| covers(access, Some(&u.base.id)));
    let ids = state.units.iter().map(|u| u.base.id.clone()).collect::<std::collections::BTreeSet<_>>();
    for unit in &mut state.units {
        if unit.parent_id.as_ref().is_some_and(|id| !ids.contains(id)) {
            unit.parent_id = None;
        }
    }

    state.memberships.retain(|m| ids.contains(&m.org_unit_id));
    state.management.retain(|m| ids.contains(&m.org_unit_id));
    state
}

/// 按当前授权裁剪变更前后的组织事实，使回放和预览只返回当前调用人可见的数据。
pub(crate) fn visible_receipt(
    mut receipt: OrganizationChangeReceipt,
    access: &AuthorizedDataScope,
) -> OrganizationChangeReceipt {
    receipt.before = visible_state(receipt.before, access);
    receipt.after = visible_state(receipt.after, access);
    receipt
}

/// 在已授权快照中检查幂等键、期望版本和变更目标，生成可见回执与写入标记。只有没有旧回执的正式提交才需要持久化。
pub(crate) fn prepare_organization_change(
    preview: bool,
    existing: Option<OrganizationChangeReceipt>,
    request: OrganizationChangeRequest,
    access: &AuthorizedDataScope,
    change_id: &str,
    actor_id: &str,
    at: Instant,
) -> Result<(OrganizationChangeReceipt, bool)> {
    if let Some(receipt) = existing {
        return Ok((replay(receipt, &request, access)?, false));
    }

    ensure_targets(&request.change, access)?;
    let after = access.organizations.changed(&request, change_id, actor_id, at)?;
    let receipt = OrganizationChangeReceipt {
        base: BaseModel::new(format!("{actor_id}:{}", request.idempotency_key)),
        actor_id: actor_id.into(),
        request,
        before: access.organizations.clone(),
        after,
        as_of: at,
    };
    Ok((visible_receipt(receipt, access), !preview))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind, OrgValidity};

    // 创建启用的独立测试部门，用 ID 区分管理边界。
    fn unit(id: &str) -> OrgUnit {
        OrgUnit::new(id.into(), id.into(), None, OrgUnitKind::Department, "admin".into(), "初始化".into())
            .unwrap()
    }

    // 构造含两个部门及一名现有成员的快照，供目标越界和调岗测试使用。
    fn state() -> OrganizationState {
        OrganizationState {
            version: 1,
            units: vec![unit("one"), unit("two")],
            memberships: vec![OrgMembership {
                base: BaseModel::new("membership".into()),
                user_id: "sales".into(),
                org_unit_id: "one".into(),
                validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: None },
                changed_by: "admin".into(),
                reason: "初始化".into(),
            }],
            management: Vec::new(),
        }
    }

    // 用指定范围与组织快照构造固定时点的测试授权，便于单独验证组织规则。
    fn access_with(scope: ResolvedScope, organizations: OrganizationState) -> AuthorizedDataScope {
        AuthorizedDataScope {
            scope,
            organizations,
            policy_version: 1,
            scope_version: "scope-v".into(),
            as_of: Instant::from_unix_secs(10),
        }
    }

    // 提供公司级组织范围，用于隔离测试版本、载荷或幂等校验。
    fn company_access(organizations: OrganizationState) -> AuthorizedDataScope {
        access_with(
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..ScopeClause::default() }],
                user_limit: None,
            },
            organizations,
        )
    }

    // 为变更测试生成固定版本及幂等键，使不同测试可只变动业务命令。
    fn request(change: OrganizationOperation) -> OrganizationChangeRequest {
        OrganizationChangeRequest {
            expected_version: 1,
            idempotency_key: "change-1".into(),
            reason: "组织调整".into(),
            change,
        }
    }

    // 构造在测试时点有效的主属关系，供批量调岗场景设置原部门。
    fn member(id: &str, user: &str, org: &str) -> OrgMembership {
        OrgMembership {
            base: BaseModel::new(id.into()),
            user_id: user.into(),
            org_unit_id: org.into(),
            validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: None },
            changed_by: "admin".into(),
            reason: "初始化".into(),
        }
    }

    // 验证配置负责人时必须管理目标组织；越界请求在应用变更前被拒绝。
    #[test]
    fn leader_assignment_requires_target_management_scope() {
        let change =
            OrganizationOperation::SetLeader { org_unit_id: "two".into(), user_id: Some("leader".into()) };
        let limited = access_with(
            ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: ["one".into()].into(),
                    ..ScopeClause::default()
                }],
                user_limit: None,
            },
            state(),
        );
        assert!(matches!(ensure_targets(&change, &limited), Err(Error::Forbidden(_))));
        assert!(ensure_targets(&change, &company_access(state())).is_ok());
    }

    // 验证批量调岗会检查逐个成员涉及的原组织和目标组织，越界时整批拒绝。
    #[test]
    fn batch_transfer_checks_every_member_org() {
        let mut state = state();
        state.memberships.push(member("buyer-membership", "buyer", "two"));
        let change = OrganizationOperation::TransferMembers {
            user_ids: vec!["sales".into(), "buyer".into()],
            org_unit_id: "one".into(),
        };
        let limited = access_with(
            ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: ["one".into()].into(),
                    ..ScopeClause::default()
                }],
                user_limit: None,
            },
            state.clone(),
        );
        assert!(matches!(ensure_targets(&change, &limited), Err(Error::Forbidden(_))));
        assert!(ensure_targets(&change, &company_access(state)).is_ok());
    }

    // 验证合并资料命令逐项检查边界，幂等回放要求完整载荷一致。
    #[test]
    fn profile_change_checks_all_targets_and_replay_payload() {
        use crate::entity::person_profile_change::PersonProfileChange;
        let change = OrganizationOperation::UpdatePersonProfile {
            profile: PersonProfileChange {
                user_id: "sales".into(),
                expected_name: "原姓名".into(),
                expected_role_ids: None,
                role_ids: None,
                name: Some("新姓名".into()),
                org_unit_id: Some("two".into()),
                remove_management_ids: vec![],
                add_management: vec![],
            },
        };
        let limited = access_with(
            ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: ["one".into()].into(),
                    ..ScopeClause::default()
                }],
                user_limit: None,
            },
            state(),
        );
        assert!(matches!(ensure_targets(&change, &limited), Err(Error::Forbidden(_))));

        let access = company_access(state());
        let request = request(change);
        let (receipt, persist) = prepare_organization_change(
            false,
            None,
            request.clone(),
            &access,
            "new",
            "admin",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert!(persist);

        let current = company_access(receipt.after.clone());
        let (_, persist) = prepare_organization_change(
            false,
            Some(receipt.clone()),
            request.clone(),
            &current,
            "unused",
            "admin",
            Instant::from_unix_secs(20),
        )
        .unwrap();
        assert!(!persist);

        let mut altered = request;
        if let OrganizationOperation::UpdatePersonProfile { profile } = &mut altered.change {
            profile.name = Some("另一个姓名".into());
        }
        assert!(matches!(replay(receipt, &altered, &current), Err(Error::ConflictError(_))));
    }

    // 验证预览保持只读，正式提交才标记写入，两条路径都拒绝旧版本。
    #[test]
    fn preview_does_not_persist_and_stale_version_fails_for_both_paths() {
        let access = company_access(state());
        let change = OrganizationOperation::RenameUnit { org_unit_id: "one".into(), name: "一部".into() };
        let (receipt, persist) = prepare_organization_change(
            true,
            None,
            request(change.clone()),
            &access,
            "new-id",
            "admin",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert!(!persist);
        assert_eq!(receipt.after.units.iter().find(|u| u.base.id == "one").unwrap().name, "一部");

        let (receipt, persist) = prepare_organization_change(
            false,
            None,
            request(change.clone()),
            &access,
            "new-id",
            "admin",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert!(persist);
        assert_eq!(receipt.after.version, 2);

        let mut stale = request(change);
        stale.expected_version = 0;
        assert!(matches!(
            prepare_organization_change(
                true,
                None,
                stale,
                &access,
                "new-id",
                "admin",
                Instant::from_unix_secs(10),
            ),
            Err(Error::ConflictError(_))
        ));
    }

    // 验证缺少管理范围时，组织、成员及管理关系均裁剪为空集。
    #[test]
    fn no_scope_projects_empty_visible_state() {
        let access = access_with(ResolvedScope { role_clauses: Vec::new(), user_limit: None }, state());
        let visible = visible_state(access.organizations.clone(), &access);
        assert!(visible.units.is_empty());
        assert!(visible.memberships.is_empty());
        assert!(!access.scope.has_role_scope());
    }

    // 验证同一个越界命令在预览和正式提交时都被拒绝。
    #[test]
    fn out_of_boundary_target_is_forbidden() {
        let access = access_with(
            ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: ["one".into()].into(),
                    ..ScopeClause::default()
                }],
                user_limit: None,
            },
            state(),
        );
        let err = prepare_organization_change(
            true,
            None,
            request(OrganizationOperation::RenameUnit { org_unit_id: "two".into(), name: "二部".into() }),
            &access,
            "new-id",
            "admin",
            Instant::from_unix_secs(10),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Forbidden(_)));
    }

    // 验证同载荷可只读重放，复用幂等键提交不同命令时产生冲突。
    #[test]
    fn idempotent_replay_rejects_different_payload() {
        let access = company_access(state());
        let first =
            request(OrganizationOperation::RenameUnit { org_unit_id: "one".into(), name: "一部".into() });
        let (saved, persist) = prepare_organization_change(
            false,
            None,
            first.clone(),
            &access,
            "new-id",
            "admin",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert!(persist);

        let (replayed, persist) = prepare_organization_change(
            false,
            Some(saved.clone()),
            first,
            &access,
            "new-id",
            "admin",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert!(!persist);
        assert_eq!(replayed.after.version, saved.after.version);

        let different = request(OrganizationOperation::RenameUnit {
            org_unit_id: "one".into(),
            name: "其他名称".into(),
        });
        assert!(matches!(
            prepare_organization_change(
                false,
                Some(saved),
                different,
                &access,
                "new-id",
                "admin",
                Instant::from_unix_secs(10),
            ),
            Err(Error::ConflictError(_))
        ));
    }
}
