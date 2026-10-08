//! 客户对象中心读模型。

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
