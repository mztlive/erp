//! 采购列表、对象中心、创建依据与责任规则的跨域只读组合。
pub mod access;
mod approval;
mod approval_query;
mod change;
mod creation_basis;
pub mod dto;
pub mod procurement_responsibility;
mod query;
pub mod repository;
mod scope;
use std::sync::Arc;

pub use change::query::PurchaseChangeListView;
use erp_procurement::{FailClosedPurchaseDataScopePort, PurchaseDataScopePort};
pub use scope::{PurchaseListParams, PurchaseListView};

/// 使用提供方公开事实装配采购视图；构造本身不访问数据库。
pub struct PurchaseOrderReadService {
    db: mongodb::Database,
    data_scope: Arc<dyn PurchaseDataScopePort>,
}
impl PurchaseOrderReadService {
    /// 以数据库句柄构造只读服务；查询各自保留原无事务读取边界。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回未注入范围 Port 的服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 列表、详情与导出必须改用 `with_scope`，未注入时失败关闭。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db, data_scope: FailClosedPurchaseDataScopePort::shared() }
    }

    /// 使用数据库句柄和采购范围 Port 构造只读查询服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `data_scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回可解析采购范围的服务；构造不执行查询。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// HTTP 列表、详情、候选、导出、变更单与退货必须经此入口。
    pub fn with_scope(db: mongodb::Database, data_scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        Self { db, data_scope }
    }

    /// 构造绑定当前 Port 的采购访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回无授权缓存的访问器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在此回退构造身份域 Service。
    fn access(&self) -> access::PurchaseAccess {
        access::PurchaseAccess::new(self.db.clone(), Arc::clone(&self.data_scope))
    }
}

#[cfg(test)]
mod test_support;
