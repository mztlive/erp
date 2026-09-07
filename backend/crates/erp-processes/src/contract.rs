//! Named contract processes that own file-asset registration and contract writes.

use crate::Result;
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_contract::{UploadContractRequest, UploadContractView};
use erp_core::ids::FileAssetId;
use erp_support::{FileAsset, FileAssetExt, RegisterFileAssetRequest};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;
use validator::Validate;

use crate::adapters::contract_service;

/// Process module name.
pub fn process_name() -> &'static str {
    "contract"
}

/// Register a contract PDF and persist the contract identity plus first revision atomically.
///
/// Object storage is already written by the HTTP adapter. This process holds the
/// root MongoDB transaction: file-asset metadata, contract + revision, and both
/// success audits share one [`persistence_core::Executor`].
///
/// # Parameters
/// * `db` - database handle
/// * `req` - contract business fields
/// * `asset_req` - already stored object bytes with registration metadata
/// * `actor` - authenticated audit actor
///
/// # Errors
/// Customer missing/disabled, field validation, unique-index conflicts, or transaction failures.
pub async fn upload_contract(
    db: Database,
    req: UploadContractRequest,
    asset_req: RegisterFileAssetRequest,
    actor: AuditActor,
) -> Result<UploadContractView> {
    req.validate()?;
    asset_req.validate()?;
    let service = contract_service(db.clone());
    let file_name = asset_req.file_name.clone();
    let asset = FileAsset::new(FileAssetId::new(next_id()), asset_req.into_data(actor.id())?)?;
    let file_asset_id = FileAssetId::new(asset.base.id.clone());
    let planned = service.plan_upload(req, file_asset_id, actor.id()).await?;
    let asset_audit =
        actor
            .clone()
            .resource_log("file_asset.register", "file_asset", asset.base.id.clone())?;
    let contract_audit =
        actor
            .clone()
            .resource_log("contract.create", "contract", planned.contract.base.id.clone())?;

    let mut contract_for_tx = planned.contract.clone();
    let revision = planned.revision.clone();
    let asset_for_tx = asset.clone();
    let client = db.client().clone();
    let db_for_tx = db.clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                db_for_tx.file_assets().create(&asset_for_tx, session).await?;
                service
                    .apply_create_in_transaction(&mut contract_for_tx, &revision, session)
                    .await?;
                db_for_tx.audit_logs().create(&asset_audit, session).await?;
                db_for_tx.audit_logs().create(&contract_audit, session).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await?;

    Ok(UploadContractView {
        id: planned.contract.base.id,
        contract_no: planned.contract.contract_no,
        revision_id: planned.revision.base.id,
        revision_no: planned.revision.revision.revision_no,
        file_asset_id: asset.base.id,
        file_name,
        created_at: planned.contract.base.created_at,
    })
}
