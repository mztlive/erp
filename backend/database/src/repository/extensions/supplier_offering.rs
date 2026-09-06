//! 域 D24 供应商供给仓储访问器。

use crate::repository::owned::{
    SupplierOfferingAvailabilityRepository, SupplierOfferingCommandRepository, SupplierOfferingRepository,
    SupplierOfferingRevisionRepository,
};
use mongodb::Database;

use super::super::supplier_offering::{SupplierOfferingDomainRepository, SupplierOfferingFilter};

pub use super::super::supplier_offering::list_filter::SupplierOfferingListQuery;

/// 供应商供给仓储访问器。
pub trait SupplierOfferingExt: Sized {
    /// 供给稳定身份集合。
    const SUPPLIER_OFFERINGS: &'static str = "supplier_offerings";
    /// 供给商业条款修订集合。
    const SUPPLIER_OFFERING_REVISIONS: &'static str = "supplier_offering_revisions";
    /// 实时可供投影集合。
    const SUPPLIER_OFFERING_AVAILABILITIES: &'static str = "supplier_offering_availabilities";
    /// 供给幂等命令集合。
    const SUPPLIER_OFFERING_COMMANDS: &'static str = "supplier_offering_commands";

    /// 供给列表筛选条件类型。
    type SupplierOfferingFilter;

    /// 供给列表高层查询条件类型（定义见 `repository::supplier_offering::list_filter`）。
    type OfferingListQuery;

    /// 获取供给稳定身份集合。
    ///
    /// # 返回
    /// 返回通用供给仓储。
    fn supplier_offerings(&self) -> SupplierOfferingRepository<'_>;

    /// 获取供给商业条款修订集合。
    ///
    /// # 返回
    /// 返回通用修订仓储。
    fn supplier_offering_revisions(&self) -> SupplierOfferingRevisionRepository<'_>;

    /// 获取实时可供投影集合。
    ///
    /// # 返回
    /// 返回通用可供投影仓储。
    fn supplier_offering_availabilities(&self) -> SupplierOfferingAvailabilityRepository<'_>;

    /// 获取供给写命令去重集合。
    ///
    /// # 返回
    /// 返回通用命令仓储。
    fn supplier_offering_commands(&self) -> SupplierOfferingCommandRepository<'_>;

    /// 获取供给跨集合事务仓储。
    ///
    /// # 返回
    /// 返回供给聚合仓储。
    fn supplier_offering_repository(&self) -> SupplierOfferingDomainRepository<'_>;
}

impl SupplierOfferingExt for Database {
    type SupplierOfferingFilter = SupplierOfferingFilter;
    type OfferingListQuery = SupplierOfferingListQuery;

    fn supplier_offerings(&self) -> SupplierOfferingRepository<'_> {
        SupplierOfferingRepository::new(self, Self::SUPPLIER_OFFERINGS)
    }

    fn supplier_offering_revisions(&self) -> SupplierOfferingRevisionRepository<'_> {
        SupplierOfferingRevisionRepository::new(self, Self::SUPPLIER_OFFERING_REVISIONS)
    }

    fn supplier_offering_availabilities(&self) -> SupplierOfferingAvailabilityRepository<'_> {
        SupplierOfferingAvailabilityRepository::new(self, Self::SUPPLIER_OFFERING_AVAILABILITIES)
    }

    fn supplier_offering_commands(&self) -> SupplierOfferingCommandRepository<'_> {
        SupplierOfferingCommandRepository::new(self, Self::SUPPLIER_OFFERING_COMMANDS)
    }

    fn supplier_offering_repository(&self) -> SupplierOfferingDomainRepository<'_> {
        SupplierOfferingDomainRepository::new(self)
    }
}
