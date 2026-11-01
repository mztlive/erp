//! 预存款拥有集合；通用 Repository 的更新使用版本 CAS。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::supplier_deposit::{
    SupplierDepositAccount, SupplierDepositAuthorization, SupplierDepositBatch, SupplierDepositLedgerEntry,
    SupplierDepositRechargeRequest, SupplierDepositReconciliation, SupplierDepositReservation,
    SupplierDepositSettlement, SupplierDepositUsage,
};

/// 预存款集合唯一来源，全部拥有者为财务领域。
pub trait SupplierDepositExt {
    const SUPPLIER_DEPOSIT_RECHARGE_REQUESTS: &'static str = "supplier_deposit_recharge_requests";
    /// 获取充值申请及审批记录仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回独立充值申请仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_recharge_requests(&self) -> Repository<'_, SupplierDepositRechargeRequest>;
    const SUPPLIER_DEPOSIT_ACCOUNTS: &'static str = "supplier_deposit_accounts";
    const SUPPLIER_DEPOSIT_BATCHES: &'static str = "supplier_deposit_batches";
    const SUPPLIER_DEPOSIT_AUTHORIZATIONS: &'static str = "supplier_deposit_authorizations";
    const SUPPLIER_DEPOSIT_RESERVATIONS: &'static str = "supplier_deposit_reservations";
    const SUPPLIER_DEPOSIT_LEDGER: &'static str = "supplier_deposit_ledger";
    const SUPPLIER_DEPOSIT_USAGES: &'static str = "supplier_deposit_usages";
    const SUPPLIER_DEPOSIT_SETTLEMENTS: &'static str = "supplier_deposit_settlements";
    const SUPPLIER_DEPOSIT_RECONCILIATIONS: &'static str = "supplier_deposit_reconciliations";

    /// 获取资金账户仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回财务拥有的账户仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_accounts(&self) -> Repository<'_, SupplierDepositAccount>;
    /// 获取到账批次仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回真实到账批次仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_batches(&self) -> Repository<'_, SupplierDepositBatch>;
    /// 获取财务自动执行授权仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回有限额授权仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_authorizations(&self) -> Repository<'_, SupplierDepositAuthorization>;
    /// 获取执行组专属占用仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回可恢复占用仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_reservations(&self) -> Repository<'_, SupplierDepositReservation>;
    /// 获取不可变资金流水仓储；业务只允许追加。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回唯一外部交易仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_ledger(&self) -> Repository<'_, SupplierDepositLedgerEntry>;
    /// 获取实际扣款使用仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回真实资金使用与核销关联仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_usages(&self) -> Repository<'_, SupplierDepositUsage>;
    /// 获取独立核销命令回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回可恢复核销回执。
    /// # 错误
    /// 无。
    fn supplier_deposit_settlements(&self) -> Repository<'_, SupplierDepositSettlement>;
    /// 获取对账证据仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回永久保留的对账差异仓储。
    /// # 错误
    /// 无。
    fn supplier_deposit_reconciliations(&self) -> Repository<'_, SupplierDepositReconciliation>;
}

impl SupplierDepositExt for Database {
    fn supplier_deposit_recharge_requests(&self) -> Repository<'_, SupplierDepositRechargeRequest> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_RECHARGE_REQUESTS)
    }
    fn supplier_deposit_accounts(&self) -> Repository<'_, SupplierDepositAccount> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_ACCOUNTS)
    }
    fn supplier_deposit_batches(&self) -> Repository<'_, SupplierDepositBatch> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_BATCHES)
    }
    fn supplier_deposit_authorizations(&self) -> Repository<'_, SupplierDepositAuthorization> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_AUTHORIZATIONS)
    }
    fn supplier_deposit_reservations(&self) -> Repository<'_, SupplierDepositReservation> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_RESERVATIONS)
    }
    fn supplier_deposit_ledger(&self) -> Repository<'_, SupplierDepositLedgerEntry> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_LEDGER)
    }
    fn supplier_deposit_usages(&self) -> Repository<'_, SupplierDepositUsage> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_USAGES)
    }
    fn supplier_deposit_settlements(&self) -> Repository<'_, SupplierDepositSettlement> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_SETTLEMENTS)
    }
    fn supplier_deposit_reconciliations(&self) -> Repository<'_, SupplierDepositReconciliation> {
        Repository::new(self, Self::SUPPLIER_DEPOSIT_RECONCILIATIONS)
    }
}
