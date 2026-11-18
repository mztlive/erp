//! 外部对象写入前的固定业务功能资格预检。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::{Permission, internal};
use persistence_core::{Executor, Transactional};

use super::SupplierProfileProcess;
use crate::Result;

impl SupplierProfileProcess {
    /// 在上传新建供应商资料附件前检查本次操作的完整功能资格。
    /// # 参数
    /// actor 为已认证账号。
    /// # 返回
    /// 当前快照具有功能资格时成功；不签发对象或附件访问凭证。
    /// # 错误
    /// 身份失效、缺少功能资格或快照失败时拒绝外部上传。
    pub async fn prepare_create_attachment_upload(&self, actor: &AuditActor) -> Result<()> {
        self.prepare_attachment_upload(actor, &internal::SUPPLIER_CREATE_ATTACHMENT_UPLOAD).await
    }

    /// 在上传修订供应商资料附件前检查本次操作的完整功能资格。
    /// # 参数
    /// actor 为已认证账号。
    /// # 返回
    /// 当前快照具有功能资格时成功；不签发对象或附件访问凭证。
    /// # 错误
    /// 身份失效、缺少功能资格或快照失败时拒绝外部上传。
    pub async fn prepare_update_attachment_upload(&self, actor: &AuditActor) -> Result<()> {
        self.prepare_attachment_upload(actor, &internal::SUPPLIER_UPDATE_ATTACHMENT_UPLOAD).await
    }

    /// 功能预检只覆盖本次外部写入，正式业务事务必须重新授权。
    async fn prepare_attachment_upload(&self, actor: &AuditActor, permission: &Permission) -> Result<()> {
        let service = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.attachment_upload_qualification(&actor, &permission, executor).await
                })
            })
            .await
    }

    /// 在原快照仅验证固定上传入口，不读取目标对象或登记文件。
    async fn attachment_upload_qualification(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .rbac
            .require(actor, permission, AccessTarget::Operation, executor)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::fixed_authorization;
    use erp_party::SensitiveDataCodec;
    use erp_permissions::{file_asset, supplier};
    use erp_supplier::SupplierAccess;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    fn service(permissions: &[Permission]) -> SupplierProfileProcess {
        let db =
            mongodb::Client::with_options(Default::default()).unwrap().database("supplier_upload_preflight");
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        SupplierProfileProcess::new(
            db.clone(),
            Arc::new(SensitiveDataCodec::from_secret(b"upload-test")),
            authorization.clone(),
            SupplierAccess::new(db, authorization),
        )
    }

    #[tokio::test]
    async fn attachments_require_the_exact_profile_qualification_without_file_upload_or_supplier_facts() {
        let actor = AuditActor::new("uploader".into(), "上传人".into(), AccountKind::Admin);
        for (entry, grant) in [
            (internal::SUPPLIER_CREATE_ATTACHMENT_UPLOAD, supplier::PROFILE_CREATE),
            (internal::SUPPLIER_UPDATE_ATTACHMENT_UPLOAD, supplier::PROFILE_UPDATE),
        ] {
            assert!(matches!(
                service(&[file_asset::UPLOAD])
                    .attachment_upload_qualification(&actor, &entry, &mut SnapshotExecutor::default())
                    .await,
                Err(Error::Forbidden(_))
            ));

            let service = service(&[grant]);
            service
                .attachment_upload_qualification(&actor, &entry, &mut SnapshotExecutor::default())
                .await
                .unwrap();

            assert!(matches!(
                service.attachment_upload_qualification(&actor, &entry, &mut ReadExecutor::default()).await,
                Err(Error::ValidationError(_))
            ));
        }
    }
}
