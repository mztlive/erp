//! 文件登记和附件关联在原写事务内检查固定管理资格。
use application_core::AuditActor;
use erp_core::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use erp_permissions::file_asset;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::{AssetRegistrationOrigin, FileAssetService};
use crate::dto::file_asset::{
    AttachToDocumentRequest, DocumentAttachmentView, FileAssetView, RegisterFileAssetRequest,
};
use crate::entity::file_asset::{AttachmentUsage, DocumentAttachment, FileAsset};
use crate::ports::PreparedSupportAudit;
use crate::repository::FileAssetExt;
use crate::{Error, Result};

impl FileAssetService {
    /// 登记文件管理元数据，资格与持久化同一事务。
    /// # 参数
    /// req为已保存对象元数据，actor为当前账号。
    /// # 返回
    /// 新文件视图。
    /// # 错误
    /// 无登记资格、输入非法、唯一冲突或审计失败时拒绝。
    pub async fn register_file_asset(
        &self,
        req: RegisterFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(req, None, AssetRegistrationOrigin::Metadata, actor).await
    }
    /// 登记管理元数据并原子建立指定单据附件关系。
    /// # 参数
    /// req、document_id、usage为已核验业务输入，actor为当前账号。
    /// # 返回
    /// 新文件视图。
    /// # 错误
    /// 无登记资格、单据缺失、唯一冲突或审计失败时拒绝。
    pub async fn register_file_asset_with_attachment(
        &self,
        req: RegisterFileAssetRequest,
        document_id: BusinessDocumentId,
        usage: AttachmentUsage,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(
            req,
            Some((document_id, usage)),
            AssetRegistrationOrigin::Metadata,
            actor,
        )
        .await
    }
    /// 登记已经过真实大小、MIME及内容检查的受控上传。
    /// # 参数
    /// req为服务器检查后的元数据，actor为真实上传账号。
    /// # 返回
    /// 基础内容已检查的文件；不伪造病毒扫描通过。
    /// # 错误
    /// 无上传资格、输入非法、唯一冲突或审计失败时拒绝。
    pub async fn register_content_checked_file_asset(
        &self,
        req: RegisterFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(req, None, AssetRegistrationOrigin::ContentCheckedUpload, actor)
            .await
    }
    /// 原子保存已检查上传、指定单据关联与审计。
    /// # 参数
    /// req为服务器内容检查结果，document_id、usage为已授权业务关系，actor为真实上传账号。
    /// # 返回
    /// 新文件视图。
    /// # 错误
    /// 无上传资格、单据缺失、唯一冲突或审计失败时拒绝。
    pub async fn register_content_checked_file_asset_with_attachment(
        &self,
        req: RegisterFileAssetRequest,
        document_id: BusinessDocumentId,
        usage: AttachmentUsage,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(
            req,
            Some((document_id, usage)),
            AssetRegistrationOrigin::ContentCheckedUpload,
            actor,
        )
        .await
    }
    /// 登记文件前完成固定资格、实际单据存在性及原事务内写入。
    async fn register_file_asset_command(
        &self,
        req: RegisterFileAssetRequest,
        target: Option<(BusinessDocumentId, AttachmentUsage)>,
        origin: AssetRegistrationOrigin,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        let asset = Self::registration_asset(req, actor.id(), origin)?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "file_asset.register",
            "file_asset",
            asset.base.id.clone(),
        )?;
        let linked = self.prepare_attachment(&asset.base.id, target, actor)?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_operation(&actor, &file_asset::UPLOAD, executor).await?;
                    if let Some((attachment, _)) = &linked {
                        this.documents.ensure_registered(attachment.document_id.as_ref(), executor).await?;
                    }
                    this.db.file_assets().create(&asset, executor).await?;
                    this.audit.persist(&audit, executor).await?;
                    if let Some((attachment, audit)) = &linked {
                        this.db.document_attachments().create(attachment, executor).await?;
                        this.audit.persist(audit, executor).await?;
                    }
                    Ok(asset.into())
                })
            })
            .await
    }
    /// 组装附件及审计；实际单据存在性在原写事务核对。
    fn prepare_attachment(
        &self,
        asset_id: &str,
        target: Option<(BusinessDocumentId, AttachmentUsage)>,
        actor: &AuditActor,
    ) -> Result<Option<(DocumentAttachment, PreparedSupportAudit)>> {
        let Some((document_id, usage)) = target else {
            return Ok(None);
        };
        let request =
            AttachToDocumentRequest { document_id, file_asset_id: FileAssetId::new(asset_id), usage };
        request.validate()?;
        let attachment =
            DocumentAttachment::new(DocumentAttachmentId::new(next_id()), request.into_data(actor.id()))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "document_attachment.create",
            "document_attachment",
            attachment.base.id.clone(),
        )?;
        Ok(Some((attachment, audit)))
    }
    /// 以真实上传人构造资产，保留受控检查来源语义。
    /// # 参数
    /// req为检查后的元数据，actor_id为真实上传人，origin为服务端固定登记来源。
    /// # 返回
    /// 保留原检查状态及上传人的新资产。
    /// # 错误
    /// 输入非法或内容状态转换失败时拒绝。
    pub(super) fn registration_asset(
        req: RegisterFileAssetRequest,
        actor_id: &str,
        origin: AssetRegistrationOrigin,
    ) -> Result<FileAsset> {
        req.validate()?;
        let mut asset = FileAsset::new(FileAssetId::new(next_id()), req.into_data(actor_id)?)?;
        if matches!(origin, AssetRegistrationOrigin::ContentCheckedUpload) {
            asset.mark_content_checked()?;
        }
        Ok(asset)
    }
    /// 在同一写事务检查附件管理资格、单据和资产存在性后关联。
    /// # 参数
    /// req为精确关联，actor为当前账号。
    /// # 返回
    /// 新附件关系视图。
    /// # 错误
    /// 无资格、单据或资产缺失、重复关系及审计失败时拒绝。
    pub async fn attach_to_document(
        &self,
        req: AttachToDocumentRequest,
        actor: &AuditActor,
    ) -> Result<DocumentAttachmentView> {
        req.validate()?;
        let attachment =
            DocumentAttachment::new(DocumentAttachmentId::new(next_id()), req.into_data(actor.id()))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "document_attachment.create",
            "document_attachment",
            attachment.base.id.clone(),
        )?;
        let actor = actor.clone();
        let this = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_operation(&actor, &file_asset::DOCUMENT_ATTACHMENT_CREATE, executor).await?;
                    this.documents.ensure_registered(attachment.document_id.as_ref(), executor).await?;
                    this.require_asset(attachment.file_asset_id.as_ref(), executor).await?;
                    this.db.document_attachments().create(&attachment, executor).await?;
                    this.audit.persist(&audit, executor).await?;
                    Ok(attachment.into())
                })
            })
            .await
    }
    /// 读取决定写入的实际资产，保持原事务。
    /// # 参数
    /// id为实际资产身份，executor为原写执行器。
    /// # 返回
    /// 当前未删除资产。
    /// # 错误
    /// 不存在或数据库读取失败时拒绝。
    pub(super) async fn require_asset(&self, id: &str, executor: &mut dyn Executor) -> Result<FileAsset> {
        self.db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("文件资产不存在".into()))
    }
}
