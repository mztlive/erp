//! 门户 HTTP 仅适配协议，供应商身份和跨域事务由独立入口提供。
pub mod admin;
mod asset_pdf;
pub mod assets;
pub mod auth;
pub mod read;
pub mod write;

use erp_supplier_catalog::supplier_portal::SupplierPortalProcess;

use crate::http::state::HttpState;
use crate::wiring::catalog_services;

fn process(state: &HttpState) -> SupplierPortalProcess {
    SupplierPortalProcess::new(
        state.application().db(),
        state.application().authorization(),
        catalog_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
}
