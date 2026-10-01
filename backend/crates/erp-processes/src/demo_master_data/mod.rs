//! 系统管理中的演示主数据。
//!
//! 生成客户、供应商、商品、仓库和字典，并补齐岗位账号、审批流程和默认责任规则。
//! 删除时清空整个数据库，只保留 admin 账号及必要超管授权。

mod accounts;
mod approvals;
mod ensure_customer;
mod ensure_dictionary;
mod ensure_product;
mod ensure_supplier;
mod ensure_warehouse;
mod foundation;
mod foundation_validation;
mod lifecycle;
mod offerings;
mod organization;
mod person_scopes;
mod plan;
mod removal;
mod repository;
mod responsibility;
mod roles;
use repository::record;
mod seed;
mod service;
mod spec;
mod voucher;

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
pub use foundation::DemoFoundationReport;
pub use record::ensure_indexes;
pub use removal::DemoResetReport;
pub use service::{ApplyDemoMasterDataRequest, DemoChunkReport, DemoStatus};

/// 演示主数据生成与删除。
pub struct DemoMasterDataService {
    pub(super) db: mongodb::Database,
    pub(super) sensitive: Arc<SensitiveDataCodec>,
    pub(super) rbac: SharedRbacService,
    enabled: bool,
}
