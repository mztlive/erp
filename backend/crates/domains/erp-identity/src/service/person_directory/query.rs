//! 人员目录查询、分页和交付复验均使用同一次统一授权与原事务。

use std::collections::BTreeSet;

use application_core::{AuditActor, PageView};
use erp_core::common::time::Instant;
use erp_permissions::{Permission, access_control};
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::PersonDirectoryPage;
use crate::entity::fingerprint::Fingerprint;
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::entity::person_directory::{DirectoryListRequest, PersonDirectoryCategory};
use crate::repository::OrganizationRepository;
use crate::repository::fingerprint::BsonFingerprint;
use crate::repository::person_directory_query::{DirectoryRead, DirectoryResult, directory_page};
use crate::{AuthorizationService, CompiledAccess, Error, Result};

struct DirectoryDisplay {
    organizations: OrganizationState,
    policy_version: u64,
    scope_version: String,
    as_of: Instant,
    no_scope: bool,
}

/// 使用中央查询条件在数据库分页前过滤人员目录。
/// 满足完整授权及业务筛选的目录页、总数与范围版本。
///
/// 无权限、非法筛选、超限、版本变化或读取失败时拒绝。
///
/// db、authorization 为共享设施；actor、category、request 为当前身份和目录筛选；executor 为原快照。
pub async fn list_page(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    request: &DirectoryListRequest,
    executor: &mut dyn Executor,
) -> Result<PersonDirectoryPage> {
    let (access, mut display) = authorize(db, authorization, actor, category, executor).await?;
    let org_filter =
        expand_org_filter(&display.organizations, &request.org_unit_ids, request.include_descendants)?;
    let orgs = org_filter.as_ref().map(|ids| ids.iter().cloned().collect::<Vec<_>>());
    let result = directory_page(
        db,
        &DirectoryRead {
            category,
            org_unit_ids: orgs.as_deref(),
            selected_ids: None,
            search: request.search.as_deref(),
            page: request.page,
            page_size: request.page_size,
        },
        &access,
        executor,
    )
    .await?;

    let content = display_version(db, &mut display, &result, executor).await?;
    let mut version = Fingerprint::new("person-directory-query-v2");
    version.text(&content);
    version.optional(request.search.as_deref(), Fingerprint::text);
    version.optional(org_filter.as_ref(), |hash, ids| hash.unordered(ids, |hash, id| hash.text(id)));
    let version = version.finish();
    ensure_current_version(&request.scope_version, &version)?;

    let page = page_from_result(result, request.page, request.page_size, &display, version)?;
    authorization.validate(&access, executor).await?;
    Ok(page)
}

/// 已选账号只收窄同一目录查询，不以请求 ID 代替中央授权条件。
/// 当前仍可读且满足目录资格的人员及授权元信息。
///
/// 身份、动作无权、输入超限、依赖到期或读取失败时拒绝。
///
/// db、authorization 为共享设施；actor、category、ids 为当前身份和已选完整集合；executor 为原快照。
pub async fn selected_page(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<PersonDirectoryPage> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(Error::ValidationError("已选人员须为 1 到 100 项".into()));
    }
    let page_size = ids.len() as u32;

    let (access, mut display) = authorize(db, authorization, actor, category, executor).await?;
    let result = directory_page(
        db,
        &DirectoryRead {
            category,
            org_unit_ids: None,
            selected_ids: Some(ids),
            search: None,
            page: 1,
            page_size,
        },
        &access,
        executor,
    )
    .await?;

    let version = display_version(db, &mut display, &result, executor).await?;
    let page = page_from_result(result, 1, page_size, &display, version)?;
    authorization.validate(&access, executor).await?;
    Ok(page)
}

// 在目录读取开始时统一解析功能资格和数据范围；展示需要的组织树也沿该快照读取。
async fn authorize(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    executor: &mut dyn Executor,
) -> Result<(CompiledAccess, DirectoryDisplay)> {
    let access = authorization.query(actor, &permission(category), executor).await?;
    let repository = OrganizationRepository::new(db);
    let organizations = OrganizationState {
        version: repository.revision(executor).await?.map_or(0, |row| row.revision),
        units: repository.units(executor).await?,
        ..Default::default()
    };
    let display = DirectoryDisplay {
        organizations,
        policy_version: access.policy_version(),
        scope_version: access.version(),
        as_of: access.as_of(),
        no_scope: !access.has_role_scope(),
    };
    Ok((access, display))
}

