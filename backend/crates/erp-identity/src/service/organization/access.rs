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

/// 以组织对象身份判断配置边界，不使用“同部门”作为权限。
///
/// # 参数
/// * `access` - 当前组织配置授权
/// * `id` - 目标组织；根节点创建时为空
///
/// # 返回
/// 授权覆盖该目标时为 `true`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 缺范围保持拒绝，不得补 Company。
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

/// 组织移动和包含下级授权须覆盖整个相关子树；调岗同时检查原组织和新组织。
///
/// # 参数
/// * `change` - 待执行的组织命令
/// * `access` - 当前组织配置授权
///
/// # 返回
/// 目标均在配置边界内时成功。
///
/// # 错误
/// 目标超出管理边界时返回 `Forbidden`；缺失管理关系返回 `NotFound`。
///
/// # 关键业务约束
/// 越界目标不得部分生效；部门负责人身份不代替配置动作。
pub(crate) fn ensure_targets(change: &OrganizationOperation, access: &AuthorizedDataScope) -> Result<()> {
    use OrganizationOperation::*;
    let state = &access.organizations;
    let mut targets = Vec::<Option<&str>>::new();
    match change {
        CreateUnit { parent_id, .. } => targets.push(parent_id.as_deref()),
        MoveUnit { org_unit_id, parent_id } => {
            targets.push(parent_id.as_deref());
            add_subtree_targets(state, org_unit_id, &mut targets)?;
        },
        RenameUnit { org_unit_id, .. } | DisableUnit { org_unit_id } => targets.push(Some(org_unit_id)),
        TransferMember { user_id, org_unit_id } => {
            targets.push(Some(org_unit_id));
            targets.push(state.own_org(user_id, access.as_of)?);
        },
        EndMembership { user_id } => targets.push(state.own_org(user_id, access.as_of)?),
        GrantManagement { org_unit_id, include_descendants, .. } => {
            targets.push(Some(org_unit_id));
            if *include_descendants {
                add_subtree_targets(state, org_unit_id, &mut targets)?;
            }
        },
        RevokeManagement { assignment_id } => {
            let grant = state
                .management
                .iter()
                .find(|g| g.base.id == *assignment_id)
                .ok_or_else(|| Error::NotFound("管理关系不存在".into()))?;
            targets.push(Some(&grant.org_unit_id));
            if grant.include_descendants {
                add_subtree_targets(state, &grant.org_unit_id, &mut targets)?;
            }
        },
    }
    if targets.into_iter().any(|id| !covers(access, id)) {
        return Err(Error::Forbidden("目标超出组织配置管理边界".into()));
    }
    Ok(())
}

/// 使用完整树验证子树边界；不按当前页或名称判断。
///
/// # 参数
/// * `state` - 完整组织事实
/// * `id` - 子树根
/// * `targets` - 追加检查的组织 ID
///
/// # 返回
/// 子树节点追加成功。
///
/// # 错误
/// 组织树不合法时返回校验错误。
///
/// # 关键业务约束
/// 必须覆盖原子树全部节点，禁止只检查可见页。
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

/// 幂等回放前仍检查当前管理边界，撤权后不能通过回执取回旧宽范围事实。
///
/// # 参数
/// * `receipt` - 已保存的回执
/// * `request` - 当前命令
/// * `access` - 当前授权
///
/// # 返回
/// 投影到当前边界后的回执。
///
/// # 错误
/// 幂等键已用于不同载荷时返回 `ConflictError`；越界时返回 `Forbidden`。
///
/// # 关键业务约束
/// 异载荷复用幂等键必须拒绝，不得覆盖原回执。
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

/// 客户端只接收当前可管理节点和相应关系，隐藏不可见父身份。
///
/// # 参数
/// * `state` - 完整或回执中的组织事实
/// * `access` - 当前授权
///
/// # 返回
/// 裁剪后的可见组织事实。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 缺范围得到空集，不得补 Company 或泄露不可见父节点身份。
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

/// 审计回执仅投影当前管理范围内的前后事实。
///
/// # 参数
/// * `receipt` - 原始回执
/// * `access` - 当前授权
///
/// # 返回
/// 可见前后事实的回执。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不可见组织不得出现在客户端回执中。
pub(crate) fn visible_receipt(
    mut receipt: OrganizationChangeReceipt,
    access: &AuthorizedDataScope,
) -> OrganizationChangeReceipt {
    receipt.before = visible_state(receipt.before, access);
    receipt.after = visible_state(receipt.after, access);
    receipt
}

/// 在已解析授权快照上准备预览或提交回执，不触达仓储。
///
/// # 参数
/// * `preview` - 预览不要求调用方写入
/// * `existing` - 已有幂等回执
/// * `request` - 当前命令
/// * `access` - 当前组织配置授权
/// * `change_id` - 新建关系主键
/// * `actor_id` - 操作人
/// * `at` - 变更时点
///
/// # 返回
/// `(可见回执, 是否持久化)`。预览与已有回放均不持久化。
///
/// # 错误
/// 幂等异载荷、期望版本冲突、越界目标或实体校验失败。
///
/// # 关键业务约束
/// 预览与提交共用校验；只有提交且无回放时写入。缺范围投影为空集。
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

    fn unit(id: &str) -> OrgUnit {
        OrgUnit::new(id.into(), id.into(), None, OrgUnitKind::Department, "admin".into(), "初始化".into())
            .unwrap()
    }

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

    fn access_with(scope: ResolvedScope, organizations: OrganizationState) -> AuthorizedDataScope {
        AuthorizedDataScope {
            user_id: "admin".into(),
            resource: "org_unit".into(),
            action: "manage".into(),
            scope,
            role_scopes: Default::default(),
            organizations,
            policy_version: 1,
            scope_version: "scope-v".into(),
            as_of: Instant::from_unix_secs(10),
        }
    }

    fn company_access(organizations: OrganizationState) -> AuthorizedDataScope {
        access_with(
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..ScopeClause::default() }],
                user_limit: None,
            },
            organizations,
        )
    }

    fn request(change: OrganizationOperation) -> OrganizationChangeRequest {
        OrganizationChangeRequest {
            expected_version: 1,
            idempotency_key: "change-1".into(),
            reason: "组织调整".into(),
            change,
        }
    }

    /// 预览不写入，提交才要求持久化；二者都重验期望版本。
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

    /// 角色缺范围时可见状态为空集，不得补 Company。
    #[test]
    fn no_scope_projects_empty_visible_state() {
        let access = access_with(ResolvedScope { role_clauses: Vec::new(), user_limit: None }, state());
        let visible = visible_state(access.organizations.clone(), &access);
        assert!(visible.units.is_empty());
        assert!(visible.memberships.is_empty());
        assert!(!access.scope.has_role_scope());
    }

    /// 越界目标在预览和提交前拒绝。
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

    /// 幂等键复用于不同载荷必须冲突，相同载荷回放且不写入。
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
