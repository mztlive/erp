//! 选品授权快照：列表、候选、详情重验与写命令复用同一范围。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::{AuditActor, FilterOption};
use erp_identity::AccessControlExt;
use erp_sales::dto::sales_selection::{
    CreateSalesSelectionBookletRequest, DeleteDisplayItemRequest, PrepareSalesSelectionRequest,
    PublishSalesSelectionRequest, SalesSelectionBookletListParams, SalesSelectionBookletView,
    SalesSelectionCommandRequest, SalesSelectionProposalListParams, SalesSelectionSessionView,
};
use erp_sales::entity::sales_selection::{FirstNonEmptyMemberImage, LinkTokenCrypto};
use erp_sales::service::sales_selection::{SalesSelectionService, SelectionAccess};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use storage::S3Storage;
use validator::Validate;

use super::adapters::{CatalogAdapter, ImageAdapter};
use super::{SelectionBookletListView, SelectionProposalListView};
use crate::{Error, Result};

/// 校验创建责任在创建动作范围内；提交人不成为负责人。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `actor` - 已认证操作人
/// * `owner` - 拟写入的显式销售负责人
/// * `org` - 拟写入的业务组织
/// * `executor` - 调用方执行器
///
/// # 返回
/// 范围允许时成功。
///
/// # 错误
/// 无动作权限或责任不在范围内时拒绝。
pub(crate) async fn check_create_scope(
    access: &SelectionAccess,
    actor: &AuditActor,
    owner: &str,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if owner.trim().is_empty() || org.trim().is_empty() {
        return Err(Error::ValidationError("销售负责人与业务组织必填".into()));
    }
    let (resolved, scope) = access.resolve(actor, "sales_selection_booklet", "create", executor).await?;
    if !access.allows_new(&resolved, &scope, owner, org)? {
        return Err(Error::Forbidden("拟创建责任不在数据范围内".into()));
    }
    Ok(())
}

/// 选品册列表授权快照；同一事务内授权、计数与候选。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `db` - 选品集合所在数据库
/// * `params` - 列表筛选
/// * `actor` - 已认证操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回分页、候选与范围版本。
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
    let (resolved, scope) = access.resolve(actor, "sales_selection_booklet", "list", executor).await?;
    ensure_version(params.scope_version.as_deref(), &resolved.scope_version)?;
    let orgs =
        expanded_orgs(access, params.org_unit_ids.as_deref(), params.include_descendants, executor).await?;
    let mut effective = params.clone();
    effective.org_unit_ids = orgs;
    let service = SalesSelectionService::new(db.clone());
    let page = service.list_booklets_with(&effective, &scope, executor).await?;
    let owners = distinct_owners(db, true, &scope, executor).await?;
    let options = db.accounts().filter_options(&owners, executor).await?;
    Ok(SelectionBookletListView {
        page,
        owner_options: options,
        scope_version: resolved.scope_version,
        policy_version: resolved.policy_version,
        organization_version: resolved.organization_version,
        no_scope: scope.is_empty(),
    })
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
/// 返回分页、候选与范围版本。
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
    let (resolved, scope) = access.resolve(actor, "sales_selection_proposal", "list", executor).await?;
    ensure_version(params.scope_version.as_deref(), &resolved.scope_version)?;
    let orgs =
        expanded_orgs(access, params.org_unit_ids.as_deref(), params.include_descendants, executor).await?;
    let mut effective = params.clone();
    effective.org_unit_ids = orgs;
    let service = SalesSelectionService::new(db.clone());
    let page = service.proposal_list_with(&effective, &scope, executor).await?;
    let owners = distinct_owners(db, false, &scope, executor).await?;
    let options = db.accounts().filter_options(&owners, executor).await?;
    Ok(SelectionProposalListView {
        page,
        owner_options: options,
        scope_version: resolved.scope_version,
        policy_version: resolved.policy_version,
        organization_version: resolved.organization_version,
        no_scope: scope.is_empty(),
    })
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
pub(crate) async fn owner_display_names(db: &Database, ids: &[String]) -> Result<HashMap<String, String>> {
    Ok(db.accounts().names_by_ids(ids, &mut NoTransaction).await?)
}

