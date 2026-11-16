//! 销售选品册和方案的跨域授权读取。
mod list;
mod read;
use std::sync::Arc;

use application_core::{AuditActor, PageView};
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_core::common::time::BusinessDate;
use erp_permissions::sales_selection;
use erp_sales::dto::sales_selection::{
    SalesSelectionBookletListItemView, SalesSelectionBookletListParams, SalesSelectionBookletView,
    SalesSelectionProposalListItemView, SalesSelectionProposalListParams, SalesSelectionProposalView,
    SelectionSourceSkuView,
};
use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
use erp_sales::service::sales_selection::{SalesSelectionService, SelectionAccess};
use list::{booklet_list_snapshot, owner_display_names, proposal_list_snapshot};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::sales_center::selection_pricing::enrich_selection_prices;
use crate::{Error, Result};
/// 在同一授权快照组合选品、目录来源与账号名称的读模型。
pub struct SalesSelectionReadService {
    db: Database,
    access: SelectionAccess,
    catalog: Arc<dyn CatalogSupplyQueryPort>,
}
impl SalesSelectionReadService {
    /// 绑定共享领域范围与目录读取能力。
    /// # 参数
    /// db 为原数据库，access 为选品授权，catalog 为当前目录事实 Port。
    /// # 返回
    /// 不缓存授权结果的读取入口。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database, access: SelectionAccess, catalog: Arc<dyn CatalogSupplyQueryPort>) -> Self {
        Self { db, access, catalog }
    }

    /// 列表。
    ///
    /// 授权、计数与分页使用同一读取事务。
    ///
    /// # 参数
    /// * `params` - 筛选
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页与范围版本；当前页行带负责人显示名，不含筛选候选。
    ///
    /// # 错误
    /// 参数非法或范围变化。
    pub async fn booklet_list(
        &self,
        params: SalesSelectionBookletListParams,
        actor: AuditActor,
    ) -> Result<SelectionBookletListView> {
        let access = self.access.clone();
        let db = self.db.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { booklet_list_snapshot(&access, &db, &params, &actor, executor).await })
            })
            .await
    }

    /// 详情。
    ///
    /// 授权、负责人显示名、来源商品与详情共用同一快照执行器。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 不存在或无权查看。
    pub async fn booklet_detail(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionBookletView> {
        let db = self.db.clone();
        let access = self.access.clone();
        let authorization = access.authorization();
        let catalog = self.catalog.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    read::checked(
                        &access,
                        &actor,
                        &sales_selection::BOOKLET_DETAIL,
                        executor,
                        move |scope, executor| {
                            Box::pin(async move {
                                let book = SalesSelectionDomainRepository::new(&db)
                                    .find_authorized_booklet(&id, scope, executor)
                                    .await?
                                    .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;
                                let mut view = SalesSelectionService::new(db.clone(), authorization)
                                    .detail_view(&book, None, executor)
                                    .await?;
                                view.sales_owner_name =
                                    owner_display_names(&db, &[view.sales_owner_user_id.clone()], executor)
                                        .await?
                                        .remove(&view.sales_owner_user_id);
                                view.source_skus =
                                    source_skus(catalog.as_ref(), &view.sku_ids, executor).await?;
                                enrich_selection_prices(&db, &mut view, executor).await?;
                                Ok(view)
                            })
                        },
                    )
                    .await
                })
            })
            .await
    }

    /// 方案列表。
    ///
    /// 授权、计数与分页使用同一读取事务。
    ///
    /// # 参数
    /// * `params` - 筛选
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页与范围版本；当前页行带负责人显示名，不含筛选候选。
    ///
    /// # 错误
    /// 参数非法或范围变化。
    pub async fn proposal_list(
        &self,
        params: SalesSelectionProposalListParams,
        actor: AuditActor,
    ) -> Result<SelectionProposalListView> {
        let access = self.access.clone();
        let db = self.db.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { proposal_list_snapshot(&access, &db, &params, &actor, executor).await })
            })
            .await
    }

    /// 方案详情。
    ///
    /// 授权与方案详情共用同一快照执行器。
    ///
    /// # 参数
    /// * `id` - 方案
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 不存在或无权查看。
    pub async fn proposal_detail(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionProposalView> {
        let access = self.access.clone();
        let authorization = access.authorization();
        let db = self.db.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    read::checked(
                        &access,
                        &actor,
                        &sales_selection::PROPOSAL_DETAIL,
                        executor,
                        move |scope, executor| {
                            Box::pin(async move {
                                let proposal = SalesSelectionDomainRepository::new(&db)
                                    .find_authorized_proposal(&id, scope, executor)
                                    .await?
                                    .ok_or_else(|| Error::NotFound("销售方案不存在或无权查看".into()))?;
                                Ok(SalesSelectionService::new(db, authorization)
                                    .proposal_view_of(&proposal, executor)
                                    .await?)
                            })
                        },
                    )
                    .await
                })
            })
            .await
    }
}
/// 选品册列表行：已授权业务行及其负责人显示名，不是筛选候选。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionBookletListRow {
    /// 选品册列表行。
    #[serde(flatten)]
    item: SalesSelectionBookletListItemView,
    /// 当前页负责人显示名；账号缺失时省略，不回退人员目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    sales_owner_name: Option<String>,
}

/// 方案列表行：已授权业务行及其负责人显示名，不是筛选候选。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionProposalListRow {
    /// 方案列表行。
    #[serde(flatten)]
    item: SalesSelectionProposalListItemView,
    /// 当前页负责人显示名；账号缺失时省略，不回退人员目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    sales_owner_name: Option<String>,
}

/// 选品册列表视图：分页与范围版本。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionBookletListView {
    /// 当前页；行上带负责人显示名。
    pub page: PageView<SelectionBookletListRow>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// 无范围时为 true，前端据此区分空结果。
    pub no_scope: bool,
}

/// 方案列表视图：分页与范围版本。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionProposalListView {
    /// 当前页；行上带负责人显示名。
    pub page: PageView<SelectionProposalListRow>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// 无范围时为 true，前端据此区分空结果。
    pub no_scope: bool,
}

/// 回显已通过册详情权限验证的来源，不扩大来源或返回采购价格。
/// # 参数
/// `query` 为已装配目录读取 Port，`ids` 仅允许来自已授权册。
/// # 返回
/// 当前仍可售来源的名称与业务编号。
/// # 错误
/// 查询失败时返回错误。
async fn source_skus(
    query: &dyn CatalogSupplyQueryPort,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<SelectionSourceSkuView>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = query.find_sellable_skus_by_ids(ids, BusinessDate::today(), executor).await?;
    Ok(rows
        .into_iter()
        .map(|row| SelectionSourceSkuView { sku_id: row.sku_id, name: row.name, sku_no: row.sku_no })
        .collect())
}
