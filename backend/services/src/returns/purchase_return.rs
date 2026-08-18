use super::dto::{
    CreatePurchaseReturnOrderRequest, PageView, PurchaseReturnOrderListParams, PurchaseReturnOrderView,
    SortDir,
};
use super::ReturnsService;
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use database::{AccessControlExt, NoTransaction, ReturnsExt, Transactional};
use entities::document_registry::DocumentType;
use entities::ids::{PurchaseReturnLineId, PurchaseReturnOrderId};
use entities::returns::{
    PurchaseReturnLine, PurchaseReturnLineData, PurchaseReturnOrder, PurchaseReturnOrderData,
};
use id_generator::next_id;
use validator::Validate;

/// 采购退货单列表筛选条件类型。
type PurchaseReturnOrderFilter = <mongodb::Database as ReturnsExt>::PurchaseReturnOrderFilter;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 采购退货单
    // -----------------------------------------------------------------------

    /// 分页查询采购退货单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn purchase_return_order_list(
        &self,
        params: &PurchaseReturnOrderListParams,
    ) -> Result<PageView<PurchaseReturnOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReturnOrderFilter {
            purchase_return_no: query.purchase_return_no,
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_return_orders()
            .search_purchase_return_orders(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.purchase_return_order_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询采购退货单详情（退货单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在
    pub async fn purchase_return_order_detail(&self, id: &str) -> Result<PurchaseReturnOrderView> {
        self.purchase_return_order_view(id.to_string()).await
    }

    /// 建立采购退货单与明细行（跨集合事务写入）。
    ///
    /// `purchase_return_no` 全局唯一（唯一索引）构成幂等去重。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建退货单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 采购退货单号重复
    pub async fn create_purchase_return_order(
        &self,
        req: CreatePurchaseReturnOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnOrderView> {
        req.validate()?;
        let order_id = PurchaseReturnOrderId::new(next_id());
        let order = PurchaseReturnOrder::new(
            order_id.clone(),
            PurchaseReturnOrderData {
                purchase_return_no: req.purchase_return_no,
                purchase_order_id: req.purchase_order_id,
                sales_return_case_id: req.sales_return_case_id,
                return_mode: req.return_mode,
            },
            actor.id(),
        )?;
        let line = PurchaseReturnLine::new(
            PurchaseReturnLineId::new(next_id()),
            PurchaseReturnLineData {
                purchase_return_order_id: order_id.clone(),
                purchase_order_revision_line_id: req.lines[0].purchase_order_revision_line_id.clone(),
                return_quantity: req.lines[0].return_quantity,
                warehouse_id: req.lines[0].warehouse_id.clone(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "purchase_return_order.create",
            "purchase_return_order",
            order_id.to_string(),
        )?;
        let document = new_registered_document(
            &order_id,
            DocumentType::PurchaseReturnOrder,
            order.purchase_return_no.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.returns()
                        .create_purchase_return_with_line(&order, &line, session)
                        .await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.purchase_return_order_detail(&order_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配采购退货单视图。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在
    async fn purchase_return_order_view(&self, id: String) -> Result<PurchaseReturnOrderView> {
        let order = self
            .db
            .purchase_return_orders()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购退货单不存在".to_string()))?;
        let lines = self
            .db
            .purchase_return_lines()
            .find_lines_by_orders(&[order.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| crate::returns::dto::PurchaseReturnLineView {
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
}
