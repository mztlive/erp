//! 供应商预存款：真实到账、专属占用和实际扣款各自保存，均不伪造银行付款。

mod account;
mod authorization;
mod auto_settlement;
mod automatic_reservation;
mod batch;
mod funds_evidence;
mod funds_lease;
mod funds_scan;
mod ledger;
mod progress;
mod recharge_request;
mod replacement;
mod reservation;
mod resolution;
mod risk_watermark;
mod settlement;

pub use account::*;
pub use authorization::*;
pub use auto_settlement::*;
pub use automatic_reservation::*;
pub use batch::*;
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
pub use funds_evidence::*;
pub use funds_lease::*;
pub(crate) use funds_lease::{
    expiration as funds_lease_expiration, validate_owner as validate_funds_lease_owner,
};
pub use funds_scan::*;
pub use ledger::*;
pub use progress::*;
pub use recharge_request::*;
pub use reservation::*;
pub use resolution::*;
pub use settlement::*;
pub(crate) use settlement::{DepositSettlementScope, ensure_deposit_entry_balances, validate_deposit_actor};

pub(super) fn required(value: String, label: &str) -> Result<String> {
    normalize_required_text(value, &format!("{label}不能为空"), 256, &format!("{label}过长"))
}

pub(super) fn positive(value: Amount) -> Result<()> {
    if value <= Amount::zero() {
        return Err(Error::from("预存款金额必须大于零"));
    }
    Ok(())
}

pub(super) fn add(left: Amount, right: Amount) -> Result<Amount> {
    let value =
        left.to_decimal().checked_add(right.to_decimal()).ok_or_else(|| Error::from("预存款金额溢出"))?;
    Amount::try_from(value)
}

pub(super) fn subtract(left: Amount, right: Amount) -> Result<Amount> {
    let value =
        left.to_decimal().checked_sub(right.to_decimal()).ok_or_else(|| Error::from("预存款金额溢出"))?;
    Amount::try_from(value)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod funds_evidence_tests;
