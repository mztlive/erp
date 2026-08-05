//! 域 D33 `supplier_settlement`：supplier_settlement_statement、supplier_settlement_item、supplier_settlement_difference（页面：W27）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D33 仓储访问器（P2 填充）。
pub trait SupplierSettlementExt: Sized {}

impl SupplierSettlementExt for mongodb::Database {}
