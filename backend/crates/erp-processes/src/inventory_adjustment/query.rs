use application_core::AuditActor;
use erp_inventory::StockAdjustmentDetailView;
use persistence_core::NoTransaction;

use super::InventoryAdjustmentService;
use super::adapter::require_frozen_binding;
use super::approval_query::{self, load_approval_binding};
use crate::Result;

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
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_detail")
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
        let inventory = self.inventory();
        let adjustment = inventory.readable_stock_adjustment(id, actor).await?;
        let lines = inventory.load_adjustment_lines(&adjustment.base.id).await?;
        let movements = inventory.load_movements_for_source_document(id).await?;
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
            },
            None => approval_query::load_document_approval(self, &adjustment, Some(binding), actor).await?,
        };
        let mut adjustment_view = erp_inventory::StockAdjustmentView::from(adjustment);
        if let Some(fact) = inventory.adjustment_people_by_ids(&[id.to_string()]).await?.remove(id) {
            adjustment_view.apply_people(&fact);
        }
        Ok(StockAdjustmentDetailView {
            approval,
            adjustment: adjustment_view,
            lines: lines.into_iter().map(Into::into).collect(),
            posted_movements: movements.into_iter().map(Into::into).collect(),
        })
    }
}
