use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::StockAdjustmentId;
use persistence_core::Transactional;
use validator::Validate;

use super::InventoryService;
use super::mapping::build_adjustment_line_updates;
use crate::dto::{StockAdjustmentView, UpdateStockAdjustmentRequest};
use crate::entity::inventory::{StockAdjustment, StockAdjustmentUpdate};
use crate::error::{Error, Result};
use crate::repository::InventoryExt;

impl InventoryService {
    /// 更新库存调整单（仅草稿/驳回；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后调整单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_update",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_update")
    )]
    pub async fn update_stock_adjustment(
        &self,
        id: &str,
        req: UpdateStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let line_updates = build_adjustment_line_updates(req.lines.as_deref().unwrap_or_default())?;
        let requires_line_validation = req.reason_type.is_some() || !line_updates.is_empty();
        let audit = self.audit.resource_log(
            actor.clone(),
            "stock_adjustment.update",
            "stock_adjustment",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let authorization_port = std::sync::Arc::clone(&self.authorization);
        let audit_port = std::sync::Arc::clone(&self.audit);
        let client = db.client().clone();
        let adjustment_id = StockAdjustmentId::new(id.to_string());
        let actor = actor.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let authorization = authorization_port.authorize(&actor, session).await?;
                    let mut adjustment = db
                        .inventory()
                        .stock_adjustment(adjustment_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                    if !authorization.actor_is_active()
                        || !authorization.can_update(adjustment.warehouse_id.as_ref())
                    {
                        return Err(Error::NotFound("库存调整单不存在".to_string()));
                    }
                    if !adjustment.matches_version(req.version) {
                        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
                    }
                    adjustment.update(StockAdjustmentUpdate {
                        reason_type: req.reason_type,
                        reviewed_by: None,
                        finance_reviewed_by: None,
                        note: req.note,
                        occurred_at: req.occurred_at.map(Instant::from_unix_secs),
                    })?;
                    if requires_line_validation {
                        let mut existing = db
                            .inventory()
                            .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&adjustment_id), session)
                            .await?;
                        let changed = adjustment.apply_line_updates(&mut existing, &line_updates, false)?;
                        for line in &changed {
                            if !db.inventory().persist_adjustment_line(line, session).await? {
                                return Err(Error::NotFound("调整明细不存在".to_string()));
                            }
                        }
                    }
                    db.stock_adjustments().update(&mut adjustment, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<StockAdjustment, crate::error::Error>(adjustment)
                })
            })
            .await?;
        Ok(updated.into())
    }
}
