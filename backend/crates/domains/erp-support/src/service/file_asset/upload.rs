//! 外部对象存储写入前的文件上传资格预检。
use application_core::AuditActor;
use erp_permissions::file_asset;
use persistence_core::{Executor, Transactional};

use super::FileAssetService;
use crate::Result;

impl FileAssetService {
    /// 在写入外部对象前确认当前账号具有文件上传资格。
    ///
    /// # 参数
    /// `actor` 为当前已认证账号。
    ///
    /// # 返回
    /// 当前快照具备上传功能资格时成功；登记资产的写事务仍须重新授权。
    ///
    /// # 错误
    /// 缺少上传资格、身份无效或授权读取失败时拒绝。
    pub async fn prepare_upload(&self, actor: &AuditActor) -> Result<()> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.upload_qualification(&actor, executor).await })
            })
            .await
    }

    /// 只证明本次外部上传资格，不产生登记或附件授权凭证。
    async fn upload_qualification(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.require_operation(actor, &file_asset::UPLOAD, executor).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_identity::Permission;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::fixed_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;
    use crate::ports::{FailClosedAuditPort, FailClosedBusinessDocumentPort};

    fn service(permissions: &[Permission]) -> FileAssetService {
        let client = mongodb::Client::with_options(Default::default()).unwrap();
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        FileAssetService::new(
            client.database("file_upload_preflight"),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedBusinessDocumentPort),
            authorization,
        )
    }

    fn actor() -> AuditActor {
        AuditActor::new("uploader".into(), "上传人".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn preflight_requires_upload_qualification_before_audit_documents_or_asset_access() {
        assert!(matches!(
            service(&[file_asset::DETAIL])
                .upload_qualification(&actor(), &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        service(&[file_asset::UPLOAD])
            .upload_qualification(&actor(), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert!(matches!(
            service(&[file_asset::UPLOAD]).upload_qualification(&actor(), &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }
}
