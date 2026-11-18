//! 域 D12 `contract` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ContractExt>::CONTRACTS` 等值。

use mongodb::Database;

use crate::repository::contract::{ContractDomainRepository, ContractFilter};
use crate::repository::owned::{ContractRepository, ContractRevisionRepository};

/// 域 D12 仓储访问器。
pub trait ContractExt {
    /// `contract` 集合名。
    const CONTRACTS: &'static str = "contracts";
    /// `contract_revision` 集合名。
    const CONTRACT_REVISIONS: &'static str = "contract_revisions";

    /// 合同列表筛选条件类型（定义见 `repository::contract`）。
    type ContractFilter;

    /// 取得 `contract` 集合的仓储，供稳定合同身份与当前修订指针的读写；由调用方提供执行器。
    fn contracts(&self) -> ContractRepository<'_>;

    /// 取得 `contract_revision` 集合的仓储，供不可变修订及 PDF 关联的读取和追加；由调用方提供执行器。
    fn contract_revisions(&self) -> ContractRevisionRepository<'_>;

    /// 取得合同身份与不可变修订的跨集合操作入口；原子写入沿用用例事务，不在仓储内开启事务。
    fn contract(&self) -> ContractDomainRepository<'_>;
}

impl ContractExt for Database {
    type ContractFilter = ContractFilter;

    fn contracts(&self) -> ContractRepository<'_> {
        ContractRepository::new(self, Self::CONTRACTS)
    }

    fn contract_revisions(&self) -> ContractRevisionRepository<'_> {
        ContractRevisionRepository::new(self, Self::CONTRACT_REVISIONS)
    }

    fn contract(&self) -> ContractDomainRepository<'_> {
        ContractDomainRepository::new(self)
    }
}
