//! 供应商履约的跨域命令、事务与供应商调用编排。
//!
//! Process 入口统一管理事务，向领域仓储和步骤传递同一个执行器：
//! - 下单同时登记履约订单、明细、首个动作、集成信封与审计；
//! - 取消/退款同时登记动作头/行、订单状态与集成信封；
//! - 外部结果同时写回订单、动作和信封，必要时创建或更新 W26 正式任务；
//! - 退款结果同时登记退款事实、分配行和订单退款进度；
//! - 调查先登记稳定意图，在事务外取得并冻结结果，再提交证据、任务和独立回执。
//!
//! 供应商 API 仅在事务外经 [`SupplierGateway`] 调用。集成信封和错误任务由
//! erp-integration 拥有；供应商订单、动作、退款事实和命令回执由 erp-supply 拥有；
//! 正式任务由 erp-workflow 拥有，各领域继续执行自己的规则。
//!
//! 下单、取消/退款、结果登记和正式任务命令沿各自幂等身份恢复原结果，不重复
//! 产生正式事实。结果未知时先依既有回执和证据查证，不自动重派供应商动作。

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
pub(crate) mod receipt;
mod refund;
mod refund_result;
mod refund_writes;
mod reject;
mod task_authorization;
mod work_item;

use std::sync::Arc;

use application_core::AuditActor;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::ports::supplier_gateway::SupplierGateway;
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use erp_supply::{FulfillmentExceptionHandlerPort, FulfillmentOrderDataScopePort};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
/// 供应商履约跨域流程。
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
    /// 新采购 API 执行对象禁止进入旧动作网关，避免两套状态同时写同一订单。
    async fn ensure_legacy_order(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        use erp_supply::api_order::ApiOrderExt;
        if self.db.supplier_api_order_executions().find_by_id(id, executor).await?.is_some() {
            return Err(crate::Error::BusinessLogicError("此订单由采购 API 流程自动执行和跟踪，请使用接口配送资料及来源采购处理；不能通过旧发单、重放或人工回执入口修改".into()));
        }
        Ok(())
    }
    /// 创建供应商履约流程实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `gateway` - 供应商动作派发网关（真实 Connector 接入点，只在事务外调用）
    /// * `data_scope` - 履约订单范围 Port
    /// * `handlers` - 当前开放 W26 处理人 Port
    ///
    /// # 返回
    /// 返回流程实例。
    /// 构造时不读取、授权或调用供应商。
    ///
    /// # 错误
    /// 不返回错误。
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

    /// 按动作重验普通订单范围；任务专用入口单独证明任务、责任与版本。
    ///
    /// # 参数
    /// * `id` - 供应商履约订单 ID。
    /// * `actor` - 当前操作人。
    /// * `action` - 本次范围动作。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回当前动作可见的订单。
    ///
    /// # 错误
    /// 开放处理人查询失败，或订单不在该动作范围内时返回对应错误。
    pub async fn require_scoped_order(
        &self,
        id: &str,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        let handlers =
            self.handlers.open_handler_user_ids(std::slice::from_ref(&id.to_string()), executor).await?;
        let handler = handlers.get(id).map(String::as_str);
        Ok(self.domain().access().require_order(actor, action, id, handler, executor).await?)
    }
}
