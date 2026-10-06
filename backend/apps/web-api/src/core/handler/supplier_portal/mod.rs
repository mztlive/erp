//! 门户 HTTP 仅适配协议，供应商身份和跨域事务由独立入口提供。
pub mod admin;
mod asset_pdf;
pub mod assets;
pub mod auth;
pub mod read;
pub mod write;

use erp_processes::supplier_portal::SupplierPortalProcess;

use crate::app_state::AppState;

fn process(state: &AppState) -> SupplierPortalProcess {
    SupplierPortalProcess::new(state.db(), state.rbac())
}
