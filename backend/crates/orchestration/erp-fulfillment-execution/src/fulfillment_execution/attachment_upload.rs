//! 对象存储写入前校验确认功能资格；正式确认仍在原事务核验对象和当前任务。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::{Permission, internal};
use persistence_core::Transactional;

use super::FulfillmentProcess;
use crate::Result;

impl FulfillmentProcess {
    /// 在上传电子交付凭证前检查固定确认功能资格。
    ///
    /// actor为当前认证账号。当前同角色完整电子交付办理功能资格有效时成功，不签发对象办理凭证。账号失效、功能资格不足或授权设施失败时拒绝上传。
    pub async fn prepare_electronic_delivery_attachment_upload(&self, actor: &AuditActor) -> Result<()> {
        self.prepare_attachment_upload(internal::ELECTRONIC_DELIVERY_ATTACHMENT_UPLOAD, actor).await
    }

    /// 在上传现场履约凭证前检查固定确认功能资格。
    ///
    /// actor为当前认证账号。当前同角色完整服务履约办理功能资格有效时成功，不签发对象办理凭证。账号失效、功能资格不足或授权设施失败时拒绝上传。
    pub async fn prepare_service_fulfillment_attachment_upload(&self, actor: &AuditActor) -> Result<()> {
        self.prepare_attachment_upload(internal::SERVICE_FULFILLMENT_ATTACHMENT_UPLOAD, actor).await
    }

    async fn prepare_attachment_upload(&self, permission: Permission, actor: &AuditActor) -> Result<()> {
        let authorization = self.task_identity.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    Ok(authorization.require(&actor, &permission, AccessTarget::Operation, executor).await?)
                })
            })
            .await
    }
}
