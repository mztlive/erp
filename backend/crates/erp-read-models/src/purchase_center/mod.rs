//! 采购列表、对象中心、创建依据与责任规则的跨域只读组合。
mod approval;
mod approval_query;
mod change;
mod creation_basis;
pub mod dto;
pub mod procurement_responsibility;
mod query;
pub mod repository;

/// 使用提供方公开事实装配采购视图；构造本身不访问数据库。
pub struct PurchaseOrderReadService {
    db: mongodb::Database,
}
impl PurchaseOrderReadService {
    /// 以数据库句柄构造只读服务；查询各自保留原无事务读取边界。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}

#[cfg(test)]
mod test_support;
