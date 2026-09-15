//! 销售变更作废的销售状态和工作副本写入，不包含审计或根事务。

use application_core::AuditActor;
use erp_core::ids::SalesChangeOrderId;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::SalesReviewService;
use crate::dto::sales_review::VoidSalesChangeOrderRequest;
use crate::entity::sales_review::SalesChangeOrder;
use crate::repository::{SalesOrderExt, SalesReviewExt};
use crate::{Error, Result};

/// 已校验版本并作废的销售事实。
pub struct VoidChangeWrite {
    change_order: SalesChangeOrder,
    working_copy: Option<crate::entity::sales_order::SalesOrderWorkingCopy>,
}
impl VoidChangeWrite {
    /// 来源销售单主键，供事务内沿原单范围重验。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回原销售单稳定身份。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 作废不得改写来源销售单身份。
    pub fn sales_order_id(&self) -> &str {
        self.change_order.sales_order_id.as_ref()
    }

    /// 在同一执行器内先写作废变更，再写放弃的工作副本。
    ///
    /// # 错误
    /// 任一步 CAS 失败时交由根事务回滚。
    pub async fn persist(&mut self, db: &mongodb::Database, executor: &mut dyn Executor) -> Result<()> {
        db.sales_change_orders().update(&mut self.change_order, executor).await?;
        if let Some(copy) = &mut self.working_copy {
            db.sales_order_working_copies().update(copy, executor).await?;
        }
        Ok(())
    }
}
impl SalesReviewService {
    /// 作废销售变更单（仅草稿态）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn prepare_void(
        &self,
        id: &str,
        req: VoidSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<VoidChangeWrite> {
        req.validate()?;
        let mut change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if !change_order.matches_version(req.version) {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        change_order.void(actor.id())?;
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_by_sales_change_order(&SalesChangeOrderId::new(id), &mut NoTransaction)
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        Ok(VoidChangeWrite { change_order, working_copy })
    }
}