// 根据固定入口选择对应人员类别的公共权限常量，不接受调用方自选权限。
fn permission(category: PersonDirectoryCategory) -> Permission {
    match category {
        PersonDirectoryCategory::Sales => access_control::PERSON_DIRECTORY_LIST_SALESPEOPLE,
        PersonDirectoryCategory::Procurement => access_control::PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE,
        PersonDirectoryCategory::Business => access_control::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
    }
}

// 为已经授权的目录版本计算补充标签，只读取准入成员所需的展示信息，不扩大成员集合。
async fn display_version(
    db: &Database,
    display: &mut DirectoryDisplay,
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
    display.organizations.memberships = OrganizationRepository::new(db)
        .directory_memberships(Some(&ids), None, display.as_of, executor)
        .await?;

    let mut version = Fingerprint::new("person-directory-display-v2");
    version.text(&content_version(&display.scope_version, result));
    version.unordered(&display.organizations.memberships, |hash, row| {
        hash.text(&row.user_id);
        hash.text(&row.org_unit_id);
        hash.text(&row.base.id);
        hash.number(row.base.version);
    });
    Ok(version.finish())
}

// 目录分页内容版本纳入账号展示字段及资格版本，不能用结果人数代替实际变化摘要。
fn content_version(scope: &str, result: &DirectoryResult) -> String {
    let mut hash = Fingerprint::new("person-directory-content-v2");
    hash.text(scope);
    hash.unordered(&result.versions, Fingerprint::document);
    hash.finish()
}

// 按组织树展开业务筛选，用于收窄目录查询；完整访问范围仍由原 `CompiledAccess` 提供。
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

// 跨页读取人员目录时核对客户端版本与当前内容，旧版本拒绝整页以免拼接不同快照结果。
fn ensure_current_version(requested: &Option<String>, current: &str) -> Result<()> {
    if requested.as_ref().is_some_and(|version| version != current) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into()));
    }
    Ok(())
}

// 将已在数据库完成资格授权和分页的成员投影为目录响应，只在原读取快照补显示信息。
fn page_from_result(
    result: DirectoryResult,
    page: u64,
    page_size: u32,
    display: &DirectoryDisplay,
    version: String,
) -> Result<PersonDirectoryPage> {
    let total = result.totals.first().map_or(0, |row| row.count);
    let mut items = result.items;
    for item in &mut items {
        let org = display.organizations.own_org(&item.id, display.as_of)?;
        item.org_label = org_label(&display.organizations, org);
        item.primary_org_unit_id = org.filter(|_| item.org_label.is_some()).map(ToOwned::to_owned);
    }
    Ok(PersonDirectoryPage::from_page(
        PageView { items, total, page, page_size },
        version,
        display.policy_version,
        display.organizations.version,
        display.as_of.as_utc().to_rfc3339(),
        display.no_scope.then_some("no_scope"),
    ))
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::dto::PersonDirectoryItem;
    use crate::entity::account_core::AccountStatus;
    use crate::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind, OrgValidity};
    use crate::repository::person_directory_query::version_fixture;

    #[test]
    fn empty_directory_keeps_scope_absence_separate_from_empty_membership() {
        let mut display = DirectoryDisplay {
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "scope".into(),
            as_of: Instant::from_unix_secs(1),
            no_scope: false,
        };
        let empty = page_from_result(DirectoryResult::default(), 1, 20, &display, "scope".into()).unwrap();
        assert_eq!(empty.empty_reason, None);

        display.no_scope = true;
        let denied = page_from_result(DirectoryResult::default(), 1, 20, &display, "scope".into()).unwrap();
        assert_eq!(denied.empty_reason, Some("no_scope"));
    }

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
            let authorized = DirectoryDisplay {
                organizations: OrganizationState {
                    units: vec![unit.clone()],
                    memberships: vec![membership.clone()],
                    ..Default::default()
                },
                policy_version: 1,
                scope_version: "scope".into(),
                as_of: Instant::from_unix_secs(at),
                no_scope: false,
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

    // 验证：目录内容按人员事实集合编码，返回顺序变化不造成版本漂移。
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
}
