//! 人员目录读取：授权、资格和组织在调用方事务内一次完成。

use std::collections::BTreeSet;

use application_core::{AuditActor, PageView};
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::PersonDirectoryPage;
use crate::entity::fingerprint::Fingerprint;
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::entity::person_directory::{
    DirectoryListRequest, PersonDirectoryCategory, candidate_in_directory, directory_org_ids,
};
use crate::repository::OrganizationRepository;
use crate::repository::fingerprint::BsonFingerprint;
use crate::repository::person_directory_query::{
    DIRECTORY_LIMIT, DirectoryRead, DirectoryResult, directory_page,
};
use crate::service::access_control::resolve::{DataScopeFacts, DataScopeService};
use crate::service::iam::SharedRbacService;
use crate::{Error, Permission, Result};

/// 查询一页人员目录。
///
/// # 参数
/// * `db` - 身份数据库
/// * `rbac` - RBAC 服务
/// * `actor` - 已认证操作人
/// * `category` - 路由固定的查询类别
/// * `request` - 已规范化的目录条件
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回目录页。无范围时为空页并标记 `no_scope`。
///
/// # 错误
/// 无动作权限、版本变化、非法组织或读取失败时返回错误。
pub async fn list_page(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    request: &DirectoryListRequest,
    executor: &mut dyn Executor,
) -> Result<PersonDirectoryPage> {
    let mut authorized = authorize(db, rbac, actor, category, executor).await?;
    let org_filter = expand_org_filter(
        &authorized.facts.organizations,
        &request.org_unit_ids,
        request.include_descendants,
    )?;
    load_scope_memberships(db, &mut authorized, org_filter.as_ref(), None, executor).await?;
    let ids = scope_ids(&authorized, org_filter.as_ref(), None)?;
    let result = directory_page(
        db,
        &DirectoryRead {
            category,
            authorized_ids: ids.as_deref(),
            selected_ids: None,
            search: request.search.as_deref(),
            page: request.page,
            page_size: request.page_size,
        },
        executor,
    )
    .await?;
    let display = display_version(db, &mut authorized, &result, executor).await?;
    // v2 字段顺序：展示版本、可选搜索词、可选无序组织集合。
    let mut version = Fingerprint::new("person-directory-query-v2");
    version.text(&display);
    version.optional(request.search.as_deref(), Fingerprint::text);
    version.optional(org_filter.as_ref(), |hash, ids| hash.unordered(ids, |hash, id| hash.text(id)));
    let version = version.finish();
    ensure_current_version(&request.scope_version, &version)?;
    page_from_result(result, request.page, request.page_size, &authorized, version)
}

/// 回显已选人员。不存在或无权读取的 ID 不返回名称。
///
/// # 参数
/// * `db` - 身份数据库
/// * `rbac` - RBAC 服务
/// * `actor` - 已认证操作人
/// * `category` - 路由固定的查询类别
/// * `ids` - 已去重的已选账号 ID
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回当前仍可读的人员及授权元信息。
///
/// # 错误
/// 无动作权限或读取失败时返回错误。
pub async fn selected_page(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<PersonDirectoryPage> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(Error::ValidationError("已选人员须为 1 到 100 项".into()));
    }
    let page_size = u32::try_from(ids.len()).map_err(|_| Error::ValidationError("已选人员过多".into()))?;
    let mut authorized = authorize(db, rbac, actor, category, executor).await?;
    load_scope_memberships(db, &mut authorized, None, Some(ids), executor).await?;
    let allowed = scope_ids(&authorized, None, Some(ids))?;
    let result = directory_page(
        db,
        &DirectoryRead {
            category,
            authorized_ids: allowed.as_deref(),
            selected_ids: Some(ids),
            search: None,
            page: 1,
            page_size,
        },
        executor,
    )
    .await?;
    let version = display_version(db, &mut authorized, &result, executor).await?;
    page_from_result(result, 1, page_size, &authorized, version)
}

struct AuthorizedDirectory {
    facts: DataScopeFacts,
    actor_id: String,
}

/// 在调用方事务内按目录类别的 `list` 动作解析数据范围。
async fn authorize(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    executor: &mut dyn Executor,
) -> Result<AuthorizedDirectory> {
    let resolved = DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, &Permission::parse(format!("{}:{}", category.resource(), "list"))?, executor)
        .await?;
    Ok(AuthorizedDirectory { facts: resolved.facts, actor_id: actor.id().to_string() })
}

/// 仅在明确组织或 selected 身份内装载当前关系，数据库阶段限制规模。
async fn load_scope_memberships(
    db: &Database,
    authorized: &mut AuthorizedDirectory,
    org_filter: Option<&BTreeSet<String>>,
    selected: Option<&[String]>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let repository = OrganizationRepository::new(db);
    let orgs = directory_org_ids(&authorized.facts.scope, org_filter).into_iter().collect::<Vec<_>>();
    let rows = if let Some(ids) = selected {
        repository.directory_memberships(Some(ids), None, authorized.facts.as_of, executor).await?
    } else {
        repository.directory_memberships(None, Some(&orgs), authorized.facts.as_of, executor).await?
    };
    let mut seen = authorized
        .facts
        .organizations
        .memberships
        .iter()
        .map(|row| row.base.id.clone())
        .collect::<BTreeSet<_>>();
    for row in rows {
        if seen.insert(row.base.id.clone()) {
            authorized.facts.organizations.memberships.push(row);
        }
    }
    Ok(())
}

