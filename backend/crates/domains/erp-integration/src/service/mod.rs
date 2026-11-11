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
    /// 使用调用方数据库句柄创建本域服务；授权访问器 失败关闭。
    ///
    /// # 参数
    /// * `db` - 本域集合所在数据库
    ///
    /// # 返回
    /// 返回未接线范围解析的服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 列表、详情和写命令必须改用 [`Self::with_scope`]。
    pub fn new(db: Database) -> Self {
        Self { db: db.clone(), data_scope: IntegrationAccess::unwired(db) }
    }

    /// 绑定已接入统一授权服务的授权访问器。
    ///
    /// # 参数
    /// * `db` - 本域集合所在数据库
    /// * `data_scope` - 组合层注入的集成授权访问器
    ///
    /// # 返回
    /// 返回可解析范围的服务。
    ///
    /// # 错误
    /// 无。
    pub fn with_scope(db: Database, data_scope: IntegrationAccess) -> Self {
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
