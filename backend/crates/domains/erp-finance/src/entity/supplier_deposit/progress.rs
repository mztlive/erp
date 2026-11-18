//! 资金阶段只从实际占用和资金流水推导，不从供应商订单完成状态推断已付。

use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::Serialize;

use super::{
    DepositExposure, SupplierDepositAccount, SupplierDepositReservation, SupplierDepositUsage, add, subtract,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositExecutionFundsStatus {
    NotReserved,
    Reserved,
    AwaitingDebitVerification,
    DebitPendingSettlement,
    PartiallySettled,
    Settled,
    PartiallyReturned,
    Returned,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DepositExecutionFundsSummary {
    pub execution_group_id: String,
    pub status: DepositExecutionFundsStatus,
    pub reconciliation_required: bool,
    pub approved_amount: Amount,
    pub reserved_amount: Amount,
    pub verified_debit_amount: Amount,
    pub settled_amount: Amount,
    pub returned_amount: Amount,
    /// 无原返款申请事实时保持缺失，不按取消或订单状态补造。
    pub pending_return_amount: Option<Amount>,
}

impl DepositExecutionFundsSummary {
    /// 按同一执行组的财务实际记录生成阶段摘要。
    /// # 参数
    /// * `execution_group_id` - 当前冻结执行组。
    /// * `reservation` / `usages` / `account` - 已获权限范围的原资金记录。
    /// # 返回
    /// 返回占用和实付分离的金额与阶段。
    /// # 错误
    /// 记录串组、跨账户、数量金额溢出或不守恒时拒绝。
    pub fn from_records(
        execution_group_id: &str,
        reservation: Option<&SupplierDepositReservation>,
        usages: &[SupplierDepositUsage],
        account: Option<&SupplierDepositAccount>,
    ) -> Result<Self> {
        let mut summary = Self {
            execution_group_id: execution_group_id.into(),
            status: DepositExecutionFundsStatus::NotReserved,
            reconciliation_required: false,
            approved_amount: Amount::zero(),
            reserved_amount: Amount::zero(),
            verified_debit_amount: Amount::zero(),
            settled_amount: Amount::zero(),
            returned_amount: Amount::zero(),
            pending_return_amount: None,
        };

        let Some(reservation) = reservation else {
            if !usages.is_empty() || account.is_some() {
                return Err(Error::from("资金摘要缺少原执行组占用"));
            }
            return Ok(summary);
        };
        if reservation.basis.execution_group_id != execution_group_id
            || account.is_none_or(|account| account.base.id != reservation.account_id)
        {
            return Err(Error::from("资金摘要账户或执行组归属不一致"));
        }

        summary.approved_amount = reservation.amount;
        summary.reserved_amount = reservation.remaining()?;
        summary.reconciliation_required = account.is_some_and(|account| account.reconciliation_required);

        for usage in usages {
            if usage.reservation_id != reservation.base.id || usage.account_id != reservation.account_id {
                return Err(Error::from("资金使用不属于当前执行组"));
            }
            summary.verified_debit_amount = add(summary.verified_debit_amount, usage.amount)?;
            summary.settled_amount = add(
                summary.settled_amount,
                subtract(usage.settled_amount, usage.reversed_settlement_amount)?,
            )?;
            summary.returned_amount = add(summary.returned_amount, usage.returned_amount)?;
        }

        summary.status = summary.stage(reservation);
        Ok(summary)
    }

    fn stage(&self, reservation: &SupplierDepositReservation) -> DepositExecutionFundsStatus {
        use DepositExecutionFundsStatus as Stage;
        if self.returned_amount > Amount::zero() {
            return if self.returned_amount == self.verified_debit_amount {
                Stage::Returned
            } else {
                Stage::PartiallyReturned
            };
        }
        if self.settled_amount == self.approved_amount {
            return Stage::Settled;
        }
        if self.settled_amount > Amount::zero() {
            return Stage::PartiallySettled;
        }
        if self.verified_debit_amount > Amount::zero() {
            return Stage::DebitPendingSettlement;
        }
        if reservation.released_amount == reservation.amount {
            return Stage::Released;
        }
        if reservation.exposure == DepositExposure::PossiblySent {
            return Stage::AwaitingDebitVerification;
        }
        Stage::Reserved
    }
}
