//! 域 D07 `party` 仓储：party、party_revision、party_contact、party_address、
//! party_tax_profile、party_bank_account（数据模型 §6.2）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`super::Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本模块只补充
//! 域特有查询与跨集合多步骤写入入口。`party` 是稳定基础资料（可软删除，
//! 身份类字段全局唯一），`party_revision` 是不可变修订（追加式、**不提供**
//! 软删除），联系人/地址/税务资料/银行账户是支持有效期的从属事实行。
//!
//! 集合名常量统一从 `PartyExt` 关联常量导入（唯一权威来源）；筛选/行类型
//! 定义在职责子模块，经本模块重新导出并由 `PartyExt` 的关联类型对外暴露。

mod address;
mod aggregate;
mod bank_account;
mod contact;
// The collection module intentionally matches the parent domain name.
#[allow(clippy::module_inception)]
mod party;
mod revision;
mod shared;
mod tax_profile;

pub use address::PartyAddressFilter;
#[allow(unused_imports)]
pub use address::PartyAddressRow;
pub use bank_account::PartyBankAccountFilter;
#[allow(unused_imports)]
pub use bank_account::PartyBankAccountRow;
pub use contact::PartyContactFilter;
#[allow(unused_imports)]
pub use contact::PartyContactRow;
pub use party::PartyFilter;
#[allow(unused_imports)]
pub use party::PartyRow;
pub use revision::PartyRevisionFilter;
#[allow(unused_imports)]
pub use revision::PartyRevisionRow;
pub use tax_profile::PartyTaxProfileFilter;
#[allow(unused_imports)]
pub use tax_profile::PartyTaxProfileRow;

use mongodb::Database;

use super::extensions::PartyExt;

/// `party` 集合名（单一来源：`PartyExt` 关联常量）。
const PARTIES: &str = <mongodb::Database as PartyExt>::PARTIES;
/// `party_revision` 集合名（单一来源：`PartyExt` 关联常量）。
const PARTY_REVISIONS: &str = <mongodb::Database as PartyExt>::PARTY_REVISIONS;

/// D07 域专用仓储：主体当前资料读取与跨集合事务写入。
///
/// 单一集合 CRUD 使用 [`super::Repository`] 基类；主体当前修订、从属事实和
/// 跨集合原子写入由本类型收敛，通过 `PartyExt::party()` 访问。
pub struct PartyRepository<'a> {
    db: &'a Database,
}

impl<'a> PartyRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}
