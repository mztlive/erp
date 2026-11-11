//! 集成异常与差异详情：组合本域事实、正式责任与权威证据。
pub(crate) mod display;
mod error_detail;
pub(crate) mod error_labels;
mod error_task;
pub use error_detail::IntegrationErrorDetailView;
mod reconciliation_difference;
mod repository;
use std::sync::Arc;

use application_core::AuditActor;
use erp_integration::IntegrationAccess;
use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

/// 只读详情服务；权威证据实现与写入口由组合根共享。
pub struct IntegrationCenterReadService {
    db: Database,
    evidence: Arc<dyn IntegrationEvidenceAuthority>,
    access: IntegrationAccess,
}
impl IntegrationCenterReadService {
    /// 绑定数据库、权威证据和授权服务。
    ///
    /// # 参数
    /// * `db` - 集成集合所在数据库
    /// * `evidence` - 权威证据端口
    /// * `access` - 组合层注入的集成授权服务
    ///
    /// # 返回
    /// 返回未执行 I/O 的详情读取服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// HTTP 详情使用组合根装配的授权服务。
    pub fn new(
        db: Database,
        evidence: Arc<dyn IntegrationEvidenceAuthority>,
        access: IntegrationAccess,
    ) -> Self {
        Self { db, evidence, access }
    }

    fn access(&self) -> IntegrationAccess {
        self.access.clone()
    }

    async fn require_visible(&self, actor: &AuditActor, resource: &str, id: &str) -> Result<()> {
        self.access().require_handler(actor, resource, "detail", id, &mut NoTransaction).await?;
        Ok(())
    }
}
