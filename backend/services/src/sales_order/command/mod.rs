//! 销售单命令用例：建单、保存草稿、提交、作废。

mod cancel;
mod create;
mod identity;
mod save;
mod sellable;
mod submit;
mod void;

#[cfg(test)]
pub(in crate::sales_order::command) use super::dto::SubmitSalesOrderRequest;
#[cfg(test)]
pub(in crate::sales_order::command) use identity::{
    sales_order_create_audit_id, sales_order_create_fingerprint, sales_submission_audit_id,
    sales_submission_fingerprint,
};

#[cfg(test)]
mod card_projection_input_tests;
#[cfg(test)]
mod goods_service_cutover_tests;
