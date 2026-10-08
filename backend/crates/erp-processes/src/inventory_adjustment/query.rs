use std::future::Future;

use application_core::AuditActor;
use erp_inventory::{
    Result as InventoryResult, StockAdjustmentDetailView, StockAdjustmentLineView, StockAdjustmentView,
};
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
    ///
    /// # 参数
    /// * `id` - 调整单主键。
    /// * `actor` - 当前认证操作人，用于投影审批动作。
    /// * `approval_instance` - 为 `Some` 时按该实例与期望主题版本投影；`None` 时加载主题上的当前审批。
    ///
    /// # 返回
    /// 返回含审批、表头、明细与过账流水的详情；姓名和 SKU 名称查询失败时保留缺失名称。
    ///
    /// # 错误
    /// 调整单不可读、绑定缺失、审批投影失败，或明细、流水、责任人员事实读取失败时返回对应错误。
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
        let mut adjustment_view = StockAdjustmentView::from(adjustment);
        if let Some(fact) = inventory.adjustment_people_by_ids(&[id.to_string()]).await?.remove(id) {
            adjustment_view.apply_people(&fact);
        }
        optional_display_names(
            inventory.enrich_adjustment_people_names(std::slice::from_mut(&mut adjustment_view)),
            actor,
            "people",
        )
        .await;
        let mut lines = lines.into_iter().map(Into::into).collect::<Vec<StockAdjustmentLineView>>();
        optional_display_names(inventory.enrich_adjustment_line_names(&mut lines), actor, "sku").await;
        Ok(StockAdjustmentDetailView {
            approval,
            adjustment: adjustment_view,
            lines,
            posted_movements: movements.into_iter().map(Into::into).collect(),
        })
    }
}

/// 纯展示关联不可用时保留缺失名称；正式命令和授权事实不进入该降级路径。
async fn optional_display_names(
    read: impl Future<Output = InventoryResult<()>>,
    actor: &AuditActor,
    field: &'static str,
) -> bool {
    if read.await.is_ok() {
        return true;
    }
    tracing::warn!(
        account = actor.account(),
        actor_id = actor.id(),
        request_id = actor.request_id().unwrap_or_default(),
        display_field = field,
        "库存调整展示名称读取失败，保留名称缺失占位"
    );
    false
}

#[cfg(test)]
mod tests {
    use std::future::ready;

    use erp_core::AccountKind;
    use erp_inventory::{Error as InventoryError, MovementDirection};
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("warehouse-user".into(), "cangchu".into(), AccountKind::Admin)
            .with_request_id(Some("request-1".into()))
            .unwrap()
    }

    fn line() -> StockAdjustmentLineView {
        StockAdjustmentLineView {
            id: "line-1".into(),
            sku_id: "sku-1".into(),
            sku_code: None,
            sku_name: None,
            quantity: "2".parse().unwrap(),
            direction: MovementDirection::Increase,
        }
    }

    #[tokio::test]
    async fn optional_name_failure_preserves_business_payload_and_allows_next_read() {
        let actor = actor();
        let mut line = line();
        let original = line.clone();
        let failed = ready(Err(InventoryError::Internal("名称查询不可用".into())));
        assert!(!optional_display_names(failed, &actor, "people").await);
        assert_eq!(line, original);
        let succeeded = async {
            line.sku_code = Some("SKU-001".into());
            line.sku_name = Some("礼品卡".into());
            Ok(())
        };
        assert!(optional_display_names(succeeded, &actor, "sku").await);
        assert_eq!(line.sku_id, original.sku_id);
        assert_eq!(line.quantity, original.quantity);
        assert_eq!(line.sku_name.as_deref(), Some("礼品卡"));
    }

    #[tokio::test]
    async fn optional_sku_name_failure_keeps_names_missing_without_command_error() {
        let actor = actor();
        let line = line();
        let original = line.clone();
        let failed = ready(Err(InventoryError::RepositoryError(PersistenceError::DatabaseError(
            MongoError::custom("商品名称查询不可用"),
        ))));
        assert!(!optional_display_names(failed, &actor, "sku").await);
        assert_eq!(line, original);
        assert!(line.sku_code.is_none());
        assert!(line.sku_name.is_none());
    }
}