/// 只为已受限的内容版本索引读取组织标签；有效期跨界亦改变目录版本。
async fn display_version(
    db: &Database,
    authorized: &mut AuthorizedDirectory,
    result: &DirectoryResult,
    executor: &mut dyn Executor,
) -> Result<String> {
    let ids = result
        .versions
        .iter()
        .map(|row| {
            row.get_str("id")
                .map(str::to_owned)
                .map_err(|_| Error::Internal("人员目录版本索引缺少身份".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    authorized.facts.organizations.memberships = OrganizationRepository::new(db)
        .directory_memberships(Some(&ids), None, authorized.facts.as_of, executor)
        .await?;
    // v2 字段顺序：内容版本、无序关系（账号、组织、关系 ID、关系版本）。
    let mut version = Fingerprint::new("person-directory-display-v2");
    version.text(&content_version(&authorized.facts.scope_version, result));
    version.unordered(&authorized.facts.organizations.memberships, |hash, row| {
        hash.text(&row.user_id);
        hash.text(&row.org_unit_id);
        hash.text(&row.base.id);
        hash.number(row.base.version);
    });
    Ok(version.finish())
}

/// 将组织状态转为有界的人员范围索引；公司无个人上限时不展开全部账号。
fn scope_ids(
    authorized: &AuthorizedDirectory,
    org_filter: Option<&BTreeSet<String>>,
    selected: Option<&[String]>,
) -> Result<Option<Vec<String>>> {
    let scope = &authorized.facts.scope;
    if selected.is_none()
        && org_filter.is_none()
        && scope.role_clauses.iter().any(|clause| clause.company)
        && scope.user_limit.as_ref().is_none_or(|limit| limit.company)
    {
        return Ok(None);
    }
    let candidates =
        selected.map(|ids| ids.iter().map(String::as_str).collect::<BTreeSet<_>>()).unwrap_or_else(|| {
            let mut ids = authorized
                .facts
                .organizations
                .memberships
                .iter()
                .map(|m| m.user_id.as_str())
                .collect::<BTreeSet<_>>();
            ids.insert(authorized.actor_id.as_str());
            ids
        });
    let mut ids = Vec::new();
    for id in candidates {
        let org = authorized.facts.organizations.own_org(id, authorized.facts.as_of)?;
        if candidate_in_directory(scope, &authorized.actor_id, id, org, org_filter) {
            ids.push(id.to_owned());
        }
        if ids.len() > DIRECTORY_LIMIT {
            return Err(Error::ValidationError("人员范围超过上限，请收窄组织条件".into()));
        }
    }
    Ok(Some(ids))
}

/// 内容索引包含账号显示信息和资格版本，不用人数代替目录版本。
fn content_version(scope: &str, result: &DirectoryResult) -> String {
    // v2 字段顺序：范围版本、无序人员版本文档；文档内部保留 BSON 字段顺序。
    let mut hash = Fingerprint::new("person-directory-content-v2");
    hash.text(scope);
    hash.unordered(&result.versions, Fingerprint::document);
    hash.finish()
}

/// 展开用户指定的目录组织条件。
fn expand_org_filter(
    state: &OrganizationState,
    org_unit_ids: &[String],
    include_descendants: bool,
) -> Result<Option<BTreeSet<String>>> {
    if org_unit_ids.is_empty() {
        return Ok(None);
    }
    let tree = OrgTree::new(&state.units)?;
    let mut filter = BTreeSet::new();
    for org_unit_id in org_unit_ids {
        let expanded = tree.expand(org_unit_id, include_descendants)?;
        if expanded.is_empty() {
            return Err(Error::ValidationError("组织筛选目标不存在或已停用".into()));
        }
        filter.extend(expanded);
    }
    Ok(Some(filter))
}

fn org_label(state: &OrganizationState, primary_org: Option<&str>) -> Option<String> {
    let org_id = primary_org?;
    state
        .units
        .iter()
        .find(|unit| unit.base.id == org_id && unit.enabled && !unit.base.is_deleted())
        .map(|unit| unit.name.clone())
}

/// 调用方带回的目录版本与当前快照不一致时拒绝，未带版本则不比较。
fn ensure_current_version(requested: &Option<String>, current: &str) -> Result<()> {
    if requested.as_ref().is_some_and(|version| version != current) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into()));
    }
    Ok(())
}

/// 填充本页人员组织标签并返回独立目录快照。
fn page_from_result(
    result: DirectoryResult,
    page: u64,
    page_size: u32,
    authorized: &AuthorizedDirectory,
    version: String,
) -> Result<PersonDirectoryPage> {
    let total = result.totals.first().map_or(0, |row| row.count);
    let mut items = result.items;
    for item in &mut items {
        let org = authorized.facts.organizations.own_org(&item.id, authorized.facts.as_of)?;
        item.org_label = org_label(&authorized.facts.organizations, org);
        item.primary_org_unit_id = org.filter(|_| item.org_label.is_some()).map(ToOwned::to_owned);
    }
    Ok(PersonDirectoryPage::from_page(
        PageView { items, total, page, page_size },
        version,
        authorized.facts.policy_version,
        authorized.facts.organizations.version,
        authorized.facts.as_of.as_utc().to_rfc3339(),
        (!authorized.facts.scope.has_role_scope()).then_some("no_scope"),
    ))
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::dto::PersonDirectoryItem;
    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::account_core::AccountStatus;
    use crate::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind, OrgValidity};
    use crate::repository::person_directory_query::version_fixture;

    #[test]
    fn directory_returns_effective_primary_org_and_clears_expired_membership() {
        let unit = OrgUnit::new(
            "org".into(),
            "销售部".into(),
            None,
            OrgUnitKind::Department,
            "admin".into(),
            "初始化".into(),
        )
        .unwrap();
        let membership = OrgMembership {
            base: BaseModel::fake(),
            user_id: "sales".into(),
            org_unit_id: "org".into(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(1),
                valid_to: Some(Instant::from_unix_secs(10)),
            },
            changed_by: "admin".into(),
            reason: "入职".into(),
        };
        for (at, enabled, expected) in [(5, true, Some("org")), (10, true, None), (5, false, None)] {
            let mut unit = unit.clone();
            unit.enabled = enabled;
            let authorized = AuthorizedDirectory {
                facts: DataScopeFacts {
                    resource: "person".into(),
                    action: "list".into(),
                    scope: ResolvedScope {
                        role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                        user_limit: None,
                    },
                    organizations: OrganizationState {
                        units: vec![unit.clone()],
                        memberships: vec![membership.clone()],
                        ..Default::default()
                    },
                    policy_version: 1,
                    scope_version: "scope".into(),
                    as_of: Instant::from_unix_secs(at),
                },
                actor_id: "admin".into(),
            };
            let result = DirectoryResult {
                items: vec![PersonDirectoryItem {
                    id: "sales".into(),
                    name: "销售员".into(),
                    account: "sales".into(),
                    status: AccountStatus::Active,
                    org_label: None,
                    primary_org_unit_id: None,
                }],
                ..Default::default()
            };
            let page = page_from_result(result, 1, 20, &authorized, "scope".into()).unwrap();
            assert_eq!(page.items[0].primary_org_unit_id.as_deref(), expected);
            assert_eq!(page.items[0].org_label.as_deref(), expected.map(|_| "销售部"));
        }
    }

    #[test]
    fn changed_display_or_empty_authorization_invalidates_previous_version() {
        let result = version_fixture("甲", "active");
        let old = content_version("scope", &result);
        assert_ne!(old, content_version("scope", &version_fixture("乙", "active")));
        assert_ne!(old, content_version("scope", &version_fixture("甲", "suspended")));
        let empty = content_version("no-scope", &DirectoryResult::default());
        assert!(matches!(
            ensure_current_version(&Some(old.clone()), &empty),
            Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")
        ));
        assert!(ensure_current_version(&Some(old.clone()), &old).is_ok());
        assert!(ensure_current_version(&None, &old).is_ok());
    }

    /// 目录内容按人员事实集合编码，返回顺序变化不造成版本漂移。
    #[test]
    fn directory_versions_ignore_row_order_but_bind_scope() {
        let mut result = version_fixture("甲", "active");
        result.versions.extend(version_fixture("乙", "active").versions);
        let version = content_version("scope", &result);
        result.versions.reverse();
        assert_eq!(version, content_version("scope", &result));
        assert_ne!(version, content_version("other-scope", &result));
        result.versions.pop();
        assert_ne!(version, content_version("scope", &result));
    }

    #[test]
    fn selected_resolution_is_limited_to_requested_ids_and_user_ceiling() {
        let mut authorized = AuthorizedDirectory {
            facts: DataScopeFacts {
                resource: "person".into(),
                action: "list".into(),
                scope: ResolvedScope {
                    role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                    user_limit: None,
                },
                organizations: OrganizationState::default(),
                policy_version: 1,
                scope_version: "scope".into(),
                as_of: Instant::from_unix_secs(1),
            },
            actor_id: "a".into(),
        };
        assert_eq!(scope_ids(&authorized, None, None).unwrap(), None);
        let selected = vec!["b".into()];
        assert_eq!(scope_ids(&authorized, None, Some(&selected)).unwrap(), Some(selected.clone()));
        authorized.facts.scope.user_limit = Some(ScopeClause { self_owned: true, ..Default::default() });
        assert_eq!(scope_ids(&authorized, None, Some(&selected)).unwrap(), Some(vec![]));
        assert_eq!(scope_ids(&authorized, None, None).unwrap(), Some(vec!["a".into()]));
        authorized.facts.scope.role_clauses.clear();
        assert_eq!(scope_ids(&authorized, None, None).unwrap(), Some(vec![]));
    }
}
