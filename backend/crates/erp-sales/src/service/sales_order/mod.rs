//! 销售本域快照构造、规则，以及调用方事务内的持久化。

pub mod command;
pub mod contract_binding;
pub mod draft_working_copy;
pub mod formalize;
pub mod lifecycle;
mod list;
pub mod mapper;
mod pricing;
pub mod procurement;
pub mod progress;
mod query;
mod sellable;
mod working_copy_persistence;

/// Sales operations using only sales repositories and consumer-owned facts.
#[derive(Clone)]
pub struct SalesOrderService {
    pub(crate) db: mongodb::Database,
}
impl SalesOrderService {
    /// 构造销售服务，不执行读写。
    ///
    /// 调用方按操作自行提供事务执行器和所需端口。
    ///
    /// # 参数
    /// * `db` - 销售集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行读写的服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
