//! 客户验收登记、过账与反向验收的跨域根流程；单域数量规则与事实写入留履约。

mod commit;
mod completion;
mod create;
mod evidence;
mod post;
mod registration;
mod reverse;
pub mod task;

use std::sync::Arc;

use erp_fulfillment::service::FulfillmentService;
use erp_identity::SharedRbacService;
use erp_read_models::fulfillment_center::FulfillmentReadService;
use mongodb::Database;

use super::task::FulfillmentIdentityFactory;
use crate::workflow::FulfillmentWorkflow;

/// 持有验收根事务，组合履约事实、销售进度、责任任务及原命令审计。
pub struct CustomerAcceptanceProcess {
    workflow: Arc<dyn FulfillmentWorkflow>,
    db: Database,
    domain: FulfillmentService,
    read: FulfillmentReadService,
    rbac: SharedRbacService,
    task_identity: FulfillmentIdentityFactory,
}
impl CustomerAcceptanceProcess {
    /// 使用入口已有配置构造流程；领域查询与跨域工作台分别持有实际读取接口。
    ///
    /// # 参数
    /// * `db` - 业务数据库。读模型由此新建。
    /// * `read` - 已构造的履约领域服务，存为领域写入端。
    /// * `rbac` - 调用方授权源。
    /// * `task_identity` - 每次任务资格核验独立创建身份的工厂。
    /// * `workflow` - 应用装配的工作流绑定、材料和正式任务能力。
    ///
    /// # 返回
    /// 返回持有读模型、领域服务和授权源的验收流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        read: FulfillmentService,
        rbac: SharedRbacService,
        task_identity: FulfillmentIdentityFactory,
        workflow: Arc<dyn FulfillmentWorkflow>,
    ) -> Self {
        Self {
            workflow,
            read: FulfillmentReadService::new(db.clone()),
            db,
            domain: read,
            rbac,
            task_identity,
        }
    }
}
