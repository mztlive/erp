//! 域 D33 `supplier_settlement` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS` 等值。

use mongodb::Database;

use super::super::supplier_settlement::SupplierSettlementRepository;
use crate::repository::owned::{
    SupplierSettlementDifferenceEvidenceRepository, SupplierSettlementDifferenceRepository,
    SupplierSettlementItemRepository, SupplierSettlementSourceEvidenceRepository,
    SupplierSettlementStatementRepository,
};

/// 域 D33 仓储访问器。
pub trait SupplierSettlementExt {
    /// `supplier_settlement_statement` 集合名。
    const SUPPLIER_SETTLEMENT_STATEMENTS: &'static str = "supplier_settlement_statements";
    /// `supplier_settlement_item` 集合名。
    const SUPPLIER_SETTLEMENT_ITEMS: &'static str = "supplier_settlement_items";
    /// `supplier_settlement_difference` 集合名。
    const SUPPLIER_SETTLEMENT_DIFFERENCES: &'static str = "supplier_settlement_differences";
    /// `supplier_settlement_source_evidence` 集合名。
    const SUPPLIER_SETTLEMENT_SOURCE_EVIDENCE: &'static str = "supplier_settlement_source_evidence";
    /// `supplier_settlement_difference_evidence` 集合名。
    const SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE: &'static str = "supplier_settlement_difference_evidence";

    /// 用当前数据库打开 `supplier_settlement_statement` 集合仓储。
    /// 返回 `SupplierSettlementStatementRepository<'_>`。
    fn supplier_settlement_statements(&self) -> SupplierSettlementStatementRepository<'_>;

    /// 用当前数据库打开 `supplier_settlement_item` 集合仓储。
    /// 返回 `SupplierSettlementItemRepository<'_>`。
    fn supplier_settlement_items(&self) -> SupplierSettlementItemRepository<'_>;

    /// 用当前数据库打开 `supplier_settlement_difference` 集合仓储。
    /// 返回 `SupplierSettlementDifferenceRepository<'_>`。
    fn supplier_settlement_differences(&self) -> SupplierSettlementDifferenceRepository<'_>;

    /// 获取不可变结算来源证据批次 Repository。
    /// 返回当前数据库的来源证据仓储。
    fn supplier_settlement_source_evidence(&self) -> SupplierSettlementSourceEvidenceRepository<'_>;

    /// 获取不可变结算差异补证 Repository。
    /// 返回当前数据库的差异补证仓储。
    fn supplier_settlement_difference_evidence(&self) -> SupplierSettlementDifferenceEvidenceRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    /// 返回 `SupplierSettlementRepository` 实例。
    fn supplier_settlement(&self) -> SupplierSettlementRepository<'_>;
}

impl SupplierSettlementExt for Database {
    fn supplier_settlement_statements(&self) -> SupplierSettlementStatementRepository<'_> {
        SupplierSettlementStatementRepository::new(self, Self::SUPPLIER_SETTLEMENT_STATEMENTS)
    }

    fn supplier_settlement_items(&self) -> SupplierSettlementItemRepository<'_> {
        SupplierSettlementItemRepository::new(self, Self::SUPPLIER_SETTLEMENT_ITEMS)
    }

    fn supplier_settlement_differences(&self) -> SupplierSettlementDifferenceRepository<'_> {
        SupplierSettlementDifferenceRepository::new(self, Self::SUPPLIER_SETTLEMENT_DIFFERENCES)
    }

    fn supplier_settlement_source_evidence(&self) -> SupplierSettlementSourceEvidenceRepository<'_> {
        SupplierSettlementSourceEvidenceRepository::new(self, Self::SUPPLIER_SETTLEMENT_SOURCE_EVIDENCE)
    }

    fn supplier_settlement_difference_evidence(&self) -> SupplierSettlementDifferenceEvidenceRepository<'_> {
        SupplierSettlementDifferenceEvidenceRepository::new(
            self,
            Self::SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE,
        )
    }

    fn supplier_settlement(&self) -> SupplierSettlementRepository<'_> {
        SupplierSettlementRepository::new(self)
    }
}
