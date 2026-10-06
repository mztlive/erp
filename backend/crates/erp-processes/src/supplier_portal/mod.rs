//! 供应商门户命令：重验身份和资格，同一执行器提交申请、正式事实、任务与回执。

mod access;
mod accounts;
mod assets;
pub mod authorization;
mod batch;
mod command;
mod command_recovery;
mod commercial;
mod dispatch;
mod dto;
mod grants;
mod new_products;
mod offerings;
mod qualification;
mod tasks;

pub use assets::{PortalAssetKind, PortalAssetView, PortalCatalogAssetAccess, PreparedPortalAsset};
pub use batch::{
    PortalBatchInput, PortalBatchMode, PortalBatchPhase, PortalBatchResult, PortalBatchRow,
    PortalBatchRowResult,
};
pub use dto::*;
use erp_identity::SharedRbacService;
use mongodb::Database;
pub(crate) use qualification::request_reviewable;

/// 门户跨域用例的组合根；外部账号始终使用真实供应商身份。
#[derive(Clone)]
pub struct SupplierPortalProcess {
    db: Database,
    rbac: SharedRbacService,
}

impl SupplierPortalProcess {
    /// 装配门户命令服务。
    /// # 参数
    /// * `db` - 各拥有域共享的数据库。
    /// * `rbac` - 当前内部权限与范围来源。
    /// # 返回
    /// 返回未执行I/O的过程。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}
