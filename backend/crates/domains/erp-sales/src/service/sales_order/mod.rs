//! 销售本域快照构造、规则，以及调用方事务内的持久化。

pub mod command;
pub mod contract_binding;
pub mod draft_working_copy;
pub mod extra_cost;
pub mod formalize;
pub mod inquiry;
pub mod lifecycle;
mod list;
pub mod mapper;
pub mod procurement;
pub mod progress;
mod query;
mod rejection;
pub use rejection::reject_order;
mod sellable;
mod working_copy_persistence;

/// Sales operations using only sales repositories and consumer-owned facts.
#[derive(Clone)]
pub struct SalesOrderService {
    pub(crate) db: mongodb::Database,
}
impl SalesOrderService {
    /// 装配本域销售单用例；每次操作由调用方另行提供执行器或所需事实端口。
    ///
    /// 调用方按操作自行提供事务执行器和所需端口。
    /// 返回未执行读写的服务实例。
    ///
    /// 输入约定：
    /// - `db`：销售集合所在数据库。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
