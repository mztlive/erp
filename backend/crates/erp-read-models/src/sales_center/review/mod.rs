//! 销售变更单详情与审批绑定的跨域只读服务。

mod dto;
mod projection;
mod query;

pub use dto::*;

/// 销售变更详情读取器；不负责审批命令或业务写入。
pub struct SalesChangeReadService {
    db: mongodb::Database,
}
impl SalesChangeReadService {
    /// 使用销售和审批集合所在数据库创建详情读取器。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
