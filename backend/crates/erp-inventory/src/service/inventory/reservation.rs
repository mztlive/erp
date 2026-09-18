use application_core::AuditActor;
use persistence_core::Transactional;
use validator::Validate;

use super::InventoryService;
use crate::dto::{PageView, StockReservationListParams, StockReservationView};
use crate::entity::inventory::StockReservation;
use crate::error::{Error, Result};
use crate::repository::{InventoryExt, StockReservationFilter};

impl InventoryService {
    /// 分页查询库存预占列表（W10 销售预占视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/SKU/状态/销售明细筛选）
    /// * `actor` - 当前认证操作人，用于计算库存预占读取范围
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_reservation_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_reservation_list")
    )]
    pub async fn stock_reservation_list(
        &self,
        params: &StockReservationListParams,
        actor: &AuditActor,
    ) -> Result<PageView<StockReservationView>> {
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
                        return Err(Error::Forbidden("当前账号无库存预占读取权限".to_string()));
                    }
                    let search = super::search::sku_filter(
                        catalog.as_ref(),
                        query.q.as_deref(),
                        query.sku_id.as_ref(),
                        session,
                    )
                    .await?;
                    let (sort_by, sort_ascending) = query.paging.sort_selection();
                    let filter = StockReservationFilter {
                        search,
                        warehouse_ids: authorization
                            .reservation_list_scope()
                            .repository_warehouse_ids(query.warehouse_id),
                        sku_id: query.sku_id,
                        status: query.status,
                        sales_order_line_id: query.sales_order_line_id,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by,
                        sort_ascending,
                    };
                    Ok::<_, Error>(db.stock_reservations().search_stock_reservations(&filter, session).await?)
                })
            })
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| StockReservationView {
                id: row.id,
                warehouse_id: row.warehouse_id.to_string(),
                sku_id: row.sku_id.to_string(),
                sales_order_line_id: row.sales_order_line_id.to_string(),
                reserved_quantity: row.reserved_quantity,
                consumed_quantity: row.consumed_quantity,
                released_quantity: row.released_quantity,
                status: row.status,
                version: row.version,
            })
            .collect();
        Ok(PageView { items, total: page.total, page: page_no, page_size })
    }
}

impl From<StockReservation> for StockReservationView {
    /// 从预占实体构造视图。
    fn from(reservation: StockReservation) -> Self {
        Self {
            id: reservation.base.id,
            warehouse_id: reservation.warehouse_id.to_string(),
            sku_id: reservation.sku_id.to_string(),
            sales_order_line_id: reservation.sales_order_line_id.to_string(),
            reserved_quantity: reservation.reserved_quantity,
            consumed_quantity: reservation.consumed_quantity,
            released_quantity: reservation.released_quantity,
            status: reservation.status,
            version: reservation.base.version,
        }
    }
}
