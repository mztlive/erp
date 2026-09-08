use crate::entity::inventory::{StockAdjustment, StockAdjustmentLine, StockMovement};
use crate::repository::InventoryExt;
use erp_core::ids::StockAdjustmentId;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::InventoryService;
use crate::dto::{
    PageView, SortDir, StockAdjustmentLineView, StockAdjustmentListParams, StockAdjustmentView,
};
use crate::error::{Error, Result};
use application_core::AuditActor;

use crate::repository::StockAdjustmentFilter;

impl InventoryService {
    /// 分页查询库存调整单列表（W10 调整记录视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/状态筛选）
    /// * `actor` - 当前认证操作人，用于计算对象读取范围
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_list")
    )]
    pub async fn stock_adjustment_list(
        &self,
        params: &StockAdjustmentListParams,
        actor: &AuditActor,
    ) -> Result<PageView<StockAdjustmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let page_no = query.paging.page;
        let page_size = query.paging.page_size;
        let db = self.db.clone();
        let authorization_port = std::sync::Arc::clone(&self.authorization);
        let catalog = std::sync::Arc::clone(&self.catalog_facts);
        let actor = actor.clone();
        let client = db.client().clone();
        let page = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization = authorization_port.authorize(&actor, session).await?;
                    if !authorization.actor_is_active() {
                        return Err(Error::Forbidden("当前账号无库存调整读取权限".to_string()));
                    }
                    let search = super::search::sku_filter(
                        catalog.as_ref(),
                        query.q.as_deref(),
                        query.sku_id.as_ref(),
                        session,
                    )
                    .await?;
                    let search = super::search::adjustment_filter(
                        &db,
                        search,
                        query.adjustment_id.as_deref(),
                        session,
                    )
                    .await?;
                    let filter = StockAdjustmentFilter {
                        search,
                        warehouse_ids: authorization
                            .adjustment_list_scope()
                            .repository_warehouse_ids(query.warehouse_id),
                        status: query.status,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by: Some(query.paging.sort_by.to_string()),
                        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
                    };
                    Ok::<_, Error>(
                        db.stock_adjustments()
                            .search_stock_adjustments(&filter, session)
                            .await?,
                    )
                })
            })
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| StockAdjustmentView {
                id: row.id,
                adjustment_no: row.adjustment_no,
                warehouse_id: row.warehouse_id.to_string(),
                reason_type: row.reason_type,
                status: row.status,
                prepared_by: row.prepared_by,
                reviewed_by: row.reviewed_by,
                finance_reviewed_by: row.finance_reviewed_by,
                note: row.note,
                occurred_at: row.occurred_at.map(|instant| instant.unix_secs()),
                version: row.version.to_string(),
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: page_no,
            page_size,
        })
    }

    /// 在同一快照内加载表头并验证对象读取范围；拒绝结果隐藏资源存在性。
    pub async fn readable_stock_adjustment(&self, id: &str, actor: &AuditActor) -> Result<StockAdjustment> {
        let db = self.db.clone();
        let authorization_port = std::sync::Arc::clone(&self.authorization);
        let id = id.to_string();
        let actor = actor.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization = authorization_port.authorize(&actor, session).await?;
                    let adjustment = db
                        .inventory()
                        .stock_adjustment(&id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                    if !authorization.actor_is_active()
                        || !authorization
                            .read_scope()
                            .covers(adjustment.warehouse_id.as_ref())
                    {
                        return Err(Error::NotFound("库存调整单不存在".to_string()));
                    }
                    Ok::<_, Error>(adjustment)
                })
            })
            .await
    }

    /// 按主键读取库存调整单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    pub async fn load_stock_adjustment(&self, id: &str) -> Result<StockAdjustment> {
        self.db
            .inventory()
            .stock_adjustment(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))
    }

    /// 按调整单主键读取全部明细。
    ///
    /// # 参数
    /// * `adjustment_id` - 调整单主键
    ///
    /// # 返回
    /// 返回该调整单的明细集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    pub async fn load_adjustment_lines(&self, adjustment_id: &str) -> Result<Vec<StockAdjustmentLine>> {
        let id = StockAdjustmentId::new(adjustment_id.to_string());
        Ok(self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&id), &mut NoTransaction)
            .await?)
    }

    /// 按来源单据读取过账流水。
    ///
    /// # 参数
    /// * `document_id` - 来源单据主键
    ///
    /// # 返回
    /// 返回该来源单据产生的库存流水。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    pub async fn load_movements_for_source_document(&self, document_id: &str) -> Result<Vec<StockMovement>> {
        Ok(self
            .db
            .inventory()
            .movements_for_source_document(document_id, &mut NoTransaction)
            .await?)
    }
}

impl From<StockAdjustment> for StockAdjustmentView {
    /// 从调整单实体构造视图。
    fn from(adjustment: StockAdjustment) -> Self {
        Self {
            id: adjustment.base.id,
            adjustment_no: adjustment.adjustment_no,
            warehouse_id: adjustment.warehouse_id.to_string(),
            reason_type: adjustment.reason_type,
            status: adjustment.status,
            prepared_by: adjustment.prepared_by,
            reviewed_by: adjustment.reviewed_by,
            finance_reviewed_by: adjustment.finance_reviewed_by,
            note: adjustment.note,
            occurred_at: adjustment.occurred_at.map(|instant| instant.unix_secs()),
            version: adjustment.base.version.to_string(),
            created_at: adjustment.base.created_at,
        }
    }
}

impl From<StockAdjustmentLine> for StockAdjustmentLineView {
    /// 从调整明细实体构造视图。
    fn from(line: StockAdjustmentLine) -> Self {
        Self {
            id: line.base.id,
            sku_id: line.sku_id.to_string(),
            quantity: line.quantity,
            direction: line.direction,
        }
    }
}
