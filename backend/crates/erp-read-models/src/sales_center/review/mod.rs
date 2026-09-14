//! 销售变更单详情与审批绑定的跨域只读服务。

mod dto;
mod projection;
mod query;

pub use dto::*;
pub use query::SalesChangeListView;

use crate::{Error, Result};
use erp_identity::SharedRbacService;

/// 销售变更详情读取器；不负责审批命令或业务写入。
pub struct SalesChangeReadService {
    db: mongodb::Database,
    rbac: Option<SharedRbacService>,
}
impl SalesChangeReadService {
    /// 使用销售和审批集合所在数据库创建详情读取器。
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
    /// HTTP 列表与详情必须改用 `with_rbac`，未注入时失败关闭。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db, rbac: None }
    }

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
        Self { db, rbac: Some(rbac) }
    }

    /// 获取销售范围解析使用的授权源。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已注入的 RBAC。
    ///
    /// # 错误
    /// 未注入时拒绝。
    ///
    /// # 关键业务约束
    /// 不得退回路由级授权或补公司范围。
    fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac
            .as_ref()
            .ok_or_else(|| Error::Internal("销售变更单读取需要授权源".into()))
    }
}
