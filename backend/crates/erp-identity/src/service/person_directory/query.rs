//! 人员目录读取：授权、资格和组织在调用方事务内一次完成。

use std::collections::BTreeSet;

use application_core::{AuditActor, PageView};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;
use serde_json::to_vec;

use crate::dto::PersonDirectoryPage;
use crate::entity::access_control::ResolvedScope;
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::entity::person_directory::{
    DirectoryListRequest, PersonDirectoryCategory, candidate_in_directory, directory_org_ids,
};
use crate::repository::person_directory_query::{
    DIRECTORY_LIMIT, DirectoryRead, DirectoryResult, directory_page,
};
use crate::repository::OrganizationRepository;
use crate::service::access_control::resolve::DataScopeService;
use crate::service::iam::SharedRbacService;
use crate::{Error, Result};

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
    let org_filter =
        expand_org_filter(&authorized.organizations, &request.org_unit_ids, request.include_descendants)?;
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
    let version = format!("{:x}", md5::compute(format!(
        "{display}:{:?}:{org_filter:?}", request.search,
    )));
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
    scope: ResolvedScope,
    organizations: OrganizationState,
    policy_version: u64,
    scope_version: String,
    as_of: Instant,
    actor_id: String,
}

async fn authorize(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    executor: &mut dyn Executor,
) -> Result<AuthorizedDirectory> {
    let resolved = DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, category.resource(), "list", executor)
        .await?;
    Ok(AuthorizedDirectory {
        scope: resolved.scope,
        organizations: resolved.organizations,
        policy_version: resolved.policy_version,
        scope_version: resolved.scope_version,
        as_of: resolved.as_of,
        actor_id: actor.id().to_string(),
    })
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
    let orgs = directory_org_ids(&authorized.scope, org_filter).into_iter().collect::<Vec<_>>();
    let rows = if let Some(ids) = selected {
        repository.directory_memberships(Some(ids), None, authorized.as_of, executor).await?
    } else {
        repository.directory_memberships(None, Some(&orgs), authorized.as_of, executor).await?
    };
    let mut seen = authorized.organizations.memberships.iter()
        .map(|row| row.base.id.clone()).collect::<BTreeSet<_>>();
    for row in rows {
        if seen.insert(row.base.id.clone()) {
            authorized.organizations.memberships.push(row);
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
    let ids = result.versions.iter().map(|row| {
        row.get_str("id").map(str::to_owned)
            .map_err(|_| Error::Internal("人员目录版本索引缺少身份".into()))
    }).collect::<Result<Vec<_>>>()?;
    authorized.organizations.memberships = OrganizationRepository::new(db)
        .directory_memberships(Some(&ids), None, authorized.as_of, executor).await?;
    let content = content_version(&authorized.scope_version, result)?;
    let relations = authorized.organizations.memberships.iter()
        .map(|row| (&row.user_id, &row.org_unit_id, &row.base.id, row.base.version))
        .collect::<Vec<_>>();
    Ok(format!("{:x}", md5::compute(format!("{content}:{relations:?}"))))
}

/// 将组织状态转为有界的人员范围索引；公司无个人上限时不展开全部账号。
fn scope_ids(
    authorized: &AuthorizedDirectory,
    org_filter: Option<&BTreeSet<String>>,
    selected: Option<&[String]>,
) -> Result<Option<Vec<String>>> {
    let scope = &authorized.scope;
    if selected.is_none()
        && org_filter.is_none()
        && scope.role_clauses.iter().any(|clause| clause.company)
        && scope.user_limit.as_ref().is_none_or(|limit| limit.company)
    {
        return Ok(None);
    }
    let candidates = selected.map(|ids| ids.iter().cloned().collect::<BTreeSet<_>>()).unwrap_or_else(|| {
        let mut ids =
            authorized.organizations.memberships.iter().map(|m| m.user_id.clone()).collect::<BTreeSet<_>>();
        ids.insert(authorized.actor_id.clone());
        ids
    });
    let mut ids = Vec::new();
    for id in candidates {
        let org = authorized.organizations.own_org(&id, authorized.as_of)?;
        if candidate_in_directory(scope, &authorized.actor_id, &id, org, org_filter) {
            ids.push(id);
        }
        if ids.len() > DIRECTORY_LIMIT {
            return Err(Error::ValidationError("人员范围超过上限，请收窄组织条件".into()));
        }
    }
    Ok(Some(ids))
}

/// 内容索引包含账号显示信息和资格版本，不用人数代替目录版本。
fn content_version(scope: &str, result: &DirectoryResult) -> Result<String> {
    let mut hash = md5::Context::new();
    hash.consume(scope.as_bytes());
    for row in &result.versions {
        hash.consume(to_vec(row).map_err(|error| Error::Internal(error.to_string()))?);
    }
    Ok(format!("{:x}", hash.finalize()))
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
        .find(|unit| unit.base.id == org_id && !unit.base.is_deleted())
        .map(|unit| unit.name.clone())
}

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
        let org = authorized.organizations.own_org(&item.id, authorized.as_of)?;
        item.org_label = org_label(&authorized.organizations, org);
    }
    Ok(PersonDirectoryPage::from_page(
        PageView { items, total, page, page_size },
        version,
        authorized.policy_version,
        authorized.organizations.version,
        authorized.as_of.as_utc().to_rfc3339(),
        (!authorized.scope.has_role_scope()).then_some("no_scope"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::access_control::ScopeClause;
    use crate::repository::person_directory_query::version_fixture;

    #[test]
    fn changed_display_or_empty_authorization_invalidates_previous_version() {
        let result = version_fixture("甲", "active");
        let old = content_version("scope", &result).unwrap();
        assert_ne!(old, content_version("scope", &version_fixture("乙", "active")).unwrap());
        assert_ne!(old, content_version("scope", &version_fixture("甲", "suspended")).unwrap());
        let empty = content_version("no-scope", &DirectoryResult::default()).unwrap();
        assert!(ensure_current_version(&Some(old), &empty).is_err());
    }

    #[test]
    fn selected_resolution_is_limited_to_requested_ids_and_user_ceiling() {
        let mut authorized = AuthorizedDirectory {
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "scope".into(),
            as_of: Instant::from_unix_secs(1),
            actor_id: "a".into(),
        };
        assert_eq!(scope_ids(&authorized, None, None).unwrap(), None);
        let selected = vec!["b".into()];
        assert_eq!(scope_ids(&authorized, None, Some(&selected)).unwrap(), Some(selected.clone()));
        authorized.scope.user_limit = Some(ScopeClause { self_owned: true, ..Default::default() });
        assert_eq!(scope_ids(&authorized, None, Some(&selected)).unwrap(), Some(vec![]));
        assert_eq!(scope_ids(&authorized, None, None).unwrap(), Some(vec!["a".into()]));
        authorized.scope.role_clauses.clear();
        assert_eq!(scope_ids(&authorized, None, None).unwrap(), Some(vec![]));
    }
}
