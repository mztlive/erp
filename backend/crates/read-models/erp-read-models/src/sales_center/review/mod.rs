//! 销售变更单详情与审批绑定的跨域只读服务。

mod approval_query;
mod dto;
mod list;
mod procurement;
mod projection;
mod query;
mod submitted_target;

pub use dto::*;
use erp_identity::SharedRbacService;
pub use query::SalesChangeListView;

/// 销售变更详情读取器；不负责审批命令或业务写入。
pub struct SalesChangeReadService {
    db: mongodb::Database,
    rbac: SharedRbacService,
}
impl SalesChangeReadService {
    /// 使用数据库句柄和当前 RBAC 服务构造只读查询服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 当前 RBAC 快照
    ///
    /// # 返回
    /// 返回可解析销售范围的服务；构造不执行查询。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 变更单沿原销售单当前负责人和 `business_org_unit_id` 接入。
    pub fn with_rbac(db: mongodb::Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}