/// 授权后启动准备；授权与写入分两次事务，先鉴权后执行。
///
/// # 参数
/// * `db` - 选品数据库
/// * `storage` - 对象存储
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `req` - 准备命令
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回准备中详情。
///
/// # 错误
/// 无权操作、状态或版本非法时拒绝。
pub(crate) async fn start_prepare_checked(
    db: &Database,
    storage: &S3Storage,
    access: SelectionAccess,
    id: String,
    req: PrepareSalesSelectionRequest,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    require_action(db, &access, &actor, "prepare", &id).await?;
    let mut command = req;
    command.booklet_id = id;
    let catalog = CatalogAdapter { db: db.clone() };
    let images = ImageAdapter { db: db.clone(), storage: Arc::new(storage.clone()) };
    let actor_id = actor.id().to_string();
    let owned = db.clone();
    Ok(SalesSelectionService::new(owned)
        .start_prepare(command, &actor_id, &catalog, &images, &FirstNonEmptyMemberImage)
        .await?)
}

/// 授权后删除陈列。
///
/// # 参数
/// * `db` - 选品数据库
/// * `access` - 选品范围访问器
/// * `booklet_id` - 选品册
/// * `item_id` - 陈列
/// * `req` - 版本
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回详情。
///
/// # 错误
/// 无权操作、状态或版本非法时拒绝。
pub(crate) async fn delete_item_checked(
    db: &Database,
    access: SelectionAccess,
    booklet_id: String,
    item_id: String,
    req: DeleteDisplayItemRequest,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    require_action(db, &access, &actor, "maintain", &booklet_id).await?;
    let actor_id = actor.id().to_string();
    Ok(SalesSelectionService::new(db.clone())
        .delete_display_item(&booklet_id, &item_id, req.expected_version, &actor_id)
        .await?)
}

/// 授权后发布。
///
/// # 参数
/// * `db` - 选品数据库
/// * `storage` - 对象存储
/// * `crypto` - 链接编解码器
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `req` - 发布命令
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回含链接的详情。
///
/// # 错误
/// 无权操作、复核失败或状态不允许时拒绝。
pub(crate) async fn publish_checked(
    db: &Database,
    storage: &S3Storage,
    crypto: &LinkTokenCrypto,
    access: SelectionAccess,
    id: String,
    req: PublishSalesSelectionRequest,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    let _ = storage;
    require_action(db, &access, &actor, "publish", &id).await?;
    let catalog = CatalogAdapter { db: db.clone() };
    let actor_id = actor.id().to_string();
    Ok(SalesSelectionService::new(db.clone()).publish(&id, req, &actor_id, &catalog, crypto).await?)
}

/// 授权后复制链接详情。
///
/// # 参数
/// * `db` - 选品数据库
/// * `crypto` - 链接编解码器
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回含路径的详情。
///
/// # 错误
/// 无链接或无权操作时拒绝。
pub(crate) async fn copy_link_checked(
    db: &Database,
    crypto: &LinkTokenCrypto,
    access: SelectionAccess,
    id: &str,
    actor: &AuditActor,
) -> Result<SalesSelectionBookletView> {
    require_action(db, &access, actor, "copy_link", id).await?;
    let service = SalesSelectionService::new(db.clone());
    let token = service.copy_link(id, crypto).await?;
    let mut view = service.booklet_detail(id).await?;
    let path = format!("/s/{token}");
    view.public_path = Some(path.clone());
    view.public_url = Some(path);
    Ok(view)
}

/// 授权后读取内部会话。
///
/// # 参数
/// * `db` - 选品数据库
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回当前会话。
///
/// # 错误
/// 无会话或无权查看时拒绝。
pub(crate) async fn session_checked(
    db: &Database,
    access: SelectionAccess,
    id: &str,
    actor: &AuditActor,
) -> Result<SalesSelectionSessionView> {
    let owned = db.clone();
    let current = id.to_string();
    let cloned = actor.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            let access = access.clone();
            let owned = owned.clone();
            let current = current.clone();
            let cloned = cloned.clone();
            Box::pin(async move {
                access.require_booklet(&cloned, "get", &current, executor).await?;
                SalesSelectionService::new(owned).session_of(&current, executor).await.map_err(Error::from)
            })
        })
        .await
}

/// 授权后更换链接。
///
/// # 参数
/// * `db` - 选品数据库
/// * `crypto` - 链接编解码器
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `req` - 命令
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回新链接详情。
///
/// # 错误
/// 无权操作、状态或版本非法时拒绝。
pub(crate) async fn rotate_link_checked(
    db: &Database,
    crypto: &LinkTokenCrypto,
    access: SelectionAccess,
    id: String,
    req: SalesSelectionCommandRequest,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    require_action(db, &access, &actor, "rotate_link", &id).await?;
    let actor_id = actor.id().to_string();
    Ok(SalesSelectionService::new(db.clone()).rotate_link(&id, req, &actor_id, crypto).await?)
}

