//! 供应商目录观察接入公司商品资料；只在明确选品后建立资料来源。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_catalog::source_sync::repository::ProductSourceExt;
use erp_catalog::source_sync::{
    ProductSource, SourceField, SourceImage, SourceObservation, SourceSkuBinding, SourceValue,
};
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::adapters::catalog_service;
use crate::audit::persist_log;
use crate::{Error, Result};

/// 在采购明确选品事务中建立唯一资料来源及当前规格绑定。
/// # 参数
/// 已持久化公司商品/供给对应的目录项及同一事务。
/// # 返回
/// 同源SPU增加规格绑定；其他供货来源不改变已选资料来源。
/// # 错误
/// 来源关联冲突、公司资料缺失或写入失败时拒绝。
pub async fn initialize_source(
    db: &Database,
    item: &SupplierCatalogItem,
    product_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = catalog_service(db.clone());
    let current = service.source_company_content(product_id, executor).await?;
    let observation = observation(item)?;
    if let Some(mut source) = db.product_sources().find_by_id(product_id, executor).await? {
        if source.connection_id != item.connection_id || source.source_product_id != item.product_id {
            return Ok(());
        }
        bind_sku(&mut source, item)?;
        let changes = source.observe(&current.values, observation)?;
        service.write_source_content(product_id, &changes, actor.id(), executor).await?;
        db.product_sources().update(&mut source, executor).await?;
    } else {
        let mut source = ProductSource::new(
            product_id,
            &item.supplier_id,
            &item.connection_id,
            &item.base.id,
            &current.values,
            observation,
        )?;
        source.source_product_id = item.product_id.clone();
        source.supplier_name = source_supplier_name(db, &item.supplier_id, executor).await?;
        bind_sku(&mut source, item)?;
        db.product_sources().create(&source, executor).await?;
    }
    let audit = actor.clone().resource_log("product.source_sync.initialize", "product", product_id.into())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 将持续目录同步应用于已经明确选品的同源公司商品。
/// # 参数
/// 最新来源观察及目录持久化所用的事务执行器。
/// # 返回
/// 无选品、非展示资料来源时保持公司资料；其他情况原子更新资料和差异。
/// # 错误
/// 公司修订、素材或乐观锁失败时拒绝，不丢弃人工内容。
pub async fn apply_source_observation(
    db: &Database,
    item: &SupplierCatalogItem,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(product_id) = item.company_product_id.as_deref() else { return Ok(()) };
    let Some(mut source) = db.product_sources().find_by_id(product_id, executor).await? else {
        return Ok(());
    };
    if source.connection_id != item.connection_id || source.source_product_id != item.product_id {
        return Ok(());
    }
    if !source
        .sku_sources
        .iter()
        .any(|s| s.catalog_item_id == item.base.id && Some(&s.sku_id) == item.sku_id.as_ref())
    {
        return Ok(());
    }
    let service = catalog_service(db.clone());
    let current = service.source_company_content(product_id, executor).await?;
    let previous = source.fields.clone();
    let changes = source.observe(&current.values, observation(item)?)?;
    if changes.is_empty() && previous == source.fields {
        return Ok(());
    }
    service.write_source_content(product_id, &changes, actor.id(), executor).await?;
    db.product_sources().update(&mut source, executor).await?;
    let audit = actor.clone().resource_log("product.source_sync.apply", "product", product_id.into())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

fn bind_sku(source: &mut ProductSource, item: &SupplierCatalogItem) -> Result<()> {
    let sku =
        item.sku_id.as_ref().ok_or_else(|| Error::ValidationError("资料来源必须绑定公司规格".into()))?;
    if let Some(binding) = source.sku_sources.iter().find(|s| s.catalog_item_id == item.base.id) {
        if binding.sku_id != *sku {
            return Err(Error::ConflictError("资料来源规格已关联其他公司SKU".into()));
        }
    } else {
        source
            .sku_sources
            .push(SourceSkuBinding { catalog_item_id: item.base.id.clone(), sku_id: sku.clone() });
    }
    Ok(())
}

fn observation(item: &SupplierCatalogItem) -> Result<SourceObservation> {
    let mut values = vec![(SourceField::Name, None, SourceValue::Text(Some(item.name.clone())))];
    if let Some(sku) = &item.sku_id {
        values.push((SourceField::SkuName, Some(sku.clone()), SourceValue::Text(Some(item.name.clone()))));
    }
    if let Some(description) = &item.description {
        values.push((SourceField::Description, None, SourceValue::Text(Some(description.clone()))));
    }
    if let Some(images) = &item.images {
        let images = images
            .iter()
            .map(|i| SourceImage {
                url: Some(i.url.clone()),
                file_asset_id: i.file_asset_id.clone(),
                content_fingerprint: i.content_fingerprint.clone(),
            })
            .collect::<Vec<_>>();
        values.push((SourceField::CarouselMedia, None, SourceValue::Images(images.clone())));
        if let Some(sku) = &item.sku_id {
            values.push((
                SourceField::MainImage,
                Some(sku.clone()),
                SourceValue::Images(images.into_iter().take(1).collect()),
            ));
        }
    }
    if let Some(images) = &item.detail_images {
        values.push((
            SourceField::DetailMedia,
            None,
            SourceValue::Images(
                images
                    .iter()
                    .map(|i| SourceImage {
                        url: Some(i.url.clone()),
                        file_asset_id: i.file_asset_id.clone(),
                        content_fingerprint: i.content_fingerprint.clone(),
                    })
                    .collect(),
            ),
        ));
    }
    Ok(SourceObservation {
        values,
        observed_at: u64::try_from(item.observed_at.unix_secs())
            .map_err(|_| Error::ValidationError("商品来源观察时间无效".into()))?,
    })
}

async fn source_supplier_name(
    db: &Database,
    supplier: &str,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier, executor).await? else {
        return Ok(None);
    };
    let (parties, revisions) = db.party().list_with_current_revisions(&[supplier.party_id], executor).await?;
    Ok(parties
        .first()
        .and_then(|party| party.stable.current_revision_id.as_ref())
        .and_then(|id| revisions.iter().find(|r| &r.base.id == id))
        .map(|r| r.legal_name.clone()))
}
