//! 业务命令在共享执行器上持久化之前准备好的文件资产批次。

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::execution::persist_logs;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::FileAssetId;
use erp_support::repository::prelude::*;
use erp_support::{
    FileAsset, FileAssetExt, PendingAttachmentBatch, PendingFileAssetRequest, PendingFileContent,
    PendingFileReference, PendingFileReferenceSet, SensitivityClass,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use crate::Result;
use crate::errors::map_audit_to_support;

/// 业务事务开始前构造、等待与聚合一起持久化的文件。
#[derive(Debug)]
pub struct PendingFileAssets {
    assets: Vec<FileAsset>,
    audits: Vec<AuditLog>,
    references: PendingFileReferenceSet,
    manifest: Vec<PendingFileContent>,
}

impl PendingFileAssets {
    /// 将已写入对象存储的附件登记请求整理为待落库批次，校验元数据，分配正式资产 ID 并预备审计。业务命令随后负责解析引用并同事务写入。
    /// 临时引用无法识别、登记或审计构造失败，以及引用或资产 ID 重复时拒绝整个批次。
    pub fn prepare(requests: Vec<PendingFileAssetRequest>, actor: &AuditActor) -> Result<Self> {
        let mut manifest = Vec::with_capacity(requests.len());
        let mut assets = Vec::with_capacity(requests.len());
        let mut audits = Vec::with_capacity(requests.len());
        let mut references = Vec::with_capacity(requests.len());

        for request in requests {
            let reference = PendingFileReference::parse(&request.reference)
                .map_err(|_| erp_core::Error::from("无法识别本次上传的附件，请移除后重新上传"))?;
            manifest.push(PendingFileContent {
                reference: reference.as_str().to_string(),
                file_name: request.registration.file_name.clone(),
                content_type: request.registration.content_type.clone(),
                byte_size: request.registration.byte_size,
                content_hmac: request.registration.content_hmac.clone(),
            });

            request.registration.validate()?;
            let sensitivity = request.registration.sensitivity_class;
            let asset =
                FileAsset::new(FileAssetId::new(next_id()), request.registration.into_data(actor.id())?)?;
            let asset_id = FileAssetId::new(asset.base.id.clone());
            let audit =
                actor.clone().resource_log("file_asset.register", "file_asset", asset.base.id.clone())?;
            references.push((reference, asset_id, sensitivity));
            assets.push(asset);
            audits.push(audit);
        }

        manifest.sort_by(|a, b| a.reference.cmp(&b.reference));
        let references = PendingFileReferenceSet::new(references)
            .map_err(|_| erp_core::Error::from("本次上传的附件登记重复，请移除重复附件后重新提交"))?;
        Ok(Self { assets, references, audits, manifest })
    }

    /// 将已完成解码或格式核验的服务端文件整理为待提交批次，并登记基础内容已检查状态；该状态不表示恶意内容扫描通过。登记校验或状态转换失败时拒绝。
    pub fn prepare_content_checked(
        requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<Self> {
        let mut pending = Self::prepare(requests, actor)?;
        for asset in &mut pending.assets {
            asset.mark_content_checked()?;
        }
        Ok(pending)
    }

    /// 将批次移入可共享的附件端口，供业务命令解析引用并在自身事务中保存资产和审计；包装时不写数据库。
    pub fn shared(self) -> Arc<dyn PendingAttachmentBatch> {
        Arc::new(self)
    }

    /// 将批次移入商品领域的附件端口，供商品命令消费上传引用，并将资产及审计写入同一商品命令事务。
    pub fn into_catalog_batch(self) -> Arc<dyn erp_catalog::PendingAttachmentBatch> {
        Arc::new(self)
    }
}

#[async_trait]
impl PendingAttachmentBatch for PendingFileAssets {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        self.references
            .resolve_id(id, used)
            .map_err(|_| erp_core::Error::from("附件未随本次操作上传或被重复使用，请核对附件后重新提交"))
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        self.references.ensure_all_used(used).map_err(|_| {
            erp_core::Error::from("有上传的文件未关联到单据，请补齐附件关联或移除多余文件后重新提交")
        })
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        self.references.contains_id(id)
    }

    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        self.references.sensitivity(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> erp_support::Result<()> {
        db.file_assets().create_many_ordered(&self.assets, executor).await?;
        persist_logs(db, &self.audits, executor).await.map_err(map_audit_to_support)
    }

    fn content_manifest(&self) -> Vec<PendingFileContent> {
        self.manifest.clone()
    }

    fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::ids::FileAssetId;
    use erp_support::{
        PendingAttachmentBatch, PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass,
        SecurityScanStatus, SensitivityClass,
    };

    use super::PendingFileAssets;

    fn actor() -> AuditActor {
        AuditActor::new("acct-1".to_string(), "admin".to_string(), AccountKind::Admin)
    }

    fn request(reference: &str, hmac: &str) -> PendingFileAssetRequest {
        PendingFileAssetRequest {
            reference: reference.to_string(),
            registration: RegisterFileAssetRequest {
                storage_object_key: format!("obj/{reference}"),
                file_name: "a.png".to_string(),
                content_type: "image/png".to_string(),
                byte_size: 12,
                content_hmac: hmac.to_string(),
                sensitivity_class: SensitivityClass::General,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
            },
        }
    }

    #[test]
    fn prepare_empty_batch_is_fully_consumed() {
        let pending = PendingFileAssets::prepare(Vec::new(), &actor()).unwrap();
        pending.ensure_all_used(&HashSet::new()).unwrap();
        assert!(!pending.contains_id(&FileAssetId::new("missing")));
    }

    #[test]
    fn checked_preparation_marks_basic_content_without_claiming_security_scan_passed() {
        let pending = PendingFileAssets::prepare_content_checked(
            vec![request("pending-file:checked", &"a".repeat(64))],
            &actor(),
        )
        .unwrap();
        assert_eq!(pending.assets[0].security_scan_status, SecurityScanStatus::ContentChecked);
    }

    #[test]
    fn prepare_rejects_duplicate_temporary_references() {
        let hmac = "a".repeat(64);
        let err = PendingFileAssets::prepare(
            vec![request("pending-file:one", &hmac), request("pending-file:one", &hmac)],
            &actor(),
        )
        .unwrap_err();
        assert!(matches!(err, crate::Error::Logic(erp_core::Error::LogicError(message))
            if message == "本次上传的附件登记重复，请移除重复附件后重新提交"));
    }

    #[test]
    fn resolve_replaces_temporary_reference_once() {
        let hmac = "b".repeat(64);
        let pending = PendingFileAssets::prepare(vec![request("pending-file:one", &hmac)], &actor()).unwrap();
        let mut used = HashSet::new();
        let mut id = FileAssetId::new("pending-file:one");
        assert!(pending.resolve_id(&mut id, &mut used).unwrap());
        assert_ne!(id.as_ref(), "pending-file:one");
        pending.ensure_all_used(&used).unwrap();

        let mut replay = FileAssetId::new("pending-file:one");
        assert!(pending.resolve_id(&mut replay, &mut used).is_err());
    }

    #[test]
    fn catalog_batch_keeps_exact_reference_consumption() {
        let pending =
            PendingFileAssets::prepare(vec![request("pending-file:one", &"a".repeat(64))], &actor())
                .unwrap()
                .into_catalog_batch();
        let mut used = HashSet::new();
        assert!(!pending.is_empty());
        assert!(pending.ensure_all_used(&used).is_err());

        let mut id = FileAssetId::new("pending-file:one");
        assert!(pending.resolve_id(&mut id, &mut used).unwrap());
        assert!(pending.contains_id(&id));
        pending.ensure_all_used(&used).unwrap();

        let mut repeated = FileAssetId::new("pending-file:one");
        assert!(pending.resolve_id(&mut repeated, &mut used).is_err());
    }

    #[test]
    fn file_asset_debug_does_not_leak_object_key() {
        let hmac = "c".repeat(64);
        let pending = PendingFileAssets::prepare(vec![request("pending-file:one", &hmac)], &actor()).unwrap();
        let debug = format!("{:?}", pending.assets[0]);
        assert!(!debug.contains("obj/pending-file:one"));
        assert!(debug.contains("<redacted>"));
    }
}
