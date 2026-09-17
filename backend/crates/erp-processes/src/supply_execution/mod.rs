//! 域 D32 `supplier_fulfillment` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 下单：`supplier_fulfillment_orders` + `supplier_fulfillment_items` +
//!   首个 `PLACE` 动作 + `inbox_message` + 审计同事务（§6.19）；
//! - 取消/退款：动作头 + 动作行 + 订单状态推进 + `inbox_message` 同事务；
//! - 拒单结果：订单状态 + 状态历史 + 动作结果 + 审计同事务（§6.19 回调幂等）；
//! - 退款成功结果：退款事实头 + 分配行 + 订单退款进度同事务（§8.4 第 5 条
//!   实体可判定部分，`create_refund_fact_with_allocations` 要求事务执行器）。
//!
//! 供应商 API 调用在事务之外完成（P3 §7）：先事务内落 `inbox_message`，事务外经
//! [`SupplierGateway`] 派发（超时/重试上限/错误分类由网关实现承担），结果经
//! `inbox_message` + `integration_error_task` 承接（D34 仓储）；需要人工处理时，
//! 同事务创建 W26 正式 `work_item` 与审计，失败降级为可观测错误并记录
//! `account` 上下文。
//!
//! 跨域协作只经各领域 `*Ext` 扩展 trait 调对方域 Repository（P3 §2）：D25 `supplier_api`
//! （连接与能力）、D24 `supplier_offering`（供给修订）、
//! D34 `integration_ops`（inbox_message / integration_error_task）。
//!
//! 资金/状态机入口一律幂等（§6.19）：下单键为 `fulfillment_order_no`，
//! 取消/退款键为「ERP 供应商订单号 + 动作类型」（本域拼装），
//! 拒单键为 `(connection_id, external_event_id)`，退款结果键为
//! `(connection_id, external_refund_no, external_refund_version)`；
//! 重复提交只返回原结果，不产生第二条正式事实。

mod cancel;
mod complete;
mod dispatch_writes;
pub mod dto;
mod execution;
mod follow_up;
mod handover;
mod investigate;
mod list;
mod place;
mod receipt;
mod refund;
mod refund_result;
mod refund_writes;
mod reject;
mod work_item;

use std::sync::Arc;

use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::ports::supplier_gateway::SupplierGateway;
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use erp_supply::{FulfillmentExceptionHandlerPort, FulfillmentOrderDataScopePort};
use mongodb::Database;

use crate::Result;
/// 供应商履约服务。
///
/// 提供供应商子订单的下单、查询、取消/退款动作提交与外部结果登记编排。
/// 本类型承接跨域写入与外部调用，纯列表由本域服务提供。
pub struct SupplierFulfillmentProcess {
    db: Database,
    gateway: Arc<dyn SupplierGateway>,
    data_scope: Arc<dyn FulfillmentOrderDataScopePort>,
    handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
}
impl SupplierFulfillmentProcess {
    /// 创建供应商履约服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `gateway` - 供应商动作派发网关（真实 Connector 接入点，只在事务外调用）
    /// * `data_scope` - 履约订单范围 Port
    /// * `handlers` - 当前开放 W26 处理人 Port
    ///
    /// # 返回
    /// 返回服务实例。
    /// 构造时不读取、授权或调用供应商。
    pub fn new(
        db: Database,
        gateway: Arc<dyn SupplierGateway>,
        data_scope: Arc<dyn FulfillmentOrderDataScopePort>,
        handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
    ) -> Self {
        Self { db, gateway, data_scope, handlers }
    }
    fn domain(&self) -> SupplierFulfillmentService {
        SupplierFulfillmentService::new(self.db.clone())
            .with_scope(self.data_scope.clone(), self.handlers.clone())
    }
    async fn load_order(&self, id: &str) -> Result<SupplierFulfillmentOrder> {
        Ok(self.domain().load_order(id).await?)
    }

    /// 按动作重验订单范围；当前开放 W26 处理人可处理非跟进人任务。
    pub async fn require_scoped_order(
        &self,
        id: &str,
        actor: &application_core::AuditActor,
        action: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        let handlers =
            self.handlers.open_handler_user_ids(std::slice::from_ref(&id.to_string()), executor).await?;
        let handler = handlers.get(id).map(String::as_str);
        Ok(self.domain().access().require_order(actor, action, id, handler, executor).await?)
    }
}
