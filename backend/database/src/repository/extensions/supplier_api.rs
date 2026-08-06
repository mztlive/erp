//! 域 D25 `supplier_api` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS` 等值。

use entities::supplier_api::{SupplierApiCapability, SupplierApiConnection};
use mongodb::Database;

use super::super::supplier_api::{
    SupplierApiCapabilityFilter, SupplierApiConnectionFilter, SupplierApiRepository,
};
use crate::Repository;

/// 域 D25 仓储访问器。
pub trait SupplierApiExt {
    /// `supplier_api_connection` 集合名。
    const SUPPLIER_API_CONNECTIONS: &'static str = "supplier_api_connections";
    /// `supplier_api_capability` 集合名。
    const SUPPLIER_API_CAPABILITIES: &'static str = "supplier_api_capabilities";

    /// 连接列表筛选条件类型（定义见 `repository::supplier_api`）。
    type SupplierApiConnectionFilter;

    /// 连接能力列表筛选条件类型（定义见 `repository::supplier_api`）。
    type SupplierApiCapabilityFilter;

    /// 获取 `supplier_api_connection` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_api::SupplierApiConnection>`。
    fn supplier_api_connections(&self) -> Repository<'_, SupplierApiConnection>;

    /// 获取 `supplier_api_capability` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_api::SupplierApiCapability>`。
    fn supplier_api_capabilities(&self) -> Repository<'_, SupplierApiCapability>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SupplierApiRepository` 实例。
    fn supplier_api(&self) -> SupplierApiRepository<'_>;
}

impl SupplierApiExt for Database {
    type SupplierApiConnectionFilter = SupplierApiConnectionFilter;
    type SupplierApiCapabilityFilter = SupplierApiCapabilityFilter;

    fn supplier_api_connections(&self) -> Repository<'_, SupplierApiConnection> {
        Repository::new(self, Self::SUPPLIER_API_CONNECTIONS)
    }

    fn supplier_api_capabilities(&self) -> Repository<'_, SupplierApiCapability> {
        Repository::new(self, Self::SUPPLIER_API_CAPABILITIES)
    }

    fn supplier_api(&self) -> SupplierApiRepository<'_> {
        SupplierApiRepository::new(self)
    }
}
