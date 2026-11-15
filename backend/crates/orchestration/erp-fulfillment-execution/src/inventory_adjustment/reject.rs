//! 库存调整驳回只更新审批状态；库存事实在最终通过时才产生。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_inventory::InventoryExt;
use persistence_core::Executor;

use super::InventoryAdjustmentProcess;
use crate::{Error, Result};

impl InventoryAdjustmentProcess {
    /// 在统一审批事务内把库存调整单标记为已驳回。
    /// # 参数
    /// * `id` - 原单主键。
    /// * `expected_subject_version` - 被驳回实例冻结的审批主题版本。
    /// * `actor` - 已授权审批人。
    /// * `executor` - 审批运行时持有的事务执行器。
    /// # 返回
    /// 保存已驳回状态和审计，不生成库存流水。
    /// # 错误
    /// 缺少事务、单据不存在、状态或并发版本冲突时拒绝。
    pub async fn reject(
        &self,
        id: &str,
        expected_subject_version: u32,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::Internal("库存调整驳回缺少运行时事务".into()));
        }
        let mut adjustment = self
            .db
            .inventory()
            .stock_adjustment(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".into()))?;
        adjustment.reject_approval(expected_subject_version)?;
        self.db.stock_adjustments().update(&mut adjustment, executor).await?;
        let audit =
            actor.clone().resource_log("stock_adjustment.reject", "stock_adjustment", id.to_owned())?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }
}
