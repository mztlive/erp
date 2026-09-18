//! PurchaseReturnOrder 详情与分页视图装配。
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_returns::repository::ReturnsExt;
use erp_returns::repository::prelude::*;
use persistence_core::Transactional;
use serde::Serialize;
use validator::Validate;

use super::ReturnsReadService;
use super::dto::{PageView, PurchaseReturnOrderListParams, PurchaseReturnOrderView, SortDir};
use crate::{Error, Result};

/// 采购退货单列表筛选条件类型。
type PurchaseReturnOrderFilter = <mongodb::Database as ReturnsExt>::PurchaseReturnOrderFilter;

/// 采购退货列表保持现有分页字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct PurchaseReturnListView {
    /// 分页结果。
    #[serde(flatten)]
    pub page: PageView<PurchaseReturnOrderView>,
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
    /// 当前采购退货范围口径摘要。
    pub scope_summary: &'static str,
}

impl ReturnsReadService {
    /// 分页查询采购退货单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的分页视图。
    ///
    /// # 错误
    /// 跨页范围变化、筛选非法或仓储失败时拒绝。
    ///
    /// # 关键业务约束
    /// 沿来源采购单当前负责人和业务组织接入；不得另建平行管理入口。
    pub async fn purchase_return_order_list(
        &self,
        params: &PurchaseReturnOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnListView> {
        let expected = params.scope_version.as_deref();
        crate::support::ensure_deep_page(params.page.unwrap_or(1), expected)?;
        params.validate()?;
        let snapshot = self.purchase_return_list_snapshot(params, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        let current = self.purchase_return_list_snapshot(params, actor).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(crate::support::data_scope_changed("数据范围或业务单据已变化，请刷新"));
        }
        Ok(snapshot)
    }

    /// 查询采购退货单详情（退货单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在或来源采购单不可见
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情凭证；沿来源采购单 detail 动作重验。
    pub async fn purchase_return_order_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnOrderView> {
        let first = self.load_authorized_purchase_return(id, actor).await?;
        let current = self.load_authorized_purchase_return(id, actor).await?;
        if first.1 != current.1 {
            return Err(crate::support::data_scope_changed("数据范围或采购退货单已变化，请刷新"));
        }
        Ok(first.0)
    }

    /// 授权、总数与退货单版本全部在同一个事务读取。
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
    async fn purchase_return_list_snapshot(
        &self,
        params: &PurchaseReturnOrderListParams,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnListView> {
        let db = self.db.clone();
        let access = self.purchase_access();
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
                    let query = params.normalized()?;
                    let filter = PurchaseReturnOrderFilter {
                        purchase_return_no: query.purchase_return_no,
                        purchase_order_id: query.purchase_order_id,
                        authorized_purchase_order_ids: authorized,
                        status: query.status,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by: Some(query.paging.sort_by.to_string()),
                        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
                    };
                    let page =
                        db.purchase_return_orders().search_purchase_return_orders(&filter, executor).await?;
                    let versions =
                        db.purchase_return_orders().query_purchase_return_versions(&filter, executor).await?;
                    if versions.len() > 10_000 {
                        return Err(Error::ValidationError(
                            "采购退货查询超过上限，请收窄原采购单条件".into(),
                        ));
                    }
                    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
                    versions.hash(&mut fingerprint);
                    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
                    let mut views = Vec::with_capacity(page.items.len());
                    for row in page.items {
                        views.push(purchase_return_order_view(&db, row.id, executor).await?);
                    }
                    Ok(PurchaseReturnListView {
                        page: PageView {
                            items: views,
                            total: page.total,
                            page: filter.page,
                            page_size: filter.page_size,
                        },
                        scope_version: context.scope_version,
                        policy_version: context.policy_version,
                        organization_version: context.organization_version,
                        as_of: context.as_of.as_utc().to_rfc3339(),
                        empty_reason: no_scope.then_some("no_scope"),
                        scope_summary: "采购退货单沿来源采购单当前负责人及单据业务组织范围",
                    })
                })
            })
            .await
    }

    /// 在独立事务中重验来源采购单后装配退货详情。
    ///
    /// # 参数
    /// * `id` - 退货单主键
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回视图和范围版本绑定。
    ///
    /// # 错误
    /// 退货单不存在或来源采购单不可见时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 不泄露越权退货单的存在性。
    async fn load_authorized_purchase_return(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<(PurchaseReturnOrderView, String)> {
        let db = self.db.clone();
        let access = self.purchase_access();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = db
                        .purchase_return_orders()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购退货单不存在或无权查看".to_string()))?;
                    let (context, scope) = access.resolve(&actor, "detail", executor).await?;
                    let source = db
                        .purchase_orders()
                        .find_authorized(order.purchase_order_id.as_ref(), &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购退货单不存在或无权查看".to_string()))?;
                    let view = purchase_return_order_view(&db, order.base.id.clone(), executor).await?;
                    let version =
                        format!("{}:{}:{}", context.scope_version, source.base.id, source.base.version);
                    Ok((view, version))
                })
            })
            .await
    }
}

/// 装配采购退货单视图。
///
/// # 参数
/// * `db` - 数据库
/// * `id` - 退货单 ID
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回完整退货单视图。
///
/// # 错误
/// * `NotFound` - 退货单不存在
///
/// # 关键业务约束
/// 明细必须与主单同一执行器读取。
async fn purchase_return_order_view(
    db: &mongodb::Database,
    id: String,
    executor: &mut dyn persistence_core::Executor,
) -> Result<PurchaseReturnOrderView> {
    let order = db
        .purchase_return_orders()
        .find_by_id(&id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购退货单不存在".to_string()))?;
    let lines = db
        .purchase_return_lines()
        .find_lines_by_orders(&[order.base.id.clone().into()], executor)
        .await?
        .into_iter()
        .map(|line| super::dto::PurchaseReturnLineView {
            id: line.base.id.clone(),
            purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
            return_quantity: line.return_quantity,
            warehouse_id: line.warehouse_id.map(|id| id.to_string()),
        })
        .collect();
    Ok(PurchaseReturnOrderView {
        id: order.base.id.clone(),
        purchase_return_no: order.purchase_return_no,
        purchase_order_id: order.purchase_order_id.to_string(),
        sales_return_case_id: order.sales_return_case_id.map(|id| id.to_string()),
        return_mode: order.return_mode,
        status: order.stable.status(),
        version: order.base.version,
        created_at: order.base.created_at,
        lines,
    })
}
