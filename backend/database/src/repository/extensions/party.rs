//! 域 D07 `party` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PartyExt>::PARTIES` 等值。

use entities::party::{Party, PartyAddress, PartyBankAccount, PartyContact, PartyRevision, PartyTaxProfile};
use mongodb::Database;

use super::super::party::{
    PartyAddressFilter, PartyBankAccountFilter, PartyContactFilter, PartyFilter, PartyRepository,
    PartyRevisionFilter, PartyTaxProfileFilter,
};
use crate::Repository;

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
    /// 返回 `Repository<'_, entities::party::Party>`。
    fn parties(&self) -> Repository<'_, Party>;

    /// 获取 `party_revision` 集合的 Repository（追加式修订，无软删除）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::party::PartyRevision>`。
    fn party_revisions(&self) -> Repository<'_, PartyRevision>;

    /// 获取 `party_contact` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::party::PartyContact>`。
    fn party_contacts(&self) -> Repository<'_, PartyContact>;

    /// 获取 `party_address` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::party::PartyAddress>`。
    fn party_addresses(&self) -> Repository<'_, PartyAddress>;

    /// 获取 `party_tax_profile` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::party::PartyTaxProfile>`。
    fn party_tax_profiles(&self) -> Repository<'_, PartyTaxProfile>;

    /// 获取 `party_bank_account` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::party::PartyBankAccount>`。
    fn party_bank_accounts(&self) -> Repository<'_, PartyBankAccount>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PartyRepository` 实例。
    fn party(&self) -> PartyRepository<'_>;
}

impl PartyExt for Database {
    type PartyFilter = PartyFilter;
    type PartyRevisionFilter = PartyRevisionFilter;
    type PartyContactFilter = PartyContactFilter;
    type PartyAddressFilter = PartyAddressFilter;
    type PartyTaxProfileFilter = PartyTaxProfileFilter;
    type PartyBankAccountFilter = PartyBankAccountFilter;

    fn parties(&self) -> Repository<'_, Party> {
        Repository::new(self, Self::PARTIES)
    }

    fn party_revisions(&self) -> Repository<'_, PartyRevision> {
        Repository::new(self, Self::PARTY_REVISIONS)
    }

    fn party_contacts(&self) -> Repository<'_, PartyContact> {
        Repository::new(self, Self::PARTY_CONTACTS)
    }

    fn party_addresses(&self) -> Repository<'_, PartyAddress> {
        Repository::new(self, Self::PARTY_ADDRESSES)
    }

    fn party_tax_profiles(&self) -> Repository<'_, PartyTaxProfile> {
        Repository::new(self, Self::PARTY_TAX_PROFILES)
    }

    fn party_bank_accounts(&self) -> Repository<'_, PartyBankAccount> {
        Repository::new(self, Self::PARTY_BANK_ACCOUNTS)
    }

    fn party(&self) -> PartyRepository<'_> {
        PartyRepository::new(self)
    }
}
