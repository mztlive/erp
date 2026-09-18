//! 范围 adapter 共用的组织快照与成员展开。
//!
//! 各 `*_data_scope` adapter 重复实现同一组织读取与成员过滤；本模块只收敛
//! 与领域无关的机械部分：同一执行器读取组织快照、组织树展开、有效主属成员
//! 排序去重。各域的条款映射与资源校验仍留在各自 adapter。

use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use erp_identity::entity::organization::OrgTree;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::repository::OrganizationRepository;
use mongodb::Database;
use persistence_core::Executor;

/// 在调用方事务内读取组织快照。
///
/// # 参数
/// * `db` - 身份数据库
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回同一事务中的组织状态。
///
/// # 错误
/// 组织集合读取失败时返回仓储错误，由调用方映射为领域错误。
///
/// # 关键业务约束
/// 不得另开事务或换成 `NoTransaction`。
pub(crate) async fn load_organization_state(
    db: &Database,
    executor: &mut dyn Executor,
) -> persistence_core::Result<OrganizationState> {
    OrganizationRepository::new(db).state(executor).await
}

/// 展开启用组织及其可选下级。
///
/// # 参数
/// * `state` - 当前组织事实
/// * `org_ids` - 请求中的组织 ID
/// * `include_descendants` - 是否包含有效下级
///
/// # 返回
/// 返回启用节点的组织 ID 集合。
///
/// # 错误
/// 未知组织或组织树非法时返回身份域错误，由调用方映射为领域错误。
///
/// # 关键业务约束
/// 筛选只能收窄授权结果，不得忽略未知组织。
pub(crate) fn expand_org_ids(
    state: &OrganizationState,
    org_ids: &[String],
    include_descendants: bool,
) -> erp_identity::Result<BTreeSet<String>> {
    let tree = OrgTree::new(&state.units)?;
    let mut expanded = BTreeSet::new();
    for id in org_ids {
        expanded.extend(tree.expand(id, include_descendants)?);
    }
    Ok(expanded)
}

/// 读取指定组织在给定时点的有效主属成员。
///
/// # 参数
/// * `state` - 组织事实
/// * `org_ids` - 内部组织集合
/// * `at` - 授权时点
///
/// # 返回
/// 返回排序去重后的人员 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 过期或未生效成员不得进入当前责任组织筛选。
pub(crate) fn member_ids(state: &OrganizationState, org_ids: &BTreeSet<String>, at: Instant) -> Vec<String> {
    let mut ids = state
        .memberships
        .iter()
        .filter(|membership| {
            !membership.base.is_deleted()
                && org_ids.contains(&membership.org_unit_id)
                && membership.validity.contains(at)
        })
        .map(|membership| membership.user_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_identity::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind, OrgValidity};

    use super::*;

    fn unit(id: &str, parent: Option<&str>, enabled: bool) -> OrgUnit {
        OrgUnit {
            base: BaseModel { id: id.to_string(), version: 1, created_at: 1, updated_at: 1, deleted_at: 0 },
            name: id.to_string(),
            parent_id: parent.map(str::to_string),
            kind: OrgUnitKind::Department,
            enabled,
            changed_by: "admin".to_string(),
            reason: "test".to_string(),
        }
    }

    fn membership(id: &str, user: &str, org: &str, from: i64, to: Option<i64>) -> OrgMembership {
        OrgMembership {
            base: BaseModel { id: id.to_string(), version: 1, created_at: 1, updated_at: 1, deleted_at: 0 },
            user_id: user.to_string(),
            org_unit_id: org.to_string(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(from),
                valid_to: to.map(Instant::from_unix_secs),
            },
            changed_by: "admin".to_string(),
            reason: "test".to_string(),
        }
    }

    fn deleted_membership(id: &str, user: &str, org: &str) -> OrgMembership {
        let mut item = membership(id, user, org, 1, None);
        item.base.deleted_at = 2;
        item
    }

    /// 有效主属成员按组织过滤并排序去重；删除与过期成员不得进入。
    #[test]
    fn member_ids_filters_deleted_expired_and_sorts() {
        let state = OrganizationState {
            version: 1,
            units: vec![unit("org-a", None, true)],
            memberships: vec![
                membership("m1", "user-2", "org-a", 1, None),
                membership("m2", "user-1", "org-a", 1, None),
                membership("m3", "user-1", "org-a", 1, None),
                membership("m4", "user-3", "org-b", 1, None),
                membership("m5", "user-4", "org-a", 1, Some(5)),
                deleted_membership("m6", "user-5", "org-a"),
            ],
            management: vec![],
        };
        let orgs = BTreeSet::from(["org-a".to_string()]);
        assert_eq!(
            member_ids(&state, &orgs, Instant::from_unix_secs(10)),
            vec!["user-1".to_string(), "user-2".to_string()]
        );
        assert!(member_ids(&state, &BTreeSet::new(), Instant::from_unix_secs(10)).is_empty());
    }

    /// 未知组织拒绝；停用节点不贡献范围。
    #[test]
    fn expand_rejects_unknown_and_skips_disabled() {
        let state = OrganizationState {
            version: 1,
            units: vec![unit("org-a", None, true), unit("org-b", None, false)],
            memberships: vec![],
            management: vec![],
        };
        assert!(expand_org_ids(&state, &["missing".to_string()], false).is_err());
        assert_eq!(
            expand_org_ids(&state, &["org-a".to_string()], false).unwrap(),
            BTreeSet::from(["org-a".to_string()])
        );
        assert!(expand_org_ids(&state, &["org-b".to_string()], false).unwrap().is_empty());
    }
}
