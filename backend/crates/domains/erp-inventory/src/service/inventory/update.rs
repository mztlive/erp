use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::StockAdjustmentId;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::InventoryService;
use super::mapping::build_adjustment_line_updates;
use crate::dto::{StockAdjustmentView, UpdateStockAdjustmentRequest};
use crate::entity::inventory::{StockAdjustment, StockAdjustmentLineUpdate, StockAdjustmentUpdate};
use crate::error::{Error, Result};
use crate::ports::{AuthorizationPort, InventoryAuditPort, PreparedInventoryAudit};
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
    /// 请求体或明细数量非法时返回 `ValidationError`。
    /// 调整单不存在、账号未激活、仓库不在更新范围，或明细行未命中时返回 `NotFound`。
    /// 期望版本不一致时返回 `ConflictError`。
    /// 状态不可编辑、文本过长、行重复或方向不一致时返回 `Error::Logic`。
    /// 授权、审计或仓储写入失败时返回对应错误。
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
        let audit = self.audit.resource_log(
            actor.clone(),
            "stock_adjustment.update",
            "stock_adjustment",
            id.to_string(),
        )?;
        let command = UpdateCommand {
            actor: actor.clone(),
            id: StockAdjustmentId::new(id.to_string()),
            req,
            line_updates,
            audit,
        };
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let audit = self.audit.clone();
        let updated = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    persist_update(&db, authorization.as_ref(), audit.as_ref(), command, executor).await
                })
            })
            .await?;
        Ok(updated.into())
    }
}

/// 在原事务持久化已由调整单实体校验的明细变化。
async fn update_adjustment_lines(
    db: &Database,
    id: &StockAdjustmentId,
    adjustment: &mut StockAdjustment,
    updates: &[StockAdjustmentLineUpdate],
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut existing =
        db.inventory().adjustment_lines_by_adjustment_ids(std::slice::from_ref(id), executor).await?;
    let changed = adjustment.apply_line_updates(&mut existing, updates, false)?;
    for line in &changed {
        if !db.inventory().persist_adjustment_line(line, executor).await? {
            return Err(Error::NotFound("调整明细不存在".into()));
        }
    }
    Ok(())
}

/// 在原更新事务加载并验证目标与期望业务版本。
async fn adjustment_for_update(
    db: &Database,
    authorization_port: &dyn AuthorizationPort,
    actor: &AuditActor,
    id: &StockAdjustmentId,
    version: u64,
    executor: &mut dyn Executor,
) -> Result<StockAdjustment> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("库存调整修改必须使用快照事务".into()));
    }
    authorization_port.require_adjustment_update(actor, id.as_ref(), executor).await.map_err(|error| {
        match error {
            Error::Forbidden(_) | Error::NotFound(_) => Error::NotFound("库存调整单不存在".into()),
            other => other,
        }
    })?;
    let adjustment = db
        .stock_adjustments()
        .find_by_id(id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".into()))?;
    if !adjustment.matches_version(version) {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".into()));
    }
    Ok(adjustment)
}

struct UpdateCommand {
    actor: AuditActor,
    id: StockAdjustmentId,
    req: UpdateStockAdjustmentRequest,
    line_updates: Vec<StockAdjustmentLineUpdate>,
    audit: PreparedInventoryAudit,
}

/// 更新步骤加入调用方事务，实体状态和版本检查保持原顺序。
async fn persist_update(
    db: &Database,
    authorization: &dyn AuthorizationPort,
    audit_port: &dyn InventoryAuditPort,
    command: UpdateCommand,
    executor: &mut dyn Executor,
) -> Result<StockAdjustment> {
    let UpdateCommand { actor, id, req, line_updates, audit } = command;
    let mut adjustment = adjustment_for_update(db, authorization, &actor, &id, req.version, executor).await?;
    adjustment.update(StockAdjustmentUpdate {
        reason_type: req.reason_type,
        reviewed_by: None,
        finance_reviewed_by: None,
        note: req.note,
        occurred_at: req.occurred_at.map(Instant::from_unix_secs),
    })?;
    if req.reason_type.is_some() || !line_updates.is_empty() {
        update_adjustment_lines(db, &id, &mut adjustment, &line_updates, executor).await?;
    }
    db.stock_adjustments().update(&mut adjustment, executor).await?;
    audit_port.persist(&audit, executor).await?;
    Ok(adjustment)
}

#[cfg(test)]
mod tests;
