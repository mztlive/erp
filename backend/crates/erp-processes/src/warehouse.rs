//! Named warehouse processes that own audited outer transactions.

use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_catalog::CatalogExt;
use erp_core::ids::WarehouseSkuPolicyId;
use erp_warehouse::entity::warehouse::status::EnableStatus;
use erp_warehouse::entity::warehouse::warehouse_sku_policy::{WarehouseSkuPolicy, WarehouseSkuPolicyData};
use erp_warehouse::{CreateWarehouseSkuPolicyRequest, WarehouseExt, WarehouseSkuPolicyView};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;
use validator::Validate;

use crate::audit::run_audited;

/// Process module name.
pub fn process_name() -> &'static str {
    "warehouse"
}

/// Create a warehouse-SKU policy and persist the success audit in one transaction.
pub async fn create_warehouse_sku_policy(
    db: Database,
    req: CreateWarehouseSkuPolicyRequest,
    actor: AuditActor,
) -> Result<WarehouseSkuPolicyView> {
    req.validate()?;
    db.warehouse()
        .warehouse(req.warehouse_id.as_ref(), &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
    db.skus()
        .find_by_id(req.sku_id.as_ref(), &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("SKU不存在".to_string()))?;
    let id = WarehouseSkuPolicyId::new(next_id());
    let policy = WarehouseSkuPolicy::new(
        id.clone(),
        WarehouseSkuPolicyData {
            warehouse_id: req.warehouse_id,
            sku_id: req.sku_id,
            minimum_available_quantity: req.minimum_available_quantity,
            status: req.status.unwrap_or(EnableStatus::Active),
            effective_from: req.effective_from,
            effective_to: req.effective_to,
        },
    )?;
    let existing = db
        .warehouse()
        .sku_policies_for_dimensions(&policy.warehouse_id, &policy.sku_id, &mut NoTransaction)
        .await?;
    policy
        .ensure_no_overlap(&existing)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let audit = actor.clone().resource_log(
        "warehouse_sku_policy.create",
        "warehouse_sku_policy",
        id.to_string(),
    )?;
    let policy_for_tx = policy.clone();
    run_audited(&db, audit, move |db, session| {
        Box::pin(async move {
            db.warehouse_sku_policies()
                .create(&policy_for_tx, session)
                .await?;
            Ok(())
        })
    })
    .await?;
    Ok(policy.into())
}
