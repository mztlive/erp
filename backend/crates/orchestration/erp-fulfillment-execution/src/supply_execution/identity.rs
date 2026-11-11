//! 单次任务或交接资格核验使用的局部身份能力；调用之间不共享策略状态。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supply::FulfillmentOrderAccess;

use crate::workflow::FulfillmentWorkflow;

/// 同一次核验的身份、工作项和订单范围必须绑定同一 RBAC 实例。
pub struct SupplierFulfillmentIdentity {
    /// 本次核验独立创建的身份状态。
    pub rbac: SharedRbacService,
    /// 使用本次身份的任务资格能力。
    pub workflow: Arc<dyn FulfillmentWorkflow>,
    /// 使用本次身份的交接订单范围能力。
    pub data_scope: FulfillmentOrderAccess,
}

/// 在原命令资格核验位置创建全新身份；构造能力不得执行 I/O。
pub type SupplierFulfillmentIdentityFactory = Arc<dyn Fn() -> SupplierFulfillmentIdentity + Send + Sync>;
