//! 选品册跨域适配器。

use async_trait::async_trait;
use erp_catalog::entity::catalog::{read_specification_signature, SpecificationSignatureRead};
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_core::common::time::BusinessDate;
use erp_customer::CustomerAccountStatus;
use erp_sales::entity::sales_selection::{
    ImageAssetSnapshot, PoolFilterSnapshot, SpecificationAttributeSnapshot,
};
use erp_sales::ports::sales_selection::{
    SelectionCatalogPort, SelectionCustomerFact, SelectionCustomerPort, SelectionImagePort, SelectionSkuFact,
};
use erp_sales::Result as SalesResult;
use mongodb::Database;
use persistence_core::NoTransaction;
use std::sync::Arc;
use storage::S3Storage;

/// 客户适配器。
pub(super) struct CustomerAdapter {
    pub db: Database,
}

#[async_trait]
impl SelectionCustomerPort for CustomerAdapter {
    /// 读取客户编号与展示名称。
    ///
    /// # 参数
    /// * `customer_id` - 客户身份
    ///
    /// # 返回
    /// 返回客户事实。
    ///
    /// # 错误
    /// 不存在。
    async fn customer_fact(&self, customer_id: &str) -> SalesResult<SelectionCustomerFact> {
        let detail = crate::adapters::customer_service(self.db.clone())
            .customer_detail(customer_id)
            .await
            .map_err(|error| erp_sales::Error::NotFound(error.to_string()))?;
        Ok(SelectionCustomerFact {
            id: detail.account.id,
            customer_no: detail.account.customer_no.clone(),
            display_name: detail
                .legal_name
                .or(detail.account.legal_name)
                .unwrap_or_else(|| detail.account.customer_no.clone()),
            active: matches!(detail.account.status, CustomerAccountStatus::Active),
        })
    }
}

/// 商品池适配器。
pub(super) struct CatalogAdapter {
    pub db: Database,
}

#[async_trait]
impl SelectionCatalogPort for CatalogAdapter {
    /// 按筛选取出可售 SKU。
    ///
    /// # 参数
    /// * `filter` - 筛选
    /// * `as_of` - 资格日期
    ///
    /// # 返回
    /// 最多取 501 条以检测超限。
    ///
    /// # 错误
    /// 仓储失败。
    async fn collect_by_filter(
        &self,
        filter: &PoolFilterSnapshot,
        as_of: BusinessDate,
    ) -> SalesResult<Vec<SelectionSkuFact>> {
        let catalog_filter = erp_catalog::repository::SellableSkuFilter {
            nationwide_only: filter.nationwide_only,
            keyword: filter.q.clone(),
            product_kind: filter
                .product_kind
                .as_deref()
                .and_then(|value| serde_json::from_value(serde_json::Value::String(value.to_string())).ok()),
            category_id: filter.category_id.clone(),
            brand_id: filter.brand_id.clone(),
            supplier_id: filter.supplier_id.clone(),
            supply_region: filter.supply_region.clone(),
            max_supplier_count: filter.max_supplier_count,
            sales_price_min: filter.sales_price_min,
            sales_price_max: filter.sales_price_max,
            eligibility_as_of: as_of,
            page: 1,
            page_size: 501,
        };
        let page = crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery::new(self.db.clone())
            .search_sellable_skus(&catalog_filter, &mut NoTransaction)
            .await
            .map_err(map_store)?;
        Ok(page.items.into_iter().map(to_fact).collect())
    }

    /// 按身份取出当前可售修订。
    ///
    /// # 参数
    /// * `sku_ids` - 稳定身份
    /// * `as_of` - 资格日期
    ///
    /// # 返回
    /// 返回仍可售行。
    ///
    /// # 错误
    /// 仓储失败。
    async fn collect_by_ids(
        &self,
        sku_ids: &[String],
        as_of: BusinessDate,
    ) -> SalesResult<Vec<SelectionSkuFact>> {
        let rows = crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery::new(self.db.clone())
            .find_sellable_skus_by_ids(sku_ids, as_of, &mut NoTransaction)
            .await
            .map_err(map_store)?;
        Ok(rows.into_iter().map(to_fact).collect())
    }

