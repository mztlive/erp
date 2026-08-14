//! 域 D25 `supplier_api` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS` 等值。

use entities::supplier_api::{
    BusinessCapabilityConfirmation, SupplierApiCapability, SupplierApiConnection,
    SupplierConnectionCommandReceipt, SupplierHealthCheckRun,
};
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
    /// 追加式采购业务能力确认集合名。
    const SUPPLIER_API_BUSINESS_CONFIRMATIONS: &'static str =
        "supplier_api_business_capability_confirmations";
    /// 后台健康检查运行记录集合名。
    const SUPPLIER_API_HEALTH_CHECK_RUNS: &'static str = "supplier_api_health_check_runs";
    /// 连接治理命令幂等回执集合名。
    const SUPPLIER_API_COMMAND_RECEIPTS: &'static str = "supplier_api_connection_command_receipts";

    /// 连接列表筛选条件类型（定义见 `repository::supplier_api`）。
    type SupplierApiConnectionFilter;

    /// 连接能力列表筛选条件类型（定义见 `repository::supplier_api`）。
    type SupplierApiCapabilityFilter;

    /// 停用连接前由服务端重验的关联业务影响类型。
    type SupplierConnectionImpact;

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

    /// 获取追加式采购业务能力确认集合。
    fn supplier_api_business_confirmations(&self) -> Repository<'_, BusinessCapabilityConfirmation>;

    /// 获取后台健康检查运行记录集合。
    fn supplier_api_health_check_runs(&self) -> Repository<'_, SupplierHealthCheckRun>;

    /// 获取连接治理命令幂等回执集合。
    fn supplier_api_command_receipts(&self) -> Repository<'_, SupplierConnectionCommandReceipt>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SupplierApiRepository` 实例。
    fn supplier_api(&self) -> SupplierApiRepository<'_>;
}

impl SupplierApiExt for Database {
    type SupplierApiConnectionFilter = SupplierApiConnectionFilter;
    type SupplierApiCapabilityFilter = SupplierApiCapabilityFilter;
    type SupplierConnectionImpact = super::super::supplier_api::SupplierConnectionImpact;

    fn supplier_api_connections(&self) -> Repository<'_, SupplierApiConnection> {
        Repository::new(self, Self::SUPPLIER_API_CONNECTIONS)
    }

    fn supplier_api_capabilities(&self) -> Repository<'_, SupplierApiCapability> {
        Repository::new(self, Self::SUPPLIER_API_CAPABILITIES)
    }

    fn supplier_api_business_confirmations(&self) -> Repository<'_, BusinessCapabilityConfirmation> {
        Repository::new(self, Self::SUPPLIER_API_BUSINESS_CONFIRMATIONS)
    }

    fn supplier_api_health_check_runs(&self) -> Repository<'_, SupplierHealthCheckRun> {
        Repository::new(self, Self::SUPPLIER_API_HEALTH_CHECK_RUNS)
    }

    fn supplier_api_command_receipts(&self) -> Repository<'_, SupplierConnectionCommandReceipt> {
        Repository::new(self, Self::SUPPLIER_API_COMMAND_RECEIPTS)
    }

    fn supplier_api(&self) -> SupplierApiRepository<'_> {
        SupplierApiRepository::new(self)
    }
}
