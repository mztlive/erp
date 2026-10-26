//! 供应商品库明确选品后加入既有公司商品与供给体系。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use mongodb::Database;

mod confirm;
mod preview;
mod product;
mod request;

pub use confirm::{AdoptionBatchResult, AdoptionRowResult};
pub use preview::{AdoptionPreviewView, AdoptionTargetView};

use crate::source_media::SourceMediaPreparer;

/// 选品跨域流程；创建商品、供给、来源关系与回执采用同一逐项事务。
#[derive(Clone)]
pub struct SupplierCatalogAdoptionProcess {
    db: Database,
    rbac: SharedRbacService,
    media_preparer: Option<Arc<SourceMediaPreparer>>,
}

impl SupplierCatalogAdoptionProcess {
    /// 绑定现有数据库和权威 RBAC，不隐式授予公司范围。
    /// # 参数
    /// `db` 为应用数据库，`rbac` 为已有共享权限实例。
    /// # 返回
    /// 尚未装配远程素材准备器的流程。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac, media_preparer: None }
    }

    /// 注入事务外执行的受控来源图片准备器。
    /// # 参数
    /// `preparer` 为组合根已有素材客户端。
    /// # 返回
    /// 已绑定素材能力的流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_media_preparer(mut self, preparer: Arc<SourceMediaPreparer>) -> Self {
        self.media_preparer = Some(preparer);
        self
    }
}
