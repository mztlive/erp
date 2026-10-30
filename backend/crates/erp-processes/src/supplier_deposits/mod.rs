//! 通用供应商预存款跨域资格及受权财务管理。

mod access;
mod replacement;
pub use replacement::prepare_execution_funds_replacement;
mod automatic;
pub use automatic::prepare_automatic_execution_funds;
mod investigation;
pub use investigation::{
    FundsInvestigationProcess, FundsInvestigationRun, FundsInvestigationStore,
    SupplierFundsInvestigationWorker,
};
mod read_access;
pub use read_access::SupplierDepositReadAccess;
mod funds;
pub use access::authorize_deposit_account;
pub use funds::{SupplierDepositFundsAdapter, ensure_execution_funds, mark_execution_funds_sent};

mod management;
pub use management::*;
