//! 客户验收登记、过账与反向验收的跨域根流程；单域数量规则与事实写入留履约。

mod commit;
mod completion;
mod post;
mod reverse;

use erp_identity::SharedRbacService;
use mongodb::Database;
use services::fulfillment::FulfillmentService;
use std::sync::Arc;

/// 持有验收根事务，组合履约事实、销售进度、责任任务及原命令审计。
pub struct CustomerAcceptanceProcess {
    db: Database,
    read: FulfillmentService,
    rbac: SharedRbacService,
    object_read: Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}
impl CustomerAcceptanceProcess {
    /// 使用入口已有配置构造流程；查询服务保留原指纹、敏感信息和对象读取上下文。
    pub fn new(
        db: Database,
        read: FulfillmentService,
        rbac: SharedRbacService,
        object_read: Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        Self {
            db,
            read,
            rbac,
            object_read,
        }
    }
}
