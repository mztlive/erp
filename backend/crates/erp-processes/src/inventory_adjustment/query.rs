use erp_inventory::InventoryExt;
use erp_inventory::StockAdjustment;
use persistence_core::{NoTransaction, Transactional};

use super::adapter::require_frozen_binding;
use super::approval_query::{self, load_approval_binding};
use super::InventoryAdjustmentService;
use crate::adapters::authorize_inventory;
use application_core::AuditActor;
use erp_inventory::StockAdjustmentDetailView;
use services::{Error, Result};

impl InventoryAdjustmentService {
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
                    let authorization = authorize_inventory(&db, &rbac, &actor, session).await?;
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
