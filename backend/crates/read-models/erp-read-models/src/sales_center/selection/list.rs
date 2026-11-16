//! 同一授权快照内查询分页并补负责人名称。
use std::collections::HashMap;

use application_core::{AuditActor, EmptyScopeVersion, PageView, ScopeVersion};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_permissions::sales_selection;
use erp_sales::dto::sales_selection::{SalesSelectionBookletListParams, SalesSelectionProposalListParams};
use erp_sales::service::sales_selection::{SalesSelectionService, SelectionAccess};
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use super::{SelectionBookletListView, SelectionProposalListView};
use crate::{Error, Result};
/// 选品册列表授权快照；同一执行器内授权与计数。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `db` - 选品集合所在数据库
/// * `params` - 列表筛选
/// * `actor` - 已认证操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回分页与范围版本；当前页行带负责人显示名。
///
/// # 错误
/// 参数非法、无动作权限或范围变化时拒绝。
pub(crate) async fn booklet_list_snapshot(
    access: &SelectionAccess,
    db: &Database,
    params: &SalesSelectionBookletListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SelectionBookletListView> {
    ensure_page(params.page.unwrap_or(1), params.scope_version.as_deref())?;
    params.validate()?;
    let scope = access.resolve(actor, &sales_selection::BOOKLET_LIST, executor).await?;
    ensure_version(params.scope_version.as_deref(), &scope.version())?;
    let orgs =
        expanded_orgs(access, params.org_unit_ids.as_deref(), params.include_descendants, executor).await?;
    let mut effective = params.clone();
    effective.org_unit_ids = orgs;
    let service = SalesSelectionService::new(db.clone(), access.authorization());
    let page = service.list_booklets_with(&effective, &scope, executor).await?;
    let page = named_owner_page(
        db,
        page,
        |item| item.sales_owner_user_id.clone(),
        |item, sales_owner_name| super::SelectionBookletListRow { item, sales_owner_name },
        executor,
    )
    .await?;
    scope.validate(executor).await?;
    Ok(SelectionBookletListView { page, scope_version: scope.version(), no_scope: false })
}

/// 方案列表授权快照；方案沿所属册责任解释。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `db` - 方案集合所在数据库
/// * `params` - 列表筛选
/// * `actor` - 已认证操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回分页与范围版本；当前页行带负责人显示名。
///
/// # 错误
/// 参数非法、无动作权限或范围变化时拒绝。
pub(crate) async fn proposal_list_snapshot(
    access: &SelectionAccess,
    db: &Database,
    params: &SalesSelectionProposalListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SelectionProposalListView> {
    ensure_page(params.page.unwrap_or(1), params.scope_version.as_deref())?;
    params.validate()?;
    let scope = access.resolve(actor, &sales_selection::PROPOSAL_LIST, executor).await?;
    ensure_version(params.scope_version.as_deref(), &scope.version())?;
    let orgs =
        expanded_orgs(access, params.org_unit_ids.as_deref(), params.include_descendants, executor).await?;
    let mut effective = params.clone();
    effective.org_unit_ids = orgs;
    let service = SalesSelectionService::new(db.clone(), access.authorization());
    let page = service.proposal_list_with(&effective, &scope, executor).await?;
    let page = named_owner_page(
        db,
        page,
        |item| item.sales_owner_user_id.clone(),
        |item, sales_owner_name| super::SelectionProposalListRow { item, sales_owner_name },
        executor,
    )
    .await?;
    scope.validate(executor).await?;
    Ok(SelectionProposalListView { page, scope_version: scope.version(), no_scope: false })
}

/// 按 ID 批量读取负责人显示名；缺失账号省略。
///
/// # 参数
/// * `db` - 账号集合所在数据库
/// * `ids` - 负责人 ID
///
/// # 返回
/// 返回 ID 到显示名的映射。
///
/// # 错误
/// 仓储失败时拒绝。
pub(crate) async fn owner_display_names(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    Ok(db.accounts().names_by_ids(ids, executor).await?)
}

/// 展开组织筛选；包含下级时必须提供组织。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `orgs` - 请求组织
/// * `include` - 是否包含有效下级
/// * `executor` - 调用方执行器
///
/// # 返回
/// 无筛选时为 `None`，否则为展开后的组织。
///
/// # 错误
/// 包含下级缺少组织或未知组织时拒绝。
async fn expanded_orgs(
    access: &SelectionAccess,
    orgs: Option<&[String]>,
    include: Option<bool>,
    executor: &mut dyn Executor,
) -> Result<Option<Vec<String>>> {
    let Some(ids) = orgs else {
        if include == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(None);
    };
    if include == Some(true) && ids.is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = access.expand_org_units(ids, include.unwrap_or(false), executor).await?;
    Ok(Some(expanded.into_iter().collect()))
}

/// 给当前页行补负责人显示名；只读本页 ID，不生成筛选候选。
///
/// # 参数
/// * `db` - 账号集合所在数据库
/// * `page` - 已授权分页
/// * `owner_id` - 行上负责人 ID
/// * `map_row` - 组装带显示名的行
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回原分页与总数，行上附带显示名。
///
/// # 错误
/// 账号读取失败时拒绝。
async fn named_owner_page<T, R>(
    db: &Database,
    page: PageView<T>,
    owner_id: impl Fn(&T) -> String,
    map_row: impl Fn(T, Option<String>) -> R,
    executor: &mut dyn Executor,
) -> Result<PageView<R>> {
    let ids: Vec<String> = page.items.iter().map(&owner_id).collect();
    let names = db.accounts().names_by_ids(&ids, executor).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| {
            let sales_owner_name = names.get(&owner_id(&item)).cloned();
            map_row(item, sales_owner_name)
        })
        .collect();
    Ok(PageView { items, total: page.total, page: page.page, page_size: page.page_size })
}

/// 后续页必须携带当前范围版本。
///
/// # 参数
/// * `page` - 请求页码
/// * `version` - 客户端回传版本
///
/// # 返回
/// 第一页或版本非空时成功。
///
/// # 错误
/// 第二页缺版本时返回 `DATA_SCOPE_CHANGED`。
fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| Error::ConflictError(format!("DATA_SCOPE_CHANGED：{}", error.detail())))
}

/// 回传版本必须与当前快照一致。
///
/// # 参数
/// * `expected` - 客户端回传版本
/// * `actual` - 当前快照版本
///
/// # 返回
/// 未携带或一致时成功。
///
/// # 错误
/// 不一致时返回 `DATA_SCOPE_CHANGED`。
fn ensure_version(expected: Option<&str>, actual: &str) -> Result<()> {
    ScopeVersion::new(expected, EmptyScopeVersion::Provided)
        .verify(actual)
        .map_err(|error| Error::ConflictError(format!("DATA_SCOPE_CHANGED：{}", error.detail())))
}
