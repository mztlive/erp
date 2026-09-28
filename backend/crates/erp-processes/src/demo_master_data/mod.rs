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
mod organization;
mod plan;
mod record;
mod service;
mod spec;

use std::sync::Arc;

use erp_core::common::time::BusinessDate;
use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
pub use foundation::DemoFoundationReport;
pub use record::ensure_indexes;
pub use service::{ApplyDemoMasterDataRequest, DemoChunkReport, DemoStatus};

use crate::{Error, Result};

/// 演示主数据生成与删除。
pub struct DemoMasterDataService {
    pub(super) db: mongodb::Database,
    pub(super) sensitive: Arc<SensitiveDataCodec>,
    pub(super) rbac: SharedRbacService,
    enabled: bool,
}

pub(super) fn demo_date() -> Result<BusinessDate> {
    BusinessDate::from_ymd(2026, 1, 1).ok_or_else(|| Error::Internal("演示日期无效".to_string()))
}

pub(super) fn demo_date_end() -> Result<BusinessDate> {
    BusinessDate::from_ymd(2031, 1, 1).ok_or_else(|| Error::Internal("演示日期无效".to_string()))
}
