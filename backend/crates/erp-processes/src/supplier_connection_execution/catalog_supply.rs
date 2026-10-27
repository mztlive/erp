//! 来源同步更新供应商采购报价与可供事实；公司销售价及人工关系状态均不覆盖。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierOfferingId;
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SupplierCatalogItem};
use erp_supply::entity::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, SupplierOffering, SupplierOfferingAvailabilityData,
};
use erp_supply::repository::SupplierOfferingExt;
use erp_supply::repository::prelude::*;
use erp_supply::service::supplier_offering::SupplierOfferingService;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub(crate) async fn apply_source_supply(
    db: &Database,
    item: &SupplierCatalogItem,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(id) = &item.offering_id else { return Ok(()) };
    let mut offering = db
        .supplier_offerings()
        .find_by_id(id, executor)
        .await?
        .filter(|offering| {
            offering.source_type == OfferingSourceType::Api
                && offering.supplier_id.as_ref() == item.supplier_id
                && offering.supplier_product_code.as_deref() == Some(item.product_id.as_str())
                && offering.source_connection_id.as_ref().map(|id| id.as_ref())
                    == Some(item.connection_id.as_str())
                && offering.supplier_sku_code == item.spec_id
                && item.sku_id.as_deref() == Some(offering.sku_id.as_ref())
        })
        .ok_or_else(|| Error::ConflictError("目录与正式API供给来源不一致".into()))?;
    SupplierOfferingService::new(db.clone()).sync_source_price(&mut offering, item, actor, executor).await?;
    apply_availability(db, item, &offering, actor, executor).await?;
    let audit = actor.clone().resource_log(
        "supplier_catalog.supply.apply",
        "supplier_offering",
        offering.base.id.clone(),
    )?;
    crate::audit::persist_log(db, &audit, executor).await?;
    Ok(())
}

async fn apply_availability(
    db: &Database,
    item: &SupplierCatalogItem,
    offering: &SupplierOffering,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut availability = db
        .supplier_offering_availabilities()
        .find_by_offering_id(&SupplierOfferingId::new(&offering.base.id), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("已选品供给缺少可供事实".into()))?;
    let status = if item.requires_identity_review {
        AvailabilityStatus::Stale
    } else {
        match item.availability {
            CatalogAvailability::Available
                if item.purchase_price.is_some()
                    && item.availability_valid_until.is_some_and(|until| until > Instant::now()) =>
            {
                AvailabilityStatus::Available
            },
            CatalogAvailability::Unavailable => AvailabilityStatus::Unavailable,
            CatalogAvailability::Withdrawn => AvailabilityStatus::Stopped,
            _ => AvailabilityStatus::Stale,
        }
    };
    let quantity = item
        .available_quantity
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| Error::ValidationError("来源可供数量无法解析".into()))?;
    availability.apply(SupplierOfferingAvailabilityData {
        supplier_offering_id: SupplierOfferingId::new(&offering.base.id),
        availability_status: status,
        available_quantity: quantity,
        source_updated_at: item.source_updated_at,
        received_at: item.observed_at,
        source_revision_token: Some(item.source_fingerprint.clone()),
        updated_by: actor.id().into(),
    })?;
    availability.valid_until = item.availability_valid_until;
    db.supplier_offering_availabilities().update(&mut availability, executor).await?;
    Ok(())
}
