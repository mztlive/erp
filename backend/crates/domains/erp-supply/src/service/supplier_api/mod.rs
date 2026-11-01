//! 连接本域查询、规则和调用方事务内写入。
use mongodb::Database;
pub mod capability;
pub mod command;
pub mod confirmation;
pub mod context;
pub mod creation;
pub mod intent;
mod query;
pub mod reference;
pub mod status;
pub use context::map_command_shape_rejection;
/// 本域服务只持有数据库，不拥有外部连接器、授权或跨域事务。
pub struct SupplierApiService {
    pub(super) db: Database,
}
impl SupplierApiService {
    /// 绑定供应链本域持久化。
    ///
    /// # 参数
    /// * `db` - 供应链数据库。
    ///
    /// # 返回
    /// 返回只持有数据库的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
#[cfg(test)]
mod tests;
