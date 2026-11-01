//! 采购命令回执拥有仓储；全部读写复用调用方 Executor。

use persistence_core::Repository;

use crate::entity::purchase_order::PurchaseCommandReceipt;

/// 采购强类型回执仓储；读取时不得用软删除隐藏命令去重事实。
pub type PurchaseCommandReceiptRepository<'a, T> = Repository<'a, PurchaseCommandReceipt<T>>;
