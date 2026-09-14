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
pub use scope::{PurchaseListParams, PurchaseListView};

use crate::{Error, Result};
use erp_identity::SharedRbacService;

/// 使用提供方公开事实装配采购视图；构造本身不访问数据库。
pub struct PurchaseOrderReadService {
    db: mongodb::Database,
    rbac: Option<SharedRbacService>,
}
impl PurchaseOrderReadService {
    /// 以数据库句柄构造只读服务；查询各自保留原无事务读取边界。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    ///
    /// # 返回
    /// 返回未注入 RBAC 的服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 列表、详情与导出必须改用 `with_rbac`，未注入时拒绝放行。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db, rbac: None }
    }

    /// 使用数据库句柄和当前 RBAC 服务构造只读查询服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 共享授权源
    ///
    /// # 返回
    /// 返回可解析采购范围的服务；构造不执行查询。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// HTTP 列表、详情、候选与导出必须经此入口。
    pub fn with_rbac(db: mongodb::Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac: Some(rbac) }
    }

    /// 获取采购范围解析使用的授权源；未注入时保持失败关闭。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已注入的 RBAC 服务。
    ///
    /// # 错误
    /// 未注入时返回内部错误，不得跳过范围解析。
    ///
    /// # 关键业务约束
    /// 不得回退为无范围的全量读取。
    fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac
            .as_ref()
            .ok_or_else(|| Error::Internal("采购单范围解析需要授权源".into()))
    }
}

#[cfg(test)]
mod test_support;