/// 授权后读取册图片对象键；同一事务内重验。
///
/// # 参数
/// * `db` - 选品数据库
/// * `access` - 选品范围访问器
/// * `id` - 选品册
/// * `asset_id` - 资产
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回快照对象键。
///
/// # 错误
/// 资产不属于本册或无权查看时拒绝。
pub(crate) async fn image_key_checked(
    db: &Database,
    access: SelectionAccess,
    id: &str,
    asset_id: &str,
    actor: &AuditActor,
) -> Result<String> {
    let owned = db.clone();
    let current = id.to_string();
    let asset = asset_id.to_string();
    let cloned = actor.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            let access = access.clone();
            let owned = owned.clone();
            let current = current.clone();
            let asset = asset.clone();
            let cloned = cloned.clone();
            Box::pin(async move {
                access.require_booklet(&cloned, "get", &current, executor).await?;
                SalesSelectionService::new(owned)
                    .image_key_of(&current, &asset, executor)
                    .await
                    .map_err(Error::from)
            })
        })
        .await
}

/// 在调用方事务外证明单对象动作；写命令前必须调用。
///
/// # 参数
/// * `db` - 选品数据库
/// * `access` - 选品范围访问器
/// * `actor` - 已认证操作人
/// * `action` - 已注册动作
/// * `id` - 选品册
///
/// # 返回
/// 授权通过时成功。
///
/// # 错误
/// 不可见对象返回未找到。
async fn require_action(
    db: &Database,
    access: &SelectionAccess,
    actor: &AuditActor,
    action: &str,
    id: &str,
) -> Result<()> {
    let owned = db.clone();
    let current = id.to_string();
    let cloned = actor.clone();
    let requested = action.to_string();
    owned
        .client()
        .clone()
        .with_transaction(move |executor| {
            let access = access.clone();
            let cloned = cloned.clone();
            let current = current.clone();
            let requested = requested.clone();
            Box::pin(async move {
                access.require_booklet(&cloned, &requested, &current, executor).await?;
                Ok::<(), erp_sales::Error>(())
            })
        })
        .await?;
    Ok(())
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

/// 同一快照内去重负责人；候选不授予命令资格。
///
/// # 参数
/// * `db` - 选品数据库
/// * `booklet` - 为 true 查册，否则查方案
/// * `scope` - 已解析责任条件
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回排序去重后的负责人 ID。
///
/// # 错误
/// 去重查询失败时拒绝。
async fn distinct_owners(
    db: &Database,
    booklet: bool,
    scope: &erp_sales::repository::sales_selection::SelectionReadScope,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
    let mut ids =
        SalesSelectionDomainRepository::new(db).distinct_owner_ids(booklet, scope, executor).await?;
    ids.sort();
    ids.dedup();
    if ids.len() > 10_000 {
        return Err(Error::ValidationError("负责人候选超过查询上限".into()));
    }
    Ok(ids)
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
    if page > 1 && version.is_none_or(str::is_empty) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：请从第一页刷新后继续查询".into()));
    }
    Ok(())
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
    if expected.is_some_and(|value| value != actual) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into()));
    }
    Ok(())
}

/// 创建请求必须携带显式责任；过渡期兼容缺字段的旧幂等键。
///
/// # 参数
/// * `req` - 创建请求
///
/// # 返回
/// 显式责任齐全时返回原请求。
///
/// # 错误
/// 缺负责人或组织时拒绝。
#[allow(dead_code)]
fn ensure_create_request(
    req: CreateSalesSelectionBookletRequest,
) -> Result<CreateSalesSelectionBookletRequest> {
    if req.sales_owner_user_id.trim().is_empty() || req.business_org_unit_id.trim().is_empty() {
        return Err(Error::ValidationError("销售负责人与业务组织必填".into()));
    }
    Ok(req)
}

/// 构造候选显示标签；只含 ID 与显示名，不回全量身份。
///
/// # 参数
/// * `ids` - 负责人 ID
/// * `names` - 显示名映射
///
/// # 返回
/// 返回稳定排序的候选。
///
/// # 错误
/// 无。
#[allow(dead_code)]
fn filter_options(ids: &[String], names: &HashMap<String, String>) -> Vec<FilterOption> {
    let mut options: Vec<FilterOption> = ids
        .iter()
        .map(|id| FilterOption {
            value: id.clone(),
            label: names.get(id).cloned().unwrap_or_else(|| id.clone()),
        })
        .collect();
    options.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.value.cmp(&b.value)));
    options
}
