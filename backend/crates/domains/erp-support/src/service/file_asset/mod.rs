//! 文件资产管理入口统一检查功能资格；已授权业务投影复用原执行器。
mod command;
mod governance;

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::BusinessDocumentId;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, file_asset};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

pub use crate::dto::file_asset::{
    AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
    FileAssetListParams, FileAssetView, MarkScanResultRequest, PageView, PendingFileAssetRequest,
    RegisterFileAssetRequest,
};
use crate::error::{Error, Result};
use crate::ports::{BusinessDocumentPort, SupportAuditPort};
use crate::repository::FileAssetExt;
use crate::repository::prelude::*;

type FileAssetFilter = <Database as FileAssetExt>::FileAssetFilter;

/// 区分直接元数据登记与已完成真实内容检查的受控上传。
enum AssetRegistrationOrigin {
    Metadata,
    ContentCheckedUpload,
}

/// 文件管理用例和已授权业务使用的最小资产读取步骤。
#[derive(Clone)]
pub struct FileAssetService {
    db: Database,
    audit: Arc<dyn SupportAuditPort>,
    documents: Arc<dyn BusinessDocumentPort>,
    authorization: Arc<AuthorizationService>,
}
impl FileAssetService {
    /// 注入文件数据库、业务事实与共享授权服务。
    /// # 参数
    /// db、audit、documents为原文件能力，authorization为应用唯一授权服务。
    /// # 返回
    /// 不缓存请求身份和授权结果的服务。
    /// # 错误
    /// 无；构造不执行I/O。
    pub fn new(
        db: Database,
        audit: Arc<dyn SupportAuditPort>,
        documents: Arc<dyn BusinessDocumentPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, audit, documents, authorization }
    }
    /// 在同一读取执行器检查文件管理列表资格并分页。
    /// # 参数
    /// params为业务筛选，actor为当前账号。
    /// # 返回
    /// 原字段口径的文件分页，不包含存储键。
    /// # 错误
    /// 参数非法、无功能资格或数据库错误时拒绝。
    pub async fn file_asset_list(
        &self,
        params: &FileAssetListParams,
        actor: &AuditActor,
    ) -> Result<PageView<FileAssetListItemView>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.file_asset_list_snapshot(&params, &actor, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn file_asset_list_snapshot(
        &self,
        params: &FileAssetListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<FileAssetListItemView>> {
        params.validate()?;
        self.require_operation(actor, &file_asset::LIST, executor).await?;
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
        let page = self.db.file_assets().search_file_assets(&filter, executor).await?;
        Ok(PageView {
            items: page.items.into_iter().map(Into::into).collect(),
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
    /// 检查管理详情资格后读取文件元数据。
    /// # 参数
    /// id为资产身份，actor为当前账号。
    /// # 返回
    /// 供受控响应使用的原资产视图。
    /// # 错误
    /// 无资格、不存在或读取失败时拒绝。
    pub async fn file_asset_detail(&self, id: &str, actor: &AuditActor) -> Result<FileAssetView> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.file_asset_detail_snapshot(&id, &actor, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn file_asset_detail_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FileAssetView> {
        self.require_operation(actor, &file_asset::DETAIL, executor).await?;
        self.asset_view(id, executor).await
    }
    /// 检查管理预览资格，在原事务读取元数据并持久化敏感读取审计。
    /// # 参数
    /// id为资产身份，actor为当前账号。
    /// # 返回
    /// 元数据；对象存储I/O由调用方在事务外执行。
    /// # 错误
    /// 无资格、不存在、读取或审计失败时拒绝。
    pub async fn file_asset_preview(&self, id: &str, actor: &AuditActor) -> Result<FileAssetView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_operation(&actor, &file_asset::PREVIEW, executor).await?;
                    this.preview_asset(&id, &actor, executor).await
                })
            })
            .await
    }
    /// 从已授权业务的原执行器读取文件元数据，不追加文件管理菜单资格。
    /// # 参数
    /// id必须来自调用方已核验的精确业务文件关系，executor为原业务执行器。
    /// # 返回
    /// 服务器内部文件视图；调用方保持其原公开图片、Portal或审批授权合同。
    /// # 错误
    /// 文件不存在或数据库读取失败时拒绝。
    pub async fn asset_view(&self, id: &str, executor: &mut dyn Executor) -> Result<FileAssetView> {
        let asset = self
            .db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("文件资产不存在".into()))?;
        Ok(asset.into())
    }
    /// 为已通过精确业务授权的文件读取记录敏感审计。
    /// # 参数
    /// id与actor来自调用方业务授权，executor为其原执行器；不签发通用文件资格。
    /// # 返回
    /// 文件元数据；不追加文件管理菜单资格。
    /// # 错误
    /// 文件不存在、数据库或审计失败时拒绝。
    pub async fn preview_asset(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FileAssetView> {
        let view = self.asset_view(id, executor).await?;
        let audit =
            self.audit.resource_log(actor.clone(), "file_asset.preview", "file_asset", id.to_string())?;
        self.audit.persist(&audit, executor).await?;
        Ok(view)
    }
    /// 检查附件管理列表资格后读取精确单据附件关系。
    /// # 参数
    /// document_id为原单据身份，actor为当前账号。
    /// # 返回
    /// 原创建顺序的附件关系视图。
    /// # 错误
    /// 无功能资格或读取失败时拒绝。
    pub async fn document_attachment_list(
        &self,
        document_id: &BusinessDocumentId,
        actor: &AuditActor,
    ) -> Result<Vec<DocumentAttachmentView>> {
        let service = self.clone();
        let document_id = document_id.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.document_attachment_list_snapshot(&document_id, &actor, executor).await
                })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn document_attachment_list_snapshot(
        &self,
        document_id: &BusinessDocumentId,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentAttachmentView>> {
        self.require_operation(actor, &file_asset::DOCUMENT_ATTACHMENT_LIST, executor).await?;
        let rows = self.db.document_attachments().list_by_document(document_id, executor).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
    /// 管理入口在原执行器检查固定功能资格。
    async fn require_operation(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::FileAssetId;

    use super::{AssetRegistrationOrigin, FileAssetService, RegisterFileAssetRequest};
    use crate::entity::file_asset::{
        ContentHmac, FileAsset, FileAssetData, RetentionClass, SecurityScanStatus, SensitivityClass,
        content_fingerprint,
    };

    /// 不可用授权设施必须在实际读取步骤触发，资产数据库与单据 Port 均不得被读取。
    #[tokio::test]
    async fn management_reads_require_identity_before_loading_assets_or_attachments() {
        use std::sync::Arc;

        use application_core::AuditActor;
        use erp_core::AccountKind;
        use erp_core::ids::BusinessDocumentId;
        use erp_identity::testing::unavailable_authorization;
        use persistence_core::ReadExecutor;

        use crate::ports::{FailClosedAuditPort, FailClosedBusinessDocumentPort};
        use crate::{Error, FileAssetListParams};
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = FileAssetService::new(
            client.database("file_management_authorization_test"),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedBusinessDocumentPort),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let errors = [
            service
                .file_asset_list_snapshot(
                    &FileAssetListParams::default(),
                    &actor,
                    &mut ReadExecutor::default(),
                )
                .await
                .expect_err("必须拒绝"),
            service
                .file_asset_detail_snapshot("asset", &actor, &mut ReadExecutor::default())
                .await
                .expect_err("必须拒绝"),
            service
                .document_attachment_list_snapshot(
                    &BusinessDocumentId::new("document"),
                    &actor,
                    &mut ReadExecutor::default(),
                )
                .await
                .expect_err("必须拒绝"),
        ];
        for error in errors {
            assert!(matches!(error,Error::Internal(ref reason) if reason=="测试授权设施不可用"));
        }
    }

    fn registration() -> RegisterFileAssetRequest {
        RegisterFileAssetRequest {
            storage_object_key: "obj/upload.png".into(),
            file_name: "upload.png".into(),
            content_type: "image/png".into(),
            byte_size: 1,
            content_hmac: content_fingerprint("checked-content", b"secret"),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        }
    }

    #[test]
    fn checked_upload_records_usable_content_and_real_uploader_without_passing_scan() {
        let asset = FileAssetService::registration_asset(
            registration(),
            "procurement-1",
            AssetRegistrationOrigin::ContentCheckedUpload,
        )
        .unwrap();
        assert_eq!(asset.created_by, "procurement-1");
        assert_ne!(asset.created_by, "other-uploader");
        assert_eq!(asset.security_scan_status, SecurityScanStatus::ContentChecked);
        assert!(asset.is_usable_at(Instant::now()));
    }

    #[test]
    fn metadata_registration_remains_pending_and_invalid_input_cannot_gain_checked_status() {
        let asset = FileAssetService::registration_asset(
            registration(),
            "procurement-1",
            AssetRegistrationOrigin::Metadata,
        )
        .unwrap();
        assert_eq!(asset.security_scan_status, SecurityScanStatus::Pending);
        assert!(!asset.is_usable_at(Instant::now()));
        let mut invalid = registration();
        invalid.content_hmac = "not-a-valid-fingerprint".into();
        assert!(
            FileAssetService::registration_asset(
                invalid,
                "procurement-1",
                AssetRegistrationOrigin::ContentCheckedUpload,
            )
            .is_err()
        );
    }

    #[test]
    fn checked_upload_preserves_expiry_quarantine_rejection_and_destroyed_guards() {
        let mut expiring = registration();
        expiring.retention_class = RetentionClass::SevenDays;
        expiring.expires_at = Some(10);
        let expired = FileAssetService::registration_asset(
            expiring,
            "procurement-1",
            AssetRegistrationOrigin::ContentCheckedUpload,
        )
        .unwrap();
        assert!(expired.is_usable_at(Instant::from_unix_secs(9)));
        assert!(!expired.is_usable_at(Instant::from_unix_secs(10)));
        for status in [SecurityScanStatus::Quarantined, SecurityScanStatus::Rejected] {
            let mut asset = FileAssetService::registration_asset(
                registration(),
                "procurement-1",
                AssetRegistrationOrigin::ContentCheckedUpload,
            )
            .unwrap();
            asset.mark_scan_result(status).unwrap();
            assert!(!asset.is_usable_at(Instant::now()));
            assert!(asset.mark_content_checked().is_err());
        }
        let mut destroyed = FileAssetService::registration_asset(
            registration(),
            "procurement-1",
            AssetRegistrationOrigin::ContentCheckedUpload,
        )
        .unwrap();
        destroyed.destroy(Instant::now()).unwrap();
        assert!(!destroyed.is_usable_at(Instant::now()));
    }

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
