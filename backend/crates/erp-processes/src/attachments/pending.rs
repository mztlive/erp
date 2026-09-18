//! Prepared file-asset batch that business commands persist on a shared executor.

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::repository::prelude::*;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog};
use erp_core::ids::FileAssetId;
use erp_support::repository::prelude::*;
use erp_support::{
    FileAsset, FileAssetExt, PendingAttachmentBatch, PendingFileAssetRequest, PendingFileReference,
    PendingFileReferenceSet, SensitivityClass,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use crate::Result;

/// Files constructed before a business transaction, waiting to persist with that aggregate.
#[derive(Debug)]
pub struct PendingFileAssets {
    assets: Vec<FileAsset>,
    audits: Vec<AuditLog>,
    references: PendingFileReferenceSet,
}

impl PendingFileAssets {
    /// Validate temporary references and registration metadata, then assign formal identities.
    ///
    /// # Parameters
    /// * `requests` - already stored object bytes with registration metadata
    /// * `actor` - authenticated audit actor
    ///
    /// # Errors
    /// Invalid temporary references, registration validation failures, or duplicate tokens.
    pub fn prepare(requests: Vec<PendingFileAssetRequest>, actor: &AuditActor) -> Result<Self> {
        let mut assets = Vec::with_capacity(requests.len());
        let mut audits = Vec::with_capacity(requests.len());
        let mut references = Vec::with_capacity(requests.len());
        for request in requests {
            let reference = PendingFileReference::parse(&request.reference)?;
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
        Ok(Self { assets, references: PendingFileReferenceSet::new(references)?, audits })
    }

    /// Wrap the batch as a `'static` consumer port.
    pub fn shared(self) -> Arc<dyn PendingAttachmentBatch> {
        Arc::new(self)
    }
}

#[async_trait]
impl PendingAttachmentBatch for PendingFileAssets {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        self.references.resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        self.references.ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        self.references.contains_id(id)
    }

    fn sensitivity(&self, id: &FileAssetId) -> Option<SensitivityClass> {
        self.references.sensitivity(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> erp_support::Result<()> {
        db.file_assets().create_many_ordered(&self.assets, executor).await?;
        db.audit_logs().create_many_ordered(&self.audits, executor).await?;
        Ok(())
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
        SensitivityClass,
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
    fn prepare_rejects_duplicate_temporary_references() {
        let hmac = "a".repeat(64);
        let err = PendingFileAssets::prepare(
            vec![request("pending-file:one", &hmac), request("pending-file:one", &hmac)],
            &actor(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("临时文件引用不能重复") || err.to_string().contains("重复"));
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
    fn file_asset_debug_does_not_leak_object_key() {
        let hmac = "c".repeat(64);
        let pending = PendingFileAssets::prepare(vec![request("pending-file:one", &hmac)], &actor()).unwrap();
        let debug = format!("{:?}", pending.assets[0]);
        assert!(!debug.contains("obj/pending-file:one"));
        assert!(debug.contains("<redacted>"));
    }
}
