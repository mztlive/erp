//! 域 D05 `file_asset` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 登记/关联/检查结果/销毁：业务行 + 审计日志 → `with_transaction` 内原子提交；
//! - 查询一律 `&mut NoTransaction`。
//!
//! 文件 I/O 不在事务闭包内执行（TRANSACTIONS.md：事务内不做外部 HTTP/文件 IO）：
//! 上传落盘由 HTTP handler 在调用 Service 前完成，Service 只编排元数据。
//!
//! 跨域：业务单据注册经消费方 Port 读取，审计写入经审计 Port 落库；
//! 文件资产的安全检查、保留期与销毁状态只作治理记录，不阻断业务对象关联。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::check_expected_version;
pub use crate::dto::file_asset::{
    AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
    FileAssetListParams, FileAssetView, MarkScanResultRequest, PageView, PendingFileAssetRequest,
    RegisterFileAssetRequest,
};
use crate::entity::file_asset::{AttachmentUsage, DocumentAttachment, FileAsset};
use crate::error::{Error, Result};
use crate::ports::{BusinessDocumentPort, SupportAuditPort};
use crate::repository::FileAssetExt;
use crate::repository::prelude::*;

/// 文件资产列表筛选条件类型（经 `FileAssetExt` 关联类型跨 crate 可达）。
type FileAssetFilter = <mongodb::Database as FileAssetExt>::FileAssetFilter;

/// 文件资产服务。
///
/// 提供文件资产登记、单据附件关联、安全检查与销毁编排。
pub struct FileAssetService {
    db: Database,
    audit: Arc<dyn SupportAuditPort>,
    documents: Arc<dyn BusinessDocumentPort>,
}

impl FileAssetService {
    /// 创建文件资产服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `documents` - 业务单据注册端口
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(
        db: Database,
        audit: Arc<dyn SupportAuditPort>,
        documents: Arc<dyn BusinessDocumentPort>,
    ) -> Self {
        Self { db, audit, documents }
    }

