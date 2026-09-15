//! 集成治理写入、正式责任与命令回执的跨域原子流程。
//!
//! 集成治理核心动作（W29 §7/§8.2，数据模型 §6.21、§7.7）：
//! - `inbox_message`：登记（消息层/业务事实层幂等由唯一索引保证，服务层不做
//!   「先查后插」重复性判断）、结果回写（processed / failed+错误任务）；
//! - `integration_error_task` 与 `reconciliation_difference` 的人工动作统一进入
//!   `task_decision` 强命令；非终结动作保持任务 `OPEN`，只有可验证终态允许完成；
//! - 无正式任务的差异只接受 decision-only 直接命令，不得隐式完成或关闭任务；
//! - 责任开始、退回、转交和关闭只由 W02 责任 API 承担。
//!
//! 单集合查询使用 `NoTransaction`；根写入通过 `transaction::run_audited`
//! 复用唯一事务，把本域写入、正式 WorkItem 和审计日志原子提交。
mod creation_writes;
mod error_task;
pub mod evidence_adapter;
mod inbox_message;
pub(crate) mod producer;
mod reconciliation_difference;
mod task_decision;
mod transaction;
mod work_item_factory;
use std::sync::Arc;

use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use erp_integration::service::IntegrationOpsService;
use mongodb::Database;
/// 集成七类写入口；原事务和证据能力由组合根提供。
pub struct IntegrationResolutionProcess {
    pub(super) db: Database,
    pub(super) evidence: Arc<dyn IntegrationEvidenceAuthority>,
}
impl IntegrationResolutionProcess {
    /// 绑定同一权威证据实例；不在构造时执行授权或读取。
    pub fn new(db: Database, evidence: Arc<dyn IntegrationEvidenceAuthority>) -> Self {
        Self { db, evidence }
    }
    fn domain(&self) -> IntegrationOpsService {
        IntegrationOpsService::new(self.db.clone())
    }
}
