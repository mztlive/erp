//! 组合层共用的组织事实读取；不解释权限或数据范围规则。

use std::collections::BTreeSet;

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

/// 展开启用组织及其可选下级；快照错误由调用方映射。
///
/// # 参数
/// * `db` - 身份数据库
/// * `org_unit_ids` - 请求中的组织 ID
/// * `include_descendants` - 是否包含有效下级
/// * `executor` - 调用方执行器
/// * `map_load` - 组织快照读取错误映射
/// * `map_identity` - 组织树展开错误映射
///
/// # 返回
/// 返回启用节点的组织 ID 集合。
///
/// # 错误
/// 快照读取或未知组织时返回调用方映射后的领域错误。
///
/// # 关键业务约束
/// 资金必须传入 `Error::RepositoryError`；其余域可传入 `Into::into`。
pub(crate) async fn expand_org_units<E>(
    db: &Database,
    org_unit_ids: &[String],
    include_descendants: bool,
    executor: &mut dyn Executor,
    map_load: impl FnOnce(persistence_core::Error) -> E,
    map_identity: impl FnOnce(erp_identity::Error) -> E,
) -> Result<BTreeSet<String>, E> {
    let state = load_organization_state(db, executor).await.map_err(map_load)?;
    expand_org_ids(&state, org_unit_ids, include_descendants).map_err(map_identity)
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_identity::entity::organization::{OrgUnit, OrgUnitKind};

    use super::*;

    fn unit(id: &str, parent: Option<&str>, enabled: bool) -> OrgUnit {
        OrgUnit {
            base: BaseModel { id: id.to_string(), version: 1, created_at: 1, updated_at: 1, deleted_at: 0 },
            name: id.to_string(),
            parent_id: parent.map(str::to_string),
            kind: OrgUnitKind::Department,
            leader: None,
            enabled,
            changed_by: "admin".to_string(),
            reason: "test".to_string(),
        }
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
