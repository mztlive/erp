//! 供应执行共享的授权、工作流与订单访问能力。

use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_supply::FulfillmentOrderAccess;

use crate::workflow::FulfillmentWorkflow;

/// 身份、工作项和订单范围必须绑定同一共享授权实例。
#[derive(Clone)]
pub struct SupplierFulfillmentIdentity {
    /// 进程共享的授权服务。
    pub rbac: Arc<AuthorizationService>,
    /// 使用本次身份的任务资格能力。
    pub workflow: Arc<dyn FulfillmentWorkflow>,
    /// 使用本次身份的交接订单范围能力。
    pub data_scope: FulfillmentOrderAccess,
}
