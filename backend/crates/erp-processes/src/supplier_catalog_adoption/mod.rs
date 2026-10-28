//! 供应商品库明确选品后加入既有公司商品与供给体系。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supply::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use erp_supply::ports::supplier_reference_registry::{SupplierReferenceRegistry, SupplierReferenceTarget};
use mongodb::Database;

use crate::{Error, Result};

mod batch;
mod batch_draft;
mod batch_execution;
mod batch_matching;
mod batch_mutation;
mod batch_preview;
mod batch_request;
mod batch_submission;
mod confirm;
mod matching;
mod preview;
mod product;
mod request;

pub use batch_request::{
    BatchItemEdit, BatchSubmissionView, BatchVersionRequest, BatchView, CreateBatchRequest, SaveBatchRequest,
    SubmitBatchRequest,
};
pub use confirm::{AdoptionBatchResult, AdoptionRowResult};
pub use matching::AdoptionResolution;
pub use preview::AdoptionPreviewView;

use crate::source_media::SourceMediaPreparer;

/// 选品跨域流程；创建商品、供给、来源关系与回执采用同一逐项事务。
#[derive(Clone)]
pub struct SupplierCatalogAdoptionProcess {
    db: Database,
    rbac: SharedRbacService,
    media_preparer: Option<Arc<SourceMediaPreparer>>,
    registry: Option<Arc<dyn SupplierReferenceRegistry>>,
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
        Self { db, rbac, media_preparer: None, registry: None }
    }

    /// 注入供应商适配器注册表，选品不要求开启自动下单。
    /// # 参数
    /// `registry` 为组合根已配置注册表。
    /// # 返回
    /// 绑定后的流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// 校验选品来源配置与商品读取能力，不发送网络请求。
    fn ensure_source(&self, connection: &SupplierApiConnection) -> Result<()> {
        connection.ensure_catalog_available()?;
        let registry =
            self.registry.as_ref().ok_or_else(|| Error::BusinessLogicError("供应商接口尚未配置".into()))?;
        let target = SupplierReferenceTarget::from(connection);
        registry.deployment_config(&target).map_err(|error| Error::BusinessLogicError(error.summary))?;
        let capabilities =
            registry.capabilities(&target).map_err(|error| Error::BusinessLogicError(error.summary))?;
        if !capabilities.contains(&SupplierApiCapabilityCode::Product) {
            return Err(Error::BusinessLogicError("供应商接口不支持商品同步".into()));
        }
        Ok(())
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
