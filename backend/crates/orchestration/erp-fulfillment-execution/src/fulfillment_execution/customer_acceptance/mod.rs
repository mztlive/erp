//! 客户验收登记、过账与反向验收的跨域根流程；单域数量规则与事实写入留履约。

mod commit;
mod completion;
mod create;
mod evidence;
mod number;
mod post;
mod registration;
mod replay;
mod reverse;
pub mod task;

use std::sync::Arc;

use erp_identity::AuthorizationService;
use mongodb::Database;

use crate::workflow::FulfillmentWorkflow;

/// 持有验收根事务，组合履约事实、销售进度、责任任务及原命令审计。
pub struct CustomerAcceptanceProcess {
    workflow: Arc<dyn FulfillmentWorkflow>,
    db: Database,
    authorization: Arc<AuthorizationService>,
}
impl CustomerAcceptanceProcess {
    /// 使用入口已有配置构造流程；查询及写入步骤共用调用方数据库。
    ///
    /// `db` 为业务数据库。读模型由此新建；`authorization` 为组合根构造并共享的统一授权服务；`workflow` 为应用装配的工作流绑定、材料和正式任务能力。
    ///
    /// 返回持有同一数据库、工作流端口和共享授权服务的验收流程。
    pub fn new(
        db: Database,
        authorization: Arc<AuthorizationService>,
        workflow: Arc<dyn FulfillmentWorkflow>,
    ) -> Self {
        Self { workflow, db, authorization }
    }
}
