//! 域 D12 `contract` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ContractExt>::CONTRACTS` 等值。

use entities::contract::{Contract, ContractRevision};
use mongodb::Database;

use super::super::contract::{ContractFilter, ContractRepository};
use crate::Repository;

/// 域 D12 仓储访问器。
pub trait ContractExt {
    /// `contract` 集合名。
    const CONTRACTS: &'static str = "contracts";
    /// `contract_revision` 集合名。
    const CONTRACT_REVISIONS: &'static str = "contract_revisions";

    /// 合同列表筛选条件类型（定义见 `repository::contract`）。
    type ContractFilter;

    /// 获取 `contract` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::contract::Contract>`。
    fn contracts(&self) -> Repository<'_, Contract>;

    /// 获取 `contract_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::contract::ContractRevision>`。
    fn contract_revisions(&self) -> Repository<'_, ContractRevision>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `ContractRepository` 实例。
    fn contract(&self) -> ContractRepository<'_>;
}

impl ContractExt for Database {
    type ContractFilter = ContractFilter;

    fn contracts(&self) -> Repository<'_, Contract> {
        Repository::new(self, Self::CONTRACTS)
    }

    fn contract_revisions(&self) -> Repository<'_, ContractRevision> {
        Repository::new(self, Self::CONTRACT_REVISIONS)
    }

    fn contract(&self) -> ContractRepository<'_> {
        ContractRepository::new(self)
    }
}