    /// 分页查询文件资产列表。
    ///
    /// 列表不暴露敏感对象存储键（§6.1 对象存储地址不得写业务日志）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`file_name`/`security_scan_status`/`retention_class` 等）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn file_asset_list(
        &self,
        params: &FileAssetListParams,
    ) -> Result<PageView<FileAssetListItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let (page, page_size, sort_by, sort_ascending) = query.paging.into_filter_parts();
        let filter = FileAssetFilter {
            file_name: query.file_name,
            security_scan_status: query.security_scan_status,
            retention_class: query.retention_class,
            sensitivity_class: query.sensitivity_class,
            page,
            page_size,
            sort_by,
            sort_ascending,
        };
        let page = self.db.file_assets().search_file_assets(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(FileAssetListItemView::from).collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 查询文件资产详情。
    ///
    /// 详情返回对象存储键（供下载路由使用）；键是加密受控存储的不可猜测键。
    ///
    /// # 参数
    /// * `id` - 文件资产 ID
    ///
    /// # 返回
    /// 返回完整资产视图。
    ///
    /// # 错误
    /// * `NotFound` - 资产不存在
    pub async fn file_asset_detail(&self, id: &str) -> Result<FileAssetView> {
        let asset = self
            .db
            .file_assets()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("文件资产不存在".to_string()))?;
        Ok(asset.into())
    }

    /// 查询文件预览所需元数据并记录敏感读取审计。
    ///
    /// 实际对象存储读取仍由 HTTP handler 在事务外执行；本方法只负责元数据
    /// 存在性校验与读取行为审计。
    ///
    /// # Errors
    /// 文件不存在或审计日志写入失败时返回错误。
    pub async fn file_asset_preview(&self, id: &str, actor: &AuditActor) -> Result<FileAssetView> {
        let view = self.file_asset_detail(id).await?;
        let audit =
            self.audit.resource_log(actor.clone(), "file_asset.preview", "file_asset", id.to_string())?;
        self.audit.persist(&audit, &mut NoTransaction).await?;
        Ok(view)
    }

    /// 登记文件资产（元数据登记，文件已由上传 handler 落盘）。
    ///
    /// 登记是纯元数据写入（单集合 + 审计日志）；同对象键重复登记由唯一索引
    /// `uk_file_assets_storage_key` 透出冲突（409）。
    ///
    /// # 参数
    /// * `req` - 登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的资产详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败（含指纹形态非法）
    /// * `ConflictError` - 同一对象键重复登记（唯一索引透出）
    pub async fn register_file_asset(
        &self,
        req: RegisterFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(req, None, actor).await
    }

    /// 登记文件资产，并在同一事务内建立业务单据附件关联。
    ///
    /// 文件对象已由上传 handler 写入对象存储；本方法只负责将文件元数据、
    /// 附件关系及两条审计日志原子提交到 MongoDB。
    ///
    /// # 参数
    /// * `req` - 文件资产登记请求
    /// * `document_id` - 要关联的业务单据 ID
    /// * `usage` - 附件用途
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的资产详情视图。
    ///
    /// # 错误
    /// 请求非法、业务单据不存在、唯一键冲突或事务写入失败时返回错误。
    pub async fn register_file_asset_with_attachment(
        &self,
        req: RegisterFileAssetRequest,
        document_id: BusinessDocumentId,
        usage: AttachmentUsage,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        self.register_file_asset_command(req, Some((document_id, usage)), actor).await
    }

    /// 执行文件资产登记命令，并按需在同一事务内追加附件关联。
    async fn register_file_asset_command(
        &self,
        req: RegisterFileAssetRequest,
        attachment_target: Option<(BusinessDocumentId, AttachmentUsage)>,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        let asset = FileAsset::new(FileAssetId::new(next_id()), req.into_data(actor.id())?)?;
        let asset_audit = self.audit.resource_log(
            actor.clone(),
            "file_asset.register",
            "file_asset",
            asset.base.id.clone(),
        )?;
        let (attachment, attachment_audit) = match attachment_target {
            Some((document_id, usage)) => {
                self.ensure_business_document_registered(&document_id).await?;
                let request = AttachToDocumentRequest {
                    document_id,
                    file_asset_id: FileAssetId::new(asset.base.id.clone()),
                    usage,
                };
                request.validate()?;
                let attachment = DocumentAttachment::new(
                    DocumentAttachmentId::new(next_id()),
                    request.into_data(actor.id()),
                )?;
                let audit = self.audit.resource_log(
                    actor.clone(),
                    "document_attachment.create",
                    "document_attachment",
                    attachment.base.id.clone(),
                )?;
                (Some(attachment), Some(audit))
            },
            None => (None, None),
        };
        let db = self.db.clone();
        let client = db.client().clone();
        let asset_for_tx = asset.clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.file_assets().create(&asset_for_tx, session).await?;
                    audit_port.persist(&asset_audit, session).await?;
                    if let (Some(attachment), Some(audit)) = (attachment.as_ref(), attachment_audit.as_ref())
                    {
                        db.document_attachments().create(attachment, session).await?;
                        audit_port.persist(audit, session).await?;
                    }
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        Ok(asset.into())
    }

    /// 建立单据附件关联。
    ///
    /// 关联前校验业务单据已注册且文件资产存在；安全检查、保留期与销毁状态
    /// 不阻断关联。关联只追加不删除（§4.5.7 审计留痕）。
    ///
    /// # 参数
    /// * `req` - 关联请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的附件关联视图。
    ///
    /// # 错误
    /// * `NotFound` - 单据未注册或资产不存在
    pub async fn attach_to_document(
        &self,
        req: AttachToDocumentRequest,
        actor: &AuditActor,
    ) -> Result<DocumentAttachmentView> {
        req.validate()?;
        self.ensure_business_document_registered(&req.document_id).await?;
        self.db
            .file_assets()
            .find_by_id(&req.file_asset_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("文件资产不存在".to_string()))?;
        let attachment =
            DocumentAttachment::new(DocumentAttachmentId::new(next_id()), req.into_data(actor.id()))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "document_attachment.create",
            "document_attachment",
            attachment.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let attachment_for_tx = attachment.clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.document_attachments().create(&attachment_for_tx, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        Ok(attachment.into())
    }

    /// 按业务单据查询附件关联。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    ///
    /// # 返回
    /// 返回按创建时间升序排列的附件关联视图。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn document_attachment_list(
        &self,
        document_id: &BusinessDocumentId,
    ) -> Result<Vec<DocumentAttachmentView>> {
        let items = self.db.document_attachments().list_by_document(document_id, &mut NoTransaction).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// 记录安全检查结果。
    ///
    /// 迁移合法性由实体安全检查状态机校验（`PENDING → PASSED|REJECTED|QUARANTINED`，
    /// `QUARANTINED → PASSED|REJECTED`）。
    ///
    /// # 参数
    /// * `id` - 文件资产 ID
    /// * `req` - 更新请求（含期望版本与检查结果）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的资产详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 资产不存在
    /// * `ConflictError` - 版本陈旧或状态机不允许迁移
    pub async fn mark_scan_result(
        &self,
        id: &str,
        req: MarkScanResultRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        let mut asset = self.load_with_version(id, req.version).await?;
        asset.mark_scan_result(req.security_scan_status)?;
        self.update_with_audit(asset, "file_asset.scan", actor).await
    }

    /// 销毁文件资产。
    ///
    /// 销毁审计只记录一次（实体校验）；已销毁资产不得再用于业务关联。
    ///
    /// # 参数
    /// * `id` - 文件资产 ID
    /// * `req` - 销毁请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回销毁后的资产详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 资产不存在
    /// * `ConflictError` - 版本陈旧或资产已销毁
    pub async fn destroy_file_asset(
        &self,
        id: &str,
        req: DestroyFileAssetRequest,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        req.validate()?;
        let mut asset = self.load_with_version(id, req.version).await?;
        asset.destroy(erp_core::common::time::Instant::now())?;
        self.update_with_audit(asset, "file_asset.destroy", actor).await
    }

    /// 按 ID 加载资产并校验期望版本。
    ///
    /// # 参数
    /// * `id` - 文件资产 ID
    /// * `expected_version` - 请求携带的期望版本
    ///
    /// # 返回
    /// 返回加载的资产实体。
    ///
    /// # 错误
    /// 资产不存在返回 `NotFound`；版本不一致返回 `ConflictError`。
    async fn load_with_version(&self, id: &str, expected_version: u64) -> Result<FileAsset> {
        let asset = self
            .db
            .file_assets()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("文件资产不存在".to_string()))?;
        check_expected_version(asset.base.version, expected_version)?;
        Ok(asset)
    }

    /// 在单个事务中更新资产并追加审计日志。
    ///
    /// `Repository::update` 以 `id + version` CAS 兜底并发竞争（base.rs：
    /// `OptimisticLockingError` → 服务层 `ConflictError`）。
    ///
    /// # 参数
    /// * `mut asset` - 已由实体完成状态迁移的资产
    /// * `action` - 审计动作名
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的资产详情视图。
    ///
    /// # 错误
    /// 并发写入冲突或审计写入失败时返回错误。
    async fn update_with_audit(
        &self,
        mut asset: FileAsset,
        action: &str,
        actor: &AuditActor,
    ) -> Result<FileAssetView> {
        let audit = self.audit.resource_log(actor.clone(), action, "file_asset", asset.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.file_assets().update(&mut asset, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<FileAsset, crate::error::Error>(asset)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验业务单据已注册（跨域单据 Port 读取）。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 单据未注册时返回 `NotFound`。
    async fn ensure_business_document_registered(&self, document_id: &BusinessDocumentId) -> Result<()> {
        self.documents.ensure_registered(document_id.as_ref(), &mut NoTransaction).await
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::FileAssetId;

    use crate::entity::file_asset::{
        ContentHmac, FileAsset, FileAssetData, RetentionClass, SecurityScanStatus, SensitivityClass,
        content_fingerprint,
    };

    fn asset() -> FileAsset {
        FileAsset::new(
            FileAssetId::new("fa-1"),
            FileAssetData {
                storage_object_key: "obj/a".to_string(),
                file_name: "a.png".to_string(),
                content_type: "image/png".to_string(),
                byte_size: 1,
                content_hmac: ContentHmac::parse(content_fingerprint("content", b"k")).unwrap(),
                sensitivity_class: SensitivityClass::Sensitive,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
                created_by: "admin-1".to_string(),
            },
        )
        .unwrap()
    }

    #[test]
    fn confirm_scan_cleanup_and_duplicate_confirm_keep_original_errors() {
        let mut first = asset();
        first.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        assert_eq!(first.security_scan_status, SecurityScanStatus::Passed);
        first
            .mark_scan_result(SecurityScanStatus::Passed)
            .expect("same-status scan confirm stays idempotent");
        assert!(first.mark_scan_result(SecurityScanStatus::Quarantined).is_err());

        first.destroy(Instant::from_unix_secs(1)).unwrap();
        assert!(first.destroy(Instant::from_unix_secs(2)).is_err());
        assert_eq!(first.destroy(Instant::from_unix_secs(2)).unwrap_err().to_string(), "文件资产已销毁");
    }
}
