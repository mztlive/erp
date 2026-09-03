//! 域 D09 `supplier` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SupplierExt>::SUPPLIER_ACCOUNTS` 等值。

use entities::supplier::{
    SupplierAccount, SupplierCapability, SupplierCapabilityRevision, SupplierCommercialProfileRevision,
    SupplierProfileCommand, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationRevision, SupplierRatingRevision,
};
use mongodb::Database;

use super::super::supplier::{
    SupplierAccountFilter, SupplierCapabilityFilter, SupplierCommercialProfileFilter, SupplierDetailBundle,
    SupplierListBundle, SupplierListSearchInput, SupplierQualificationFilter,
    SupplierQualificationHealthFilter, SupplierRepository,
};
use crate::Repository;

/// 域 D09 仓储访问器。
pub trait SupplierExt {
    /// `supplier_account` 集合名。
    const SUPPLIER_ACCOUNTS: &'static str = "supplier_accounts";
    /// `supplier_commercial_profile_revision` 集合名。
    const SUPPLIER_COMMERCIAL_PROFILE_REVISIONS: &'static str = "supplier_commercial_profile_revisions";
    /// `supplier_capability` 集合名。
    const SUPPLIER_CAPABILITIES: &'static str = "supplier_capabilities";
    /// `supplier_capability_revision` 集合名。
    const SUPPLIER_CAPABILITY_REVISIONS: &'static str = "supplier_capability_revisions";
    /// `supplier_qualification` 集合名。
    const SUPPLIER_QUALIFICATIONS: &'static str = "supplier_qualifications";
    /// `supplier_qualification_revision` 集合名。
    const SUPPLIER_QUALIFICATION_REVISIONS: &'static str = "supplier_qualification_revisions";
    /// `supplier_qualification_capability` 集合名。
    const SUPPLIER_QUALIFICATION_CAPABILITIES: &'static str = "supplier_qualification_capabilities";
    /// `supplier_rating_revision` 集合名。
    const SUPPLIER_RATING_REVISIONS: &'static str = "supplier_rating_revisions";
    /// `supplier_profile_command` 根级命令去重结果集合名。
    const SUPPLIER_PROFILE_COMMANDS: &'static str = "supplier_profile_commands";

    /// 供应商角色列表筛选条件类型（定义见 `repository::supplier`）。
    type SupplierAccountFilter;
    /// 商务结算版本列表筛选条件类型（定义见 `repository::supplier`）。
    type SupplierCommercialProfileFilter;
    /// 能力列表筛选条件类型（定义见 `repository::supplier`）。
    type SupplierCapabilityFilter;
    /// 资质列表筛选条件类型（定义见 `repository::supplier`）。
    type SupplierQualificationFilter;
    /// 供应商列表事实束搜索输入类型（定义见 `repository::supplier`）。
    type SupplierListSearchInput;
    /// 供应商列表事实束类型（定义见 `repository::supplier`）。
    type SupplierListBundle;
    /// 供应商详情事实束类型（定义见 `repository::supplier`）。
    type SupplierDetailBundle;
    /// 供应商列表资质健康筛选类型（定义见 `repository::supplier`）。
    type SupplierQualificationHealthFilter;

    /// 获取 `supplier_account` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierAccount>`。
    fn supplier_accounts(&self) -> Repository<'_, SupplierAccount>;

    /// 获取 `supplier_commercial_profile_revision` 集合的 Repository（追加式修订）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierCommercialProfileRevision>`。
    fn supplier_commercial_profile_revisions(&self) -> Repository<'_, SupplierCommercialProfileRevision>;

    /// 获取 `supplier_capability` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierCapability>`。
    fn supplier_capabilities(&self) -> Repository<'_, SupplierCapability>;

    /// 获取 `supplier_capability_revision` 集合的 Repository（追加式修订）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierCapabilityRevision>`。
    fn supplier_capability_revisions(&self) -> Repository<'_, SupplierCapabilityRevision>;

    /// 获取 `supplier_qualification` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierQualification>`。
    fn supplier_qualifications(&self) -> Repository<'_, SupplierQualification>;

    /// 获取 `supplier_qualification_revision` 集合的 Repository（追加式修订）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierQualificationRevision>`。
    fn supplier_qualification_revisions(&self) -> Repository<'_, SupplierQualificationRevision>;

    /// 获取 `supplier_qualification_capability` 集合的 Repository（纯关联行）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierQualificationCapability>`。
    fn supplier_qualification_capabilities(&self) -> Repository<'_, SupplierQualificationCapability>;

    /// 获取 `supplier_rating_revision` 集合的 Repository（追加式修订）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier::SupplierRatingRevision>`。
    fn supplier_rating_revisions(&self) -> Repository<'_, SupplierRatingRevision>;

    /// 获取供应商资料根级命令去重仓储。
    fn supplier_profile_commands(&self) -> Repository<'_, SupplierProfileCommand>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SupplierRepository` 实例。
    fn supplier(&self) -> SupplierRepository<'_>;
}

impl SupplierExt for Database {
    type SupplierAccountFilter = SupplierAccountFilter;
    type SupplierCommercialProfileFilter = SupplierCommercialProfileFilter;
    type SupplierCapabilityFilter = SupplierCapabilityFilter;
    type SupplierQualificationFilter = SupplierQualificationFilter;
    type SupplierListSearchInput = SupplierListSearchInput;
    type SupplierListBundle = SupplierListBundle;
    type SupplierDetailBundle = SupplierDetailBundle;
    type SupplierQualificationHealthFilter = SupplierQualificationHealthFilter;

    fn supplier_accounts(&self) -> Repository<'_, SupplierAccount> {
        Repository::new(self, Self::SUPPLIER_ACCOUNTS)
    }

    fn supplier_commercial_profile_revisions(&self) -> Repository<'_, SupplierCommercialProfileRevision> {
        Repository::new(self, Self::SUPPLIER_COMMERCIAL_PROFILE_REVISIONS)
    }

    fn supplier_capabilities(&self) -> Repository<'_, SupplierCapability> {
        Repository::new(self, Self::SUPPLIER_CAPABILITIES)
    }

    fn supplier_capability_revisions(&self) -> Repository<'_, SupplierCapabilityRevision> {
        Repository::new(self, Self::SUPPLIER_CAPABILITY_REVISIONS)
    }

    fn supplier_qualifications(&self) -> Repository<'_, SupplierQualification> {
        Repository::new(self, Self::SUPPLIER_QUALIFICATIONS)
    }

    fn supplier_qualification_revisions(&self) -> Repository<'_, SupplierQualificationRevision> {
        Repository::new(self, Self::SUPPLIER_QUALIFICATION_REVISIONS)
    }

    fn supplier_qualification_capabilities(&self) -> Repository<'_, SupplierQualificationCapability> {
        Repository::new(self, Self::SUPPLIER_QUALIFICATION_CAPABILITIES)
    }

    fn supplier_rating_revisions(&self) -> Repository<'_, SupplierRatingRevision> {
        Repository::new(self, Self::SUPPLIER_RATING_REVISIONS)
    }

    fn supplier_profile_commands(&self) -> Repository<'_, SupplierProfileCommand> {
        Repository::new(self, Self::SUPPLIER_PROFILE_COMMANDS)
    }

    fn supplier(&self) -> SupplierRepository<'_> {
        SupplierRepository::new(self)
    }
}
