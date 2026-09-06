//! Customer-center read model.

mod repository;
mod service;

pub use repository::{
    CustomerCenterContractRow, CustomerCenterRelatedRow, CustomerCenterRepository,
    CustomerCenterSalesOrderRow,
};
pub use service::{
    CustomerCenterContractView, CustomerCenterReadService, CustomerCenterReceivableView,
    CustomerCenterRelatedView, CustomerCenterSalesOrderView,
};