    /// 复核精确修订。
    ///
    /// # 参数
    /// * `refs` - 引用
    /// * `as_of` - 资格日期
    ///
    /// # 返回
    /// 返回仍合格引用。
    ///
    /// # 错误
    /// 仓储失败。
    async fn qualified_refs(
        &self,
        refs: &[(String, String)],
        as_of: BusinessDate,
    ) -> SalesResult<Vec<(String, String)>> {
        let rows = crate::adapters::catalog_supply_query::MongoCatalogSupplyQuery::new(self.db.clone())
            .find_sellable_sku_refs(refs, as_of, &mut NoTransaction)
            .await
            .map_err(map_store)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect())
    }
}

/// 图片适配器。
pub(super) struct ImageAdapter {
    pub db: Database,
    pub storage: Arc<S3Storage>,
}

#[async_trait]
impl SelectionImagePort for ImageAdapter {
    /// 快照 SKU 主图。无法读取时按无图处理。
    ///
    /// # 参数
    /// * `source_asset_id` - 原资产
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    ///
    /// # 返回
    /// 返回快照引用。
    ///
    /// # 错误
    /// 存储失败以外的缺失返回 `None`。
    async fn snapshot_image(
        &self,
        source_asset_id: Option<&str>,
        booklet_id: &str,
        batch_id: &str,
    ) -> SalesResult<Option<ImageAssetSnapshot>> {
        let Some(source_asset_id) = source_asset_id.filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let service = erp_support::FileAssetService::new(
            self.db.clone(),
            crate::adapters::support_audit::MongoSupportAudit::shared(self.db.clone()),
            crate::adapters::support_documents::MongoBusinessDocument::shared(self.db.clone()),
        );
        let Ok(view) = service.file_asset_detail(source_asset_id).await else {
            return Ok(None);
        };
        let Ok(bytes) = self.storage.read(&view.storage_object_key).await else {
            return Ok(None);
        };
        let new_key = format!("sales-selection/{booklet_id}/{batch_id}/{source_asset_id}");
        if self
            .storage
            .save_with_content_type(&new_key, &bytes, Some(&view.content_type))
            .await
            .is_err()
        {
            return Err(erp_sales::Error::selection_prepare_failed(
                "图片快照保存失败，请重新准备",
            ));
        }
        Ok(Some(ImageAssetSnapshot {
            file_asset_id: view.id,
            content_checksum: view.content_hmac,
            storage_object_key: new_key,
        }))
    }

    /// 读取快照对象。
    ///
    /// # 参数
    /// * `storage_object_key` - 对象键
    ///
    /// # 返回
    /// 返回字节与内容类型。
    ///
    /// # 错误
    /// 对象不存在。
    async fn load_bytes(&self, storage_object_key: &str) -> SalesResult<(Vec<u8>, String)> {
        let bytes = self
            .storage
            .read(storage_object_key)
            .await
            .map_err(|error| erp_sales::Error::NotFound(format!("图片不存在: {error}")))?;
        Ok((bytes, "image/jpeg".into()))
    }
}

/// 映射商品池行为。
///
/// # 参数
/// * `row` - 可售行
///
/// # 返回
/// 返回选品事实。
///
/// # 错误
/// 无。
fn to_fact(row: erp_catalog::repository::SellableSkuRow) -> SelectionSkuFact {
    let specification_attributes = match read_specification_signature(&row.specification_signature) {
        SpecificationSignatureRead::Canonical(entries) => entries
            .into_iter()
            .map(|entry| SpecificationAttributeSnapshot {
                name: entry.attribute_code,
                value: entry.value_code,
            })
            .collect(),
        SpecificationSignatureRead::LegacyNonCanonical => Vec::new(),
    };
    SelectionSkuFact {
        sku_id: row.sku_id,
        sku_revision_id: row.sku_revision_id,
        product_id: row.product_id,
        product_kind: row.product_kind.as_str().to_string(),
        category_id: row.category_id,
        name: row.name,
        specification_attributes,
        unit: row
            .base_unit_name
            .or(row.base_unit_code)
            .unwrap_or_else(|| "件".into()),
        main_image_asset_id: row.main_image_asset_id,
        sales_visible_price_gross: row.sales_visible_price_gross,
    }
}

/// 映射存储错误。
///
/// # 参数
/// * `error` - 持久化错误
///
/// # 返回
/// 返回销售错误。
///
/// # 错误
/// 无。
fn map_store(error: persistence_core::Error) -> erp_sales::Error {
    erp_sales::Error::from(error)
}
