//! 域 D05 `file_asset` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 登记/关联/检查结果/销毁：业务行 + 审计日志 → `with_transaction` 内原子提交；
//! - 查询一律 `&mut NoTransaction`。
//!
//! 文件 I/O 不在事务闭包内执行（TRANSACTIONS.md：事务内不做外部 HTTP/文件 IO）：
//! 上传落盘由 HTTP handler 在调用 Service 前完成，Service 只编排元数据。
//!
//! 跨域：只经 `DatabaseExt` 调对方域 Repository（P3-service-api §2）。本域依赖
//! D02：附件关联前经 `db.business_documents()` 校验业务单据已注册；文件资产的
//! 安全检查、保留期与销毁状态只作治理记录，不阻断业务对象关联。

use database::{AccessControlExt, DocumentRegistryExt, FileAssetExt, NoTransaction, Transactional};
use entities::file_asset::{DocumentAttachment, FileAsset};
use entities::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
    FileAssetListParams, FileAssetView, MarkScanResultRequest, PageView, RegisterFileAssetRequest,
};

/// 文件资产列表筛选条件类型（经 `FileAssetExt` 关联类型跨 crate 可达）。
type FileAssetFilter = <mongodb::Database as FileAssetExt>::FileAssetFilter;

/// 文件资产服务。
///
/// 提供文件资产登记、单据附件关联、安全检查与销毁编排。
pub struct FileAssetService {
    db: Database,
}

impl FileAssetService {
    /// 创建文件资产服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
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
        let filter = FileAssetFilter {
            file_name: query.file_name,
            security_scan_status: query.security_scan_status,
            retention_class: query.retention_class,
            sensitivity_class: query.sensitivity_class,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .file_assets()
            .search_file_assets(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| FileAssetListItemView {
                id: row.id,
                file_name: row.file_name,
                content_type: row.content_type,
                byte_size: row.byte_size,
                security_scan_status: row.security_scan_status,
                sensitivity_class: row.sensitivity_class,
                retention_class: row.retention_class,
                expires_at: row.expires_at,
                created_by: row.created_by,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
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
        req.validate()?;
        let asset = FileAsset::new(FileAssetId::new(next_id()), req.into_data(actor.id())?)?;
        let audit = actor
            .clone()
            .resource_log("file_asset.register", "file_asset", asset.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let asset_for_tx = asset.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.file_assets().create(&asset_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
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
        let audit = actor.clone().resource_log(
            "document_attachment.create",
            "document_attachment",
            attachment.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let attachment_for_tx = attachment.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.document_attachments()
                        .create(&attachment_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
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
        let items = self
            .db
            .document_attachments()
            .list_by_document(document_id, &mut NoTransaction)
            .await?;
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
        asset.destroy(entities::common::time::Instant::now())?;
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
        if asset.base.version != expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
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
        let audit = actor
            .clone()
            .resource_log(action, "file_asset", asset.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.file_assets().update(&mut asset, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<FileAsset, crate::errors::Error>(asset)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验业务单据已注册（跨域 D02 仓储读取）。
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
        self.db
            .business_documents()
            .find_by_id(document_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
        Ok(())
    }
}
