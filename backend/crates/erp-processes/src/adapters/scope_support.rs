//! 范围 adapter 共用的组织快照、维度拒绝与公共判定。
//!
//! 各 `*_data_scope` adapter 重复实现同一组织读取、不支持维度拒绝和已登记
//! 资源判定；本模块只收敛与领域无关的机械部分。资源名校验、本域条款类型
//! 构造和协作/历史参与字段仍由各 adapter 填写。组织快照错误映射由调用方
//! 传入：资金必须继续走 `Error::RepositoryError`，不得改成 `?`。

use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::entity::organization::OrgTree;
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::repository::OrganizationRepository;
use erp_identity::service::access_control::consumers::registration;
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

/// 拒绝结算主体或仓库维度；文案由调用方原样传入。
///
/// # 参数
/// * `clause` - 身份域正向范围
/// * `message` - 本域拒绝文案
///
/// # 返回
/// 两维皆空时成功。
///
/// # 错误
/// 结算主体或仓库目标非空时返回调用方文案。
///
/// # 关键业务约束
/// 不支持的维度必须拒绝，不得静默丢弃；不得改写调用方文案。
pub(crate) fn reject_unsupported_dimensions(clause: &ScopeClause, message: &str) -> Result<(), String> {
    if clause.settlement_party_ids.is_empty() && clause.warehouse_ids.is_empty() {
        return Ok(());
    }
    Err(message.to_string())
}

/// 将已解析条款转回公共判定输入，保留公司、本人、协作和组织。
///
/// # 参数
/// * `company` - 公司范围
/// * `self_owned` - 本人负责
/// * `collaborative` - 协作；集成域传入 `false`
/// * `org_unit_ids` - 已展开内部组织
///
/// # 返回
/// 返回不含结算主体和仓库的公共条款。
///
/// # 错误
/// 无。
pub(crate) fn scope_clause(
    company: bool,
    self_owned: bool,
    collaborative: bool,
    org_unit_ids: &[String],
) -> ScopeClause {
    ScopeClause {
        company,
        self_owned,
        collaborative,
        org_unit_ids: org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
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

/// 读取指定组织在给定时点的有效主属成员；快照错误由调用方映射。
///
/// # 参数
/// * `db` - 身份数据库
/// * `org_unit_ids` - 内部组织集合
/// * `at` - 授权时点
/// * `executor` - 调用方执行器
/// * `map_load` - 组织快照读取错误映射
///
/// # 返回
/// 返回排序去重后的人员 ID。
///
/// # 错误
/// 快照读取失败时返回调用方映射后的领域错误。
///
/// # 关键业务约束
/// 资金必须传入 `Error::RepositoryError`；其余域可传入 `Into::into`。
pub(crate) async fn org_member_ids<E>(
    db: &Database,
    org_unit_ids: &BTreeSet<String>,
    at: Instant,
    executor: &mut dyn Executor,
    map_load: impl FnOnce(persistence_core::Error) -> E,
) -> Result<Vec<String>, E> {
    let state = load_organization_state(db, executor).await.map_err(map_load)?;
    Ok(member_ids(&state, org_unit_ids, at))
}

/// 查询账号在解析时点的唯一主属组织；快照错误由调用方映射。
///
/// # 参数
/// * `db` - 身份数据库
/// * `user_id` - 当前账号
/// * `at` - 授权时点
/// * `executor` - 调用方执行器
/// * `map_load` - 组织快照读取错误映射
/// * `map_identity` - 多主属关系错误映射
///
/// # 返回
/// 存在唯一主属组织时返回其 ID；没有主属组织时返回 `None`。
///
/// # 错误
/// 快照读取失败或同一时点多条主属关系时返回调用方映射后的领域错误。
///
/// # 关键业务约束
/// 不得默认放入根组织或公司范围。资金 Port 无此方法。
pub(crate) async fn own_org<E>(
    db: &Database,
    user_id: &str,
    at: Instant,
    executor: &mut dyn Executor,
    map_load: impl FnOnce(persistence_core::Error) -> E,
    map_identity: impl FnOnce(erp_identity::Error) -> E,
) -> Result<Option<String>, E> {
    let state = load_organization_state(db, executor).await.map_err(map_load)?;
    Ok(state.own_org(user_id, at).map_err(map_identity)?.map(str::to_string))
}

/// 用已登记资源动作和本域填好的对象字段做公共判定。
///
/// # 参数
/// * `resource` - 已通过本域校验的资源名
/// * `action` - 已登记动作
/// * `role_clauses` - 转回公共条款的角色范围
/// * `user_limit` - 转回公共条款的个人上限
/// * `object` - 本域填写的责任事实；`collaborating` / `historical_read_participant` 由调用方填
/// * `map_identity` - 未登记资源动作的错误映射
///
/// # 返回
/// 返回角色范围和个人上限共同允许的判定。
///
/// # 错误
/// 资源动作未登记时返回调用方映射后的领域错误。
///
/// # 关键业务约束
/// 结算主体与仓库恒为 `None`；不得在此补公司范围或重解释原始规则。
pub(crate) fn evaluate_registered<E>(
    resource: &str,
    action: &str,
    role_clauses: impl IntoIterator<Item = ScopeClause>,
    user_limit: Option<ScopeClause>,
    object: ScopedObject<'_>,
    map_identity: impl FnOnce(erp_identity::Error) -> E,
) -> Result<bool, E> {
    let consumer = registration(resource, action).map_err(map_identity)?;
    let resolved = ResolvedScope { role_clauses: role_clauses.into_iter().collect(), user_limit };
    Ok(resolved.allows(
        &ScopedObject {
            owned: object.owned,
            collaborating: object.collaborating,
            historical_read_participant: object.historical_read_participant,
            org_unit_id: object.org_unit_id,
            settlement_party_id: None,
            warehouse_id: None,
        },
        consumer.allows_history,
    ))
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

    /// 拒绝文案由调用方决定，空维度不得改写。
    #[test]
    fn reject_unsupported_dimensions_keeps_caller_message() {
        let warehouse =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        assert_eq!(
            reject_unsupported_dimensions(&warehouse, "商品范围不支持结算主体或仓库维度").unwrap_err(),
            "商品范围不支持结算主体或仓库维度"
        );
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert_eq!(
            reject_unsupported_dimensions(&settlement, "资金范围不支持结算主体或仓库维度").unwrap_err(),
            "资金范围不支持结算主体或仓库维度"
        );
        let allowed = ScopeClause { self_owned: true, ..ScopeClause::default() };
        assert!(reject_unsupported_dimensions(&allowed, "unused").is_ok());
    }

    /// 公共判定忽略对象上的结算主体和仓库，即使调用方误填。
    #[test]
    fn evaluate_registered_ignores_settlement_and_warehouse() {
        let warehouse_only =
            ScopeClause { warehouse_ids: BTreeSet::from(["wh-1".into()]), ..ScopeClause::default() };
        let object = ScopedObject {
            owned: true,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: None,
            settlement_party_id: None,
            warehouse_id: Some("wh-1"),
        };
        let denied = evaluate_registered("product", "list", vec![warehouse_only], None, object, |error| {
            error.to_string()
        })
        .unwrap();
        assert!(!denied);
        let company = ScopeClause { company: true, ..ScopeClause::default() };
        let allowed = evaluate_registered(
            "product",
            "list",
            vec![company],
            None,
            ScopedObject {
                owned: false,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: None,
                settlement_party_id: None,
                warehouse_id: None,
            },
            |error| error.to_string(),
        )
        .unwrap();
        assert!(allowed);
    }
}
