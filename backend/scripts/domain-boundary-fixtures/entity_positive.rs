use crate::ids::CustomerId;
use erp_core::money::Amount;

pub struct CustomerAccount {
    pub id: CustomerId,
    pub credit_limit: Amount,
}

impl CustomerAccount {
    pub fn has_credit(&self) -> bool {
        self.credit_limit.to_decimal() >= 0.into()
    }
}
