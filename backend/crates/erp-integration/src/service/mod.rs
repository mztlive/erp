//! 集成本域构造、决定规则与持久化服务。

pub mod access;
pub mod error_task;
pub mod evidence;
pub mod inbox_message;
pub mod reconciliation_difference;
pub mod scope;
pub mod task_decision;
pub mod validation;

use std::sync::Arc;

pub use access::IntegrationAccess;
use mongodb::Database;
pub use scope::ScopedIntegrationList;

use crate::ports::{FailClosedIntegrationDataScopePort, IntegrationDataScopePort};

/// 只持有本域持久化上下文的集成服务。
#[derive(Clone)]
pub struct IntegrationOpsService {
    pub(crate) db: Database,
    pub(crate) data_scope: Arc<dyn IntegrationDataScopePort>,
}

impl IntegrationOpsService {
    /// 使用调用方数据库句柄创建本域服务；范围 Port 失败关闭。
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
        Self { db, data_scope: FailClosedIntegrationDataScopePort::shared() }
    }

    /// 绑定已接入公共解析器的范围 Port。
    ///
    /// # 参数
    /// * `db` - 本域集合所在数据库
    /// * `data_scope` - 组合层注入的集成范围 Port
    ///
    /// # 返回
    /// 返回可解析范围的服务。
    ///
    /// # 错误
    /// 无。
    pub fn with_scope(db: Database, data_scope: Arc<dyn IntegrationDataScopePort>) -> Self {
        Self { db, data_scope }
    }

    /// 返回绑定当前范围 Port 的对象访问器。
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
        IntegrationAccess::new(self.data_scope.clone())
    }
}
