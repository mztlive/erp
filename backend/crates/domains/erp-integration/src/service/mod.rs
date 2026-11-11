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
    /// 绑定本域数据库及完整授权能力。
    /// # 参数
    /// db 为本域数据库，data_scope 为组合根构造的授权访问器。
    /// # 返回
    /// 授权依赖完整的集成服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database, data_scope: IntegrationAccess) -> Self {
        Self { db, data_scope }
    }

    /// 返回绑定当前授权访问器 的对象访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可解析处理人组织与对象范围的访问器。
    ///
    /// # 错误
    /// 无。
    pub fn access(&self) -> IntegrationAccess {
        self.data_scope.clone()
    }
}
