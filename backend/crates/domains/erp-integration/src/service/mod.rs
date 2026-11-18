//! 集成本域构造、决定规则与持久化服务。

pub mod access;
pub use crate::repository::authorization;
pub mod error_task;
pub mod evidence;
pub mod inbox_message;
pub mod reconciliation_difference;
pub mod scope;
pub mod task_decision;
pub mod validation;

pub use access::IntegrationAccess;
use mongodb::Database;
pub use scope::ScopedIntegrationList;

/// 只持有本域持久化上下文的集成服务。
#[derive(Clone)]
pub struct IntegrationOpsService {
    pub(crate) db: Database,
    pub(crate) data_scope: IntegrationAccess,
}

impl IntegrationOpsService {
    /// 由组合根注入集成数据库和共享授权访问器，供后续查询及命令共用；装配时不访问数据库。
    pub fn new(db: Database, data_scope: IntegrationAccess) -> Self {
        Self { db, data_scope }
    }

    /// 取得当前集成服务的授权访问器，供用例解析处理人组织和对象范围。
    pub fn access(&self) -> IntegrationAccess {
        self.data_scope.clone()
    }
}
