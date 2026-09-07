//! 客户验收登记、过账与反向验收的跨域根流程；单域数量规则与事实写入留履约。

mod commit;
mod completion;
mod create;
mod post;
mod registration;
mod reverse;
pub mod task;

use erp_fulfillment::service::FulfillmentService;
use erp_identity::SharedRbacService;
use erp_read_models::fulfillment_center::FulfillmentReadService;
use mongodb::Database;
use std::sync::Arc;

/// 持有验收根事务，组合履约事实、销售进度、责任任务及原命令审计。
pub struct CustomerAcceptanceProcess {
    db: Database,
    domain: FulfillmentService,
    read: FulfillmentReadService,
    rbac: SharedRbacService,
    object_read: Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}
impl CustomerAcceptanceProcess {
    /// 使用入口已有配置构造流程；领域查询与跨域工作台分别持有实际读取接口。
    pub fn new(
        db: Database,
        read: FulfillmentService,
        rbac: SharedRbacService,
        object_read: Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        Self {
            read: FulfillmentReadService::new(db.clone()),
            db,
            domain: read,
            rbac,
            object_read,
        }
    }
}
