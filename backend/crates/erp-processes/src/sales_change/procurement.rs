//! 销售变更生效后的采购联动（第一期 §6.3“销售变更对采购的影响”第 4 条）。
//!
//! 在生效写入之后、同一事务内：按新销售版本判定受影响采购，撤回其中审批中的采购，
//! 再同步采购依据更新记录、采购负责人待办与供给分配任务。任一步失败则销售变更不生效。

use application_core::AuditActor;
use erp_core::ids::SalesOrderId;
use erp_read_models::purchase_center::source_impact::load_purchase_source_state;
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_review::EffectiveChangeWrite;
use mongodb::Database;
use persistence_core::Executor;

use crate::procure_to_pay::{SalesChangeWithdrawal, sync_procurement_after_sales_change};
use crate::{Error, Result};

/// 生效写入移交前取得的变更身份。
#[derive(Debug, Clone)]
pub(super) struct EffectiveChangeIdentity {
    /// 销售变更单。
    change_id: String,
    /// 来源销售单。
    sales_order_id: String,
    /// 来源销售单号。
    sales_order_no: String,
}

impl EffectiveChangeIdentity {
    /// 从生效写入中复制身份，写入随后被持久化步骤消耗。
    ///
    /// # 参数
    /// * `write` - 已准备的生效写入
    ///
    /// # 返回
    /// 返回变更单、销售单及单号。
    ///
    /// # 错误
    /// 无。
    pub(super) fn of(write: &EffectiveChangeWrite) -> Self {
        Self {
            change_id: write.change_id().to_string(),
            sales_order_id: write.sales_order_id().to_string(),
            sales_order_no: write.sales_order_no().to_string(),
        }
    }

    /// 撤回受影响的审批中采购并同步依据更新与供给分配任务。
    ///
    /// # 参数
    /// * `db` - 业务数据库
    /// * `actor` - 触发生效的审批人
    /// * `executor` - 生效事务执行器
    ///
    /// # 返回
    /// 全部联动成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 销售单缺失、覆盖事实不完整、撤回 CAS 冲突或写入失败时返回错误。
    pub(super) async fn sync_procurement(
        &self,
        db: &Database,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let sales_order_id = SalesOrderId::new(&self.sales_order_id);
        let order = db
            .sales_orders()
            .find_by_id(&sales_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let state = load_purchase_source_state(db, &order, executor).await?;
        SalesChangeWithdrawal {
            sales_change_order_id: &self.change_id,
            sales_order_no: &self.sales_order_no,
            actor,
        }
        .withdraw_affected(db, &state, executor)
        .await?;
        sync_procurement_after_sales_change(db, &sales_order_id, &self.change_id, executor).await
    }
}
