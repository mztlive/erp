//! Sales command identity contracts.
pub mod identity;
#[cfg(test)]
use crate::dto::sales_order::SubmitSalesOrderRequest;
#[cfg(test)]
use identity::{
    sales_order_create_audit_id, sales_order_create_fingerprint, sales_submission_audit_id,
    sales_submission_fingerprint,
};
#[cfg(test)]
mod card_projection_input_tests;
