//! 集成异常与差异详情：组合本域事实、正式责任与权威证据。
mod error_task;
mod reconciliation_difference;
use std::sync::Arc;

use application_core::AuditActor;
use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use erp_integration::{IntegrationAccess, IntegrationDataScopePort};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

/// 只读详情服务；权威证据实现与写入口由组合根共享。
pub struct IntegrationCenterReadService {
    db: Database,
    evidence: Arc<dyn IntegrationEvidenceAuthority>,
    data_scope: Arc<dyn IntegrationDataScopePort>,
}
impl IntegrationCenterReadService {
    /// 绑定数据库、权威证据和范围 Port。
    ///
    /// # 参数
    /// * `db` - 集成集合所在数据库
    /// * `evidence` - 权威证据端口
    /// * `data_scope` - 组合层注入的集成范围 Port
    ///
    /// # 返回
    /// 返回未执行 I/O 的详情读取服务。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// HTTP 详情必须注入生产 adapter，不得保留失败关闭端口。
    pub fn new(
        db: Database,
        evidence: Arc<dyn IntegrationEvidenceAuthority>,
        data_scope: Arc<dyn IntegrationDataScopePort>,
    ) -> Self {
        Self { db, evidence, data_scope }
    }

    fn access(&self) -> IntegrationAccess {
        IntegrationAccess::new(self.data_scope.clone())
    }

    async fn require_visible(
        &self,
        actor: &AuditActor,
        resource: &str,
        owner_user_id: &str,
        owner_org_unit_id: &str,
    ) -> Result<()> {
        self.access()
            .require_handler(actor, resource, "detail", owner_user_id, owner_org_unit_id, &mut NoTransaction)
            .await?;
        Ok(())
    }
}
