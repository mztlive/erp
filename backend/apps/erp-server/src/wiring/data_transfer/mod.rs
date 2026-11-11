//! 应用组合根：为文件任务装配原业务查询，不保存 HTTP token。
mod reads;
use std::sync::Arc;

use erp_data_import::data_transfer::{BusinessTransferServices, BusinessTransfers, DataTransferProcess};
use erp_supplier_catalog::supplier_portal::SupplierPortalProcess;

use crate::wiring::{Application, catalog_services, scoped_offering_process};

pub(crate) fn process(state: &Application) -> DataTransferProcess {
    let reader = Arc::new(reads::Reads(state.clone()));
    let services = BusinessTransferServices {
        inquiry: state.inquiry_import_process(),
        offerings: Arc::new(scoped_offering_process(state.db(), state.rbac())),
        portal: Arc::new(SupplierPortalProcess::new(
            state.db(),
            state.rbac(),
            catalog_services(state.db(), state.rbac()),
        )),
    };
    let handler =
        Arc::new(BusinessTransfers::new(state.db(), state.rbac(), reader, state.storage().clone(), services));
    DataTransferProcess::new(
        state.db(),
        state.rbac(),
        state.storage().clone(),
        state.sensitive_data(),
        handler,
    )
}
