//! 域 D27 `projection` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTIONS` 等值。

use entities::projection::{
    SalesOrderProjection, SalesOrderProjectionDelivery, SalesOrderProjectionRevision,
};
use mongodb::Database;

use super::super::projection::{
    ProjectionRepository, SalesOrderProjectionDeliveryFilter, SalesOrderProjectionFilter,
};
use crate::Repository;

/// 域 D27 仓储访问器。
pub trait ProjectionExt {
    /// `sales_order_projection` 集合名。
    const SALES_ORDER_PROJECTIONS: &'static str = "sales_order_projections";
    /// `sales_order_projection_revision` 集合名。
    const SALES_ORDER_PROJECTION_REVISIONS: &'static str = "sales_order_projection_revisions";
    /// `sales_order_projection_delivery` 集合名。
    const SALES_ORDER_PROJECTION_DELIVERIES: &'static str = "sales_order_projection_deliveries";

    /// 投影列表筛选条件类型（定义见 `repository::projection`）。
    type SalesOrderProjectionFilter;

    /// 投影下发列表筛选条件类型（定义见 `repository::projection`）。
    type SalesOrderProjectionDeliveryFilter;

    /// 获取 `sales_order_projection` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::projection::SalesOrderProjection>`。
    fn sales_order_projections(&self) -> Repository<'_, SalesOrderProjection>;

    /// 获取 `sales_order_projection_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::projection::SalesOrderProjectionRevision>`。
    fn sales_order_projection_revisions(&self) -> Repository<'_, SalesOrderProjectionRevision>;

    /// 获取 `sales_order_projection_delivery` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::projection::SalesOrderProjectionDelivery>`。
    fn sales_order_projection_deliveries(&self) -> Repository<'_, SalesOrderProjectionDelivery>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `ProjectionRepository` 实例。
    fn projection(&self) -> ProjectionRepository<'_>;
}

impl ProjectionExt for Database {
    type SalesOrderProjectionFilter = SalesOrderProjectionFilter;
    type SalesOrderProjectionDeliveryFilter = SalesOrderProjectionDeliveryFilter;

    fn sales_order_projections(&self) -> Repository<'_, SalesOrderProjection> {
        Repository::new(self, Self::SALES_ORDER_PROJECTIONS)
    }

    fn sales_order_projection_revisions(&self) -> Repository<'_, SalesOrderProjectionRevision> {
        Repository::new(self, Self::SALES_ORDER_PROJECTION_REVISIONS)
    }

    fn sales_order_projection_deliveries(&self) -> Repository<'_, SalesOrderProjectionDelivery> {
        Repository::new(self, Self::SALES_ORDER_PROJECTION_DELIVERIES)
    }

    fn projection(&self) -> ProjectionRepository<'_> {
        ProjectionRepository::new(self)
    }
}
