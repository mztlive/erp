//! 集成异常与差异详情：组合本域事实、正式责任与权威证据。
pub(crate) mod display;
mod error_detail;
pub(crate) mod error_labels;
mod error_task;
pub use error_detail::IntegrationErrorDetailView;
mod reconciliation_difference;
mod repository;
use std::sync::Arc;

use erp_integration::IntegrationAccess;
use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use mongodb::Database;

/// 只读详情服务；权威证据实现与写入口由组合根共享。
#[derive(Clone)]
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
}

#[cfg(test)]
mod testing {
    use erp_identity::testing::unbound_authorization;
    use erp_integration::dto::ControlledEvidenceRef;
    use erp_integration::ports::evidence::{
        EvidenceFuture, EvidenceSubject, OriginalResultFact, VerifiedEvidence,
    };
    use mongodb::Client;
    use persistence_core::Executor;

    use super::*;

    struct UnusedEvidence;
    impl IntegrationEvidenceAuthority for UnusedEvidence {
        fn query_original<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, OriginalResultFact> {
            panic!("授权失败不得读取证据")
        }
        fn replay_original<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, String> {
            panic!("详情不得重放动作")
        }
        fn verify_reattribution<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, String> {
            panic!("授权失败不得验证证据")
        }
        fn verify_evidence<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a ControlledEvidenceRef,
            _: &'a str,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, VerifiedEvidence> {
            panic!("授权失败不得验证证据")
        }
        fn discover_evidence<'a>(
            &'a self,
            _: &'a EvidenceSubject,
            _: &'a mut dyn Executor,
        ) -> EvidenceFuture<'a, Vec<ControlledEvidenceRef>> {
            panic!("授权失败不得发现证据")
        }
    }

    /// 创建不连接数据库的拒绝式服务；业务读取一旦执行便无法成功。
    pub(super) async fn service() -> IntegrationCenterReadService {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .expect("合法测试地址");
        let db = client.database("integration_detail_authorization");
        IntegrationCenterReadService::new(
            db.clone(),
            Arc::new(UnusedEvidence),
            IntegrationAccess::new(db, unbound_authorization()),
        )
    }
}
