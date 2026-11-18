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
mod failure;
mod follow_up;
mod handover;
mod identity;
mod investigate;
mod legacy;
mod place;
pub mod receipt;
mod refund_result;
mod refund_writes;
mod reject;
pub(crate) mod task_authorization;
mod work_item;

use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::Permission;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::ports::supplier_gateway::SupplierGateway;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use erp_supply::{FulfillmentExceptionHandlerPort, FulfillmentOrderAccess};
pub use identity::SupplierFulfillmentIdentity;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
/// 供应商履约跨域流程。
///
/// 提供供应商子订单的下单、查询、取消/退款动作提交与外部结果登记编排。
/// 本类型承接跨域写入与外部调用，纯列表由本域服务提供。
#[derive(Clone)]
pub struct SupplierFulfillmentProcess {
    identity: SupplierFulfillmentIdentity,
    db: Database,
    registry: Option<Arc<dyn SupplierReferenceRegistry>>,
    gateway: Arc<dyn SupplierGateway>,
    data_scope: FulfillmentOrderAccess,
    handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
}
impl SupplierFulfillmentProcess {
    /// 应用装配供应商履约流程时，注入共享身份和范围检查、W26 处理人能力及真实派发网关。
    /// 构造不读取数据、不执行授权或调用供应商；后续网关派发只在事务外执行。
    pub fn new(
        db: Database,
        gateway: Arc<dyn SupplierGateway>,
        data_scope: FulfillmentOrderAccess,
        handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
        identity: SupplierFulfillmentIdentity,
    ) -> Self {
        Self { identity, db, gateway, data_scope, handlers, registry: None }
    }
    /// 组合根通过共享注册表为后续领域用例提供实际供应商适配器能力，不在此调用供应商。
    pub fn with_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }
    fn domain(&self) -> SupplierFulfillmentService {
        self.domain_with_scope(self.data_scope.clone())
    }

    fn domain_with_scope(&self, data_scope: FulfillmentOrderAccess) -> SupplierFulfillmentService {
        let service =
            SupplierFulfillmentService::new(self.db.clone()).with_scope(data_scope, self.handlers.clone());
        match &self.registry {
            Some(registry) => service.with_registry(registry.clone()),
            None => service,
        }
    }

    // 先重验订单范围，再排除由采购 API 流程执行的订单。
    async fn require_legacy_order(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        let order = self.data_scope.require_order(actor, permission, id, executor).await?;
        legacy::ensure(&self.db, id, executor).await?;
        Ok(order)
    }
}
