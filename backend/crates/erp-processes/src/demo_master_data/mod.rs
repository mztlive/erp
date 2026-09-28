//! 系统管理中的演示主数据。
//!
//! 生成客户、供应商、商品、仓库和字典，并补齐岗位账号与审批流程。
//! 删除时先清掉引用这批主数据的单据，再删主数据。账号和已发布审批流程保留。

mod accounts;
mod approvals;
mod derived;
mod derived_graph;
mod ensure_customer;
mod ensure_dictionary;
mod ensure_product;
mod ensure_supplier;
mod ensure_warehouse;
mod foundation;
mod lifecycle;
mod master_graph;
mod organization;
mod plan;
mod removal;
mod repository;
use repository::record;
mod seed;
mod service;
mod spec;

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
pub use foundation::DemoFoundationReport;
pub use record::ensure_indexes;
pub use service::{ApplyDemoMasterDataRequest, DemoChunkReport, DemoStatus};

/// 演示主数据生成与删除。
pub struct DemoMasterDataService {
    pub(super) db: mongodb::Database,
    pub(super) sensitive: Arc<SensitiveDataCodec>,
    pub(super) rbac: SharedRbacService,
    enabled: bool,
}
