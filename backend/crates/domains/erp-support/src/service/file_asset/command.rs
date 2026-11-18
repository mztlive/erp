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
    /// 登记已保存对象的文件管理元数据；在写事务检查当前账号上传权限并将资产与登记审计一同写入。
    /// 输入非法、无登记资格、唯一键冲突或者资产、审计写入失败时拒绝。
    pub async fn register_file_asset(
        &self,
        req: RegisterFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(req, None, AssetRegistrationOrigin::Metadata, actor).await
    }
    /// 上传边界已检查实际大小、MIME 和内容后，按真实上传账号登记受控文件。只标记基础内容已检查，不伪造病毒扫描通过。
    /// 在写事务验权并写资产和审计；无资格、输入非法、唯一键冲突或写入失败时拒绝。
    pub async fn register_content_checked_file_asset(
        &self,
        req: RegisterFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(req, None, AssetRegistrationOrigin::ContentCheckedUpload, actor)
            .await
    }
    /// 已检查上传用于已授权单据时，用真实上传账号登记资产，并在同一写事务核对实际单据、保存附件用途及两条审计。
    /// 无上传资格、单据缺失、输入非法、唯一键冲突或写入失败时不完成登记。
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
    // 两个登记来源共用的写入阶段：以服务端来源构造资产、准备可选附件和审计；在同一事务验上传权限、核对目标单据并写入。
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
    // 登记命令指定业务单据时，预先按资产 ID、单据 ID 和附件用途构造关联及其审计；实际单据存在性留到写事务核对。
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
    // 按真实上传人和服务端固定来源生成资产；只有受控上传才将扫描状态转为基础内容已检查。
    // 元数据非法或状态转换失败时拒绝，不登记资产。
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
    /// 把现存文件按确切用途关联到业务单据。在同一写事务核验附件创建权限、实际单据和未删除资产，再持久化关系与审计。
    /// 无资格、目标缺失、重复关系或写入失败时不返回新附件视图。
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
    // 命令使用原写事务读取实际未删除资产；缺失映射 `NotFound`，传播读取错误，不以请求元数据代替已登记记录。
    pub(super) async fn require_asset(&self, id: &str, executor: &mut dyn Executor) -> Result<FileAsset> {
        self.db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("文件资产不存在".into()))
    }
}
