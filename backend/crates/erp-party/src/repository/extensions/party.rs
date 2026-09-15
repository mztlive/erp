//! 域 D07 `party` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PartyExt>::PARTIES` 等值。

use mongodb::Database;

use super::super::party::{
    PartyAddressFilter, PartyBankAccountFilter, PartyContactFilter, PartyDomainRepository, PartyFilter,
    PartyRevisionFilter, PartyTaxProfileFilter,
};
use crate::repository::owned::{
    PartyAddressRepository, PartyBankAccountRepository, PartyContactRepository, PartyRepository,
    PartyRevisionRepository, PartyTaxProfileRepository,
};

/// 域 D07 仓储访问器。
pub trait PartyExt {
    /// `party` 集合名。
    const PARTIES: &'static str = "parties";
    /// `party_revision` 集合名。
    const PARTY_REVISIONS: &'static str = "party_revisions";
    /// `party_contact` 集合名。
    const PARTY_CONTACTS: &'static str = "party_contacts";
    /// `party_address` 集合名。
    const PARTY_ADDRESSES: &'static str = "party_addresses";
    /// `party_tax_profile` 集合名。
    const PARTY_TAX_PROFILES: &'static str = "party_tax_profiles";
    /// `party_bank_account` 集合名。
    const PARTY_BANK_ACCOUNTS: &'static str = "party_bank_accounts";

    /// 主体列表筛选条件类型（定义见 `repository::party`）。
    type PartyFilter;
    /// 主体修订列表筛选条件类型（定义见 `repository::party`）。
    type PartyRevisionFilter;
    /// 联系人列表筛选条件类型（定义见 `repository::party`）。
    type PartyContactFilter;
    /// 地址列表筛选条件类型（定义见 `repository::party`）。
    type PartyAddressFilter;
    /// 税务资料列表筛选条件类型（定义见 `repository::party`）。
    type PartyTaxProfileFilter;
    /// 银行账户列表筛选条件类型（定义见 `repository::party`）。
    type PartyBankAccountFilter;

    /// 获取 `party` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PartyRepository<'_>`。
    fn parties(&self) -> PartyRepository<'_>;

    /// 获取 `party_revision` 集合的 Repository（追加式修订，无软删除）。
    ///
    /// # 返回
    /// 返回 `PartyRevisionRepository<'_>`。
    fn party_revisions(&self) -> PartyRevisionRepository<'_>;

    /// 获取 `party_contact` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PartyContactRepository<'_>`。
    fn party_contacts(&self) -> PartyContactRepository<'_>;

    /// 获取 `party_address` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PartyAddressRepository<'_>`。
    fn party_addresses(&self) -> PartyAddressRepository<'_>;

    /// 获取 `party_tax_profile` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PartyTaxProfileRepository<'_>`。
    fn party_tax_profiles(&self) -> PartyTaxProfileRepository<'_>;

    /// 获取 `party_bank_account` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PartyBankAccountRepository<'_>`。
    fn party_bank_accounts(&self) -> PartyBankAccountRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PartyDomainRepository` 实例。
    fn party(&self) -> PartyDomainRepository<'_>;
}

impl PartyExt for Database {
    type PartyFilter = PartyFilter;
    type PartyRevisionFilter = PartyRevisionFilter;
    type PartyContactFilter = PartyContactFilter;
    type PartyAddressFilter = PartyAddressFilter;
    type PartyTaxProfileFilter = PartyTaxProfileFilter;
    type PartyBankAccountFilter = PartyBankAccountFilter;

    fn parties(&self) -> PartyRepository<'_> {
        PartyRepository::new(self, Self::PARTIES)
    }

    fn party_revisions(&self) -> PartyRevisionRepository<'_> {
        PartyRevisionRepository::new(self, Self::PARTY_REVISIONS)
    }

    fn party_contacts(&self) -> PartyContactRepository<'_> {
        PartyContactRepository::new(self, Self::PARTY_CONTACTS)
    }

    fn party_addresses(&self) -> PartyAddressRepository<'_> {
        PartyAddressRepository::new(self, Self::PARTY_ADDRESSES)
    }

    fn party_tax_profiles(&self) -> PartyTaxProfileRepository<'_> {
        PartyTaxProfileRepository::new(self, Self::PARTY_TAX_PROFILES)
    }

    fn party_bank_accounts(&self) -> PartyBankAccountRepository<'_> {
        PartyBankAccountRepository::new(self, Self::PARTY_BANK_ACCOUNTS)
    }

    fn party(&self) -> PartyDomainRepository<'_> {
        PartyDomainRepository::new(self)
    }
}
