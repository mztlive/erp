use database::InventoryExt;
use entities::inventory::{StockAdjustment, StockAdjustmentLine};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::adapter::require_frozen_binding;
use super::approval_query::{self, load_approval_binding};
use super::authorization::inventory_authorization_with_executor;
use super::dto::{
    PageView, SortDir, StockAdjustmentDetailView, StockAdjustmentLineView, StockAdjustmentListParams,
    StockAdjustmentView,
};
use super::InventoryService;
use crate::errors::{Error, Result};
use application_core::AuditActor;

/// 库存调整单列表筛选条件类型。
type StockAdjustmentFilter = <mongodb::Database as InventoryExt>::StockAdjustmentFilter;

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
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let client = db.client().clone();
        let page = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization =
                        inventory_authorization_with_executor(&db, &rbac, &actor, session).await?;
                    if !authorization.actor_is_active() {
                        return Err(Error::Forbidden("当前账号无库存调整读取权限".to_string()));
                    }
                    let filter = StockAdjustmentFilter {
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

    /// 查询库存调整单详情（表头 + 明细 + 过账流水）。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `actor` - 当前认证操作人，用于投影 actor-aware 审批动作
    ///
    /// # 返回
    /// 返回调整单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_detail"
        )
    )]
    pub async fn stock_adjustment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        self.stock_adjustment_detail_with_instance(id, actor, None).await
    }

    /// 构造库存调整详情；签署命令结果必须按收据引用的实例精确投影。
    pub(super) async fn stock_adjustment_detail_with_instance(
        &self,
        id: &str,
        actor: &AuditActor,
        approval_instance: Option<(&str, u32)>,
    ) -> Result<StockAdjustmentDetailView> {
        let adjustment = self.readable_stock_adjustment(id, actor).await?;
        let lines = self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(&[adjustment.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let movements = self
            .db
            .inventory()
            .movements_for_source_document(id, &mut NoTransaction)
            .await?;
        let binding = load_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?;
        let approval = match approval_instance {
            Some((instance_id, expected_subject_version)) => {
                approval_query::load_document_approval_for_instance(
                    self,
                    &adjustment,
                    Some(binding),
                    instance_id,
                    expected_subject_version,
                    actor,
                )
                .await?
            }
            None => approval_query::load_document_approval(self, &adjustment, Some(binding), actor).await?,
        };
        Ok(StockAdjustmentDetailView {
            approval,
            adjustment: adjustment.into(),
            lines: lines.into_iter().map(Into::into).collect(),
            posted_movements: movements.into_iter().map(Into::into).collect(),
        })
    }

    /// 在同一快照内加载表头并验证对象读取范围；拒绝结果隐藏资源存在性。
    async fn readable_stock_adjustment(&self, id: &str, actor: &AuditActor) -> Result<StockAdjustment> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_string();
        let actor = actor.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization =
                        inventory_authorization_with_executor(&db, &rbac, &actor, session).await?;
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
    pub(super) async fn load_stock_adjustment(&self, id: &str) -> Result<StockAdjustment> {
        self.db
            .inventory()
            .stock_adjustment(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))
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
