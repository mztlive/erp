//! 客户对象中心读模型。

pub mod profile;
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
