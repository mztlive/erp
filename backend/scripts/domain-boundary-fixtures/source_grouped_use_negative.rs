use erp_sales::{
    entity::SalesOrder,
    dto::{SubmitSalesOrderRequest as Submit, Line},
};
use erp_finance::{
    entity::Invoice,
    service::ReceivableService as FinanceReceivable,
};

pub fn touch(order: SalesOrder, invoice: Invoice) {}
