//! 已明确选品的来源供给应用；公司销售价与人工关系状态不受同步覆盖。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierOfferingId, SupplierOfferingRevisionId};
use erp_core::money::UnitPrice;
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SupplierCatalogItem};
use erp_supply::entity::supplier_offering::{
    AvailabilityStatus, FromGrossPricesParams, OfferingSourceType, SupplierOffering,
    SupplierOfferingAvailabilityData, SupplierOfferingRevision, SupplierOfferingRevisionData,
};
use erp_supply::repository::SupplierOfferingExt;
use erp_supply::repository::prelude::*;
use id_generator::next_id;
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
    apply_availability(db, item, &offering, actor, executor).await?;
    if !item.requires_identity_review {
        revise_source_price(db, item, &mut offering, actor, executor).await?;
    }
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
                if item.availability_valid_until.is_some_and(|until| until > Instant::now()) =>
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

async fn revise_source_price(
    db: &Database,
    item: &SupplierCatalogItem,
    offering: &mut SupplierOffering,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(price) = &item.purchase_price else { return Ok(()) };
    let price: UnitPrice =
        price.parse().map_err(|_| Error::ValidationError("来源采购报价无法解析".into()))?;
    let id = offering
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::ConflictError("供给缺少当前条款".into()))?;
    let current = db
        .supplier_offering_revisions()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("供给当前条款不存在".into()))?;
    if current.dropship_supply_price_gross == price {
        return Ok(());
    }
    let revision_no = current
        .revision
        .revision_no
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("供给修订号超限".into()))?;
    let data = SupplierOfferingRevisionData::from_gross_prices(FromGrossPricesParams {
        supplier_offering_id: current.supplier_offering_id,
        revision_no,
        dropship_supply_price_gross: price,
        bulk_supply_price_gross: current.bulk_supply_price_gross,
        input_tax_rate: current.input_tax_rate,
        bulk_minimum_order_quantity: current.bulk_minimum_order_quantity,
        product_capabilities: current.product_capabilities,
        valid_from: current.valid_from,
        valid_to: current.valid_to,
        prefill_source_refs: current.prefill_source_refs,
    });
    let revision = SupplierOfferingRevision::new(SupplierOfferingRevisionId::new(next_id()), data)?;
    db.supplier_offering_revisions().create(&revision, executor).await?;
    offering.stable.current_revision_id = Some(revision.base.id);
    offering.stable.touch(actor.id());
    db.supplier_offerings().update(offering, executor).await?;
    Ok(())
}
