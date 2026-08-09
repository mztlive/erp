//! 域 D24 供应商供给仓储访问器。

use entities::supplier_offering::{
    SupplierOffering, SupplierOfferingAvailability, SupplierOfferingCommand, SupplierOfferingRevision,
};
use mongodb::Database;

use super::super::supplier_offering::{SupplierOfferingFilter, SupplierOfferingRepository};
use crate::Repository;

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

    /// 获取供给稳定身份集合。
    ///
    /// # 返回
    /// 返回通用供给仓储。
    fn supplier_offerings(&self) -> Repository<'_, SupplierOffering>;

    /// 获取供给商业条款修订集合。
    ///
    /// # 返回
    /// 返回通用修订仓储。
    fn supplier_offering_revisions(&self) -> Repository<'_, SupplierOfferingRevision>;

    /// 获取实时可供投影集合。
    ///
    /// # 返回
    /// 返回通用可供投影仓储。
    fn supplier_offering_availabilities(&self) -> Repository<'_, SupplierOfferingAvailability>;

    /// 获取供给写命令去重集合。
    ///
    /// # 返回
    /// 返回通用命令仓储。
    fn supplier_offering_commands(&self) -> Repository<'_, SupplierOfferingCommand>;

    /// 获取供给跨集合事务仓储。
    ///
    /// # 返回
    /// 返回供给聚合仓储。
    fn supplier_offering_repository(&self) -> SupplierOfferingRepository<'_>;
}

impl SupplierOfferingExt for Database {
    type SupplierOfferingFilter = SupplierOfferingFilter;

    fn supplier_offerings(&self) -> Repository<'_, SupplierOffering> {
        Repository::new(self, Self::SUPPLIER_OFFERINGS)
    }

    fn supplier_offering_revisions(&self) -> Repository<'_, SupplierOfferingRevision> {
        Repository::new(self, Self::SUPPLIER_OFFERING_REVISIONS)
    }

    fn supplier_offering_availabilities(&self) -> Repository<'_, SupplierOfferingAvailability> {
        Repository::new(self, Self::SUPPLIER_OFFERING_AVAILABILITIES)
    }

    fn supplier_offering_commands(&self) -> Repository<'_, SupplierOfferingCommand> {
        Repository::new(self, Self::SUPPLIER_OFFERING_COMMANDS)
    }

    fn supplier_offering_repository(&self) -> SupplierOfferingRepository<'_> {
        SupplierOfferingRepository::new(self)
    }
}
