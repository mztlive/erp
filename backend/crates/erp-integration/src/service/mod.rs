//! 集成本域构造、决定规则与持久化服务。

pub mod error_task;
pub mod evidence;
pub mod inbox_message;
pub mod reconciliation_difference;
pub mod task_decision;
pub mod validation;

use mongodb::Database;

/// 只持有本域持久化上下文的集成服务。
#[derive(Clone)]
pub struct IntegrationOpsService {
    db: Database,
}

impl IntegrationOpsService {
    /// 使用调用方数据库句柄创建本域服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
