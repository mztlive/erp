//! 系统管理中的演示主数据。
//!
//! 生成客户、供应商、商品、仓库和字典，并补齐岗位账号、审批流程和默认责任规则。
//! 删除时清空整个数据库，只保留 admin 账号及必要超管授权。

use std::sync::Arc;
mod accounts;
mod approvals;
mod ensure_customer;
mod ensure_dictionary;
mod ensure_product;
mod ensure_supplier;
mod ensure_warehouse;
mod foundation;
mod foundation_validation;
mod image_assets;
mod image_request;
mod images;
mod lifecycle;
mod offerings;
mod organization;
mod person_scopes;
mod plan;
mod prices;
mod removal;
mod repository;
mod responsibility;
mod roles;
use repository::record;
mod seed;
mod service;
mod spec;
mod voucher;

use erp_identity::{AuthorizationService, SharedRbacService};
pub use foundation::DemoFoundationReport;
pub use record::ensure_indexes;
pub use removal::DemoResetReport;
pub use service::{ApplyDemoMasterDataRequest, DemoChunkReport, DemoStatus};
use storage::S3Storage;

/// 演示主数据生成与删除。
pub struct DemoMasterDataProcess<A> {
    pub(super) db: mongodb::Database,
    ports: crate::runtime_ports::DemoRuntimePorts<A>,
    pub(super) identity: SharedRbacService,
    pub(super) authorization: Arc<AuthorizationService>,
    enabled: bool,
    storage: S3Storage,
    image_secret: Vec<u8>,
}
