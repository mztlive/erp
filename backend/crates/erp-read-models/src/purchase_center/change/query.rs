use erp_procurement::repository::purchase_order::PurchaseChangeSearch;
use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use persistence_core::NoTransaction;
use serde::Serialize;
use std::hash::{Hash, Hasher};
use validator::Validate;

use super::super::dto::PurchaseChangeOrderView;
use super::super::PurchaseOrderReadService;
use super::mapping::change_list_view;
use crate::{Error, Result};
use application_core::{normalize_sort, AuditActor, PageView, SortDir};
use erp_procurement::dto::purchase_order::PurchaseChangeOrderListParams;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::Transactional;

/// 采购变更列表保持现有分页字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct PurchaseChangeListView {
    /// 分页结果。
    #[serde(flatten)]
    pub page: PageView<PurchaseChangeOrderView>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前采购变更范围口径摘要。
    pub scope_summary: &'static str,
}

impl PurchaseOrderReadService {
    /// 分页查询采购变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法
    /// * `ConflictError` - 跨页范围版本缺失或已变化
    /// * `Forbidden` - 没有列表动作权限
    ///
    /// # 关键业务约束
    /// 沿来源采购单当前负责人和业务组织接入；不得另建平行管理入口。
    pub async fn change_order_list(
        &self,
        params: &PurchaseChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeListView> {
        let expected = params.scope_version.as_deref();
        if params.page.unwrap_or(1) > 1 && expected.is_none_or(str::is_empty) {
            return Err(Error::ConflictError(
                "DATA_SCOPE_CHANGED：请从第一页刷新后继续查询".into(),
            ));
        }
        params.validate()?;
        let snapshot = self.change_list_snapshot(params, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(Error::ConflictError(
                "DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into(),
            ));
        }
        let current = self.change_list_snapshot(params, actor).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(Error::ConflictError(
                "DATA_SCOPE_CHANGED：数据范围或业务单据已变化，请刷新".into(),
            ));
        }
        Ok(snapshot)
    }

    /// 查询采购变更单详情。
    ///
    /// 返回统一只读审批结构；创建后未提交只返回绑定定义。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在或来源采购单不可见
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情凭证；沿来源采购单 detail 动作重验。
    pub async fn change_order_detail(&self, id: &str, actor: &AuditActor) -> Result<PurchaseChangeOrderView> {
        let this_access = self.access();
        let this_db = self.db.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let first = load_authorized_change(&this_db, &this_access, &actor, &id).await?;
        let current = load_authorized_change(&this_db, &this_access, &actor, &id).await?;
        if first.1 != current.1 {
            return Err(Error::ConflictError(
                "DATA_SCOPE_CHANGED：数据范围或采购变更单已变化，请刷新".into(),
            ));
        }
        let _ = self.load_change_binding(&id).await?;
        Ok(first.0)
    }

    /// 授权、总数与变更单版本全部在同一个事务读取。
    ///
    /// # 参数
    /// * `params` - 原始查询
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的列表快照。
    ///
    /// # 错误
    /// 范围变化、筛选非法或仓储失败时拒绝。
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果；缺范围保持空集。
    async fn change_list_snapshot(
        &self,
        params: &PurchaseChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeListView> {
        let db = self.db.clone();
        let access = self.access();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, scope) = access.resolve(&actor, "list", executor).await?;
                    let no_scope = scope.is_empty();
                    let authorized = access.authorized_source_ids(&scope, executor).await?;
                    let (_, sort_dir) = normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
                    let page = params.page.unwrap_or(1);
                    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
                    let purchase_order_id = normalized_filter(params.purchase_order_id.as_deref());
                    let status = normalized_filter(params.status.as_deref());
                    let result = db
                        .purchase_order()
                        .search_change_orders(
                            PurchaseChangeSearch {
                                purchase_order_id: purchase_order_id.as_deref(),
                                status: status.as_deref(),
                                authorized_purchase_order_ids: authorized.as_deref(),
                                page,
                                page_size,
                                sort_ascending: matches!(sort_dir, SortDir::Asc),
                            },
                            executor,
                        )
                        .await?;
                    let versions = db
                        .purchase_order()
                        .query_change_versions(
                            purchase_order_id.as_deref(),
                            status.as_deref(),
                            authorized.as_deref(),
                            executor,
                        )
                        .await?;
                    if versions.len() > 10_000 {
                        return Err(Error::ValidationError(
                            "采购变更查询超过上限，请收窄原采购单条件".into(),
                        ));
                    }
                    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
                    versions.hash(&mut fingerprint);
                    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
                    let views = result
                        .items
                        .into_iter()
                        .map(|change| change_list_view(change, None))
                        .collect();
                    Ok(PurchaseChangeListView {
                        page: PageView {
                            items: views,
                            total: result.total,
                            page,
                            page_size,
                        },
                        scope_version: context.scope_version,
                        policy_version: context.policy_version,
                        organization_version: context.organization_version,
                        as_of: context.as_of.as_utc().to_rfc3339(),
                        empty_reason: no_scope.then_some("no_scope"),
                        scope_summary: "采购变更单沿来源采购单当前负责人及单据业务组织范围",
                    })
                })
            })
            .await
    }

    /// 读取变更单创建时冻结的审批绑定。未注册时返回空绑定。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn load_change_binding(&self, id: &str) -> Result<Option<ApprovalDefinitionBinding>> {
        match find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => Ok(binding),
            Err(Error::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// 在独立事务中重验来源采购单后装配变更单详情。
///
/// # 参数
/// * `db` - 数据库
/// * `access` - 采购范围访问器
/// * `actor` - 已认证操作人
/// * `id` - 变更单主键
///
/// # 返回
/// 返回视图和范围版本绑定。
///
/// # 错误
/// 变更单不存在或来源采购单不可见时返回 NotFound。
///
/// # 关键业务约束
/// 不泄露越权变更单的存在性。
async fn load_authorized_change(
    db: &mongodb::Database,
    access: &super::super::access::PurchaseAccess,
    actor: &AuditActor,
    id: &str,
) -> Result<(PurchaseChangeOrderView, String)> {
    let actor = actor.clone();
    let id = id.to_string();
    let access = access.clone();
    let db = db.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                let change = db
                    .purchase_change_orders()
                    .find_by_id(&id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("采购变更单不存在或无权查看".to_string()))?;
                let (context, scope) = access.resolve(&actor, "detail", executor).await?;
                let order = db
                    .purchase_orders()
                    .find_authorized(change.purchase_order_id.as_ref(), &scope, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("采购变更单不存在或无权查看".to_string()))?;
                let binding = match find_approval_binding(&db, &id, executor)
                    .await
                    .map_err(crate::Error::from)
                {
                    Ok(binding) => binding,
                    Err(Error::NotFound(_)) => None,
                    Err(error) => return Err(error),
                };
                let approval = super::super::approval_query::load_change_document_approval(
                    &db,
                    &id,
                    binding.as_ref(),
                    change.stable.status,
                )
                .await?;
                let mut view = change_list_view(change, binding);
                view.approval = approval;
                let version = format!(
                    "{}:{}:{}",
                    context.scope_version, order.base.id, order.base.version
                );
                Ok((view, version))
            })
        })
        .await
}

/// 规范化可选列表筛选文本。
///
/// # 参数
/// * `value` - 原始可选筛选值
///
/// # 返回
/// 空白值返回 `None`，否则返回去除首尾空白后的字符串。
fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
