//! 供应商供给的跨域只读列表。查询、当前指针与成本脱敏 wire 保持原合同。
use super::repository::offering::{
    SupplierOfferingListQuery as OfferingListQuery, SupplierOfferingReadRepository,
};
use application_core::{normalized_text, page_or_default, page_size_or_default};
use erp_catalog::{Product, Sku, SkuRevision};
use erp_party::{Party, PartyRevision};
use erp_supplier::SupplierAccount;
use erp_supply::entity::supplier_offering::{SupplierOfferingAvailability, SupplierOfferingRevision};
use mongodb::Database;
use persistence_core::NoTransaction;
use services::Result;
use std::collections::HashMap;
use validator::Validate;
pub mod dto;
pub use dto::{PageView, SupplierOfferingListParams, SupplierOfferingView};
use dto::{SortDir, OFFERING_SORT_FIELDS};
/// 供给列表只读服务。
pub struct SupplierOfferingReadService {
    db: Database,
}
#[derive(Default)]
struct OfferingListContext {
    skus: HashMap<String, Sku>,
    sku_revisions: HashMap<String, SkuRevision>,
    products: HashMap<String, Product>,
    suppliers: HashMap<String, SupplierAccount>,
    parties: HashMap<String, Party>,
    party_revisions: HashMap<String, PartyRevision>,
}

impl SupplierOfferingReadService {
    /// 创建供应商供给服务。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 分页查询供应商供给。
    ///
    /// # 参数
    /// * `params` - 筛选与分页参数
    ///
    /// # 返回
    /// 返回包含公司 SKU、供应商、当前商业条款和实时可供状态的列表。
    ///
    /// # 错误
    /// 参数或数据库查询失败时返回错误。
    pub async fn list(&self, params: &SupplierOfferingListParams) -> Result<PageView<SupplierOfferingView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            dto::normalize_sort(&params.sort_by, &params.sort_dir, OFFERING_SORT_FIELDS)?;
        let keyword = normalized_text(params.q.as_deref());
        let product_no = normalized_text(params.product_no.as_deref());
        let sku_no = normalized_text(params.sku_no.as_deref());
        let query = OfferingListQuery {
            availability_status: params.availability_status,
            keyword,
            product_no,
            sku_no,
            sku_id: params.typed_sku_id(),
            supplier_id: params.typed_supplier_id(),
            status: params.status,
            source_type: params.source_type,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: sort_dir == SortDir::Asc,
        };
        let bundle = SupplierOfferingReadRepository::new(&self.db)
            .load_offering_list_page(&query, &mut NoTransaction)
            .await?;
        let context = OfferingListContext {
            skus: by_id(bundle.skus),
            sku_revisions: by_id(bundle.sku_revisions),
            products: by_id(bundle.products),
            suppliers: by_id(bundle.suppliers),
            parties: by_id(bundle.parties),
            party_revisions: by_id(bundle.party_revisions),
        };
        let items = bundle
            .page
            .items
            .into_iter()
            .map(|row| {
                let id = row.id.clone();
                build_view(
                    row,
                    bundle.revisions.get(&id).cloned(),
                    bundle.availabilities.get(&id).cloned(),
                    &context,
                )
            })
            .collect();
        Ok(PageView {
            items,
            total: bundle.page.total,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
        })
    }
}
fn build_view(
    row: erp_supply::repository::supplier_offering::SupplierOfferingRow,
    revision: Option<SupplierOfferingRevision>,
    availability: Option<SupplierOfferingAvailability>,
    context: &OfferingListContext,
) -> SupplierOfferingView {
    let sku = context.skus.get(row.sku_id.as_ref());
    let sku_revision = sku
        .and_then(|sku| sku.stable.current_revision_id.as_deref())
        .and_then(|id| context.sku_revisions.get(id));
    let product = sku.and_then(|sku| context.products.get(sku.product_id.as_ref()));
    let supplier = context.suppliers.get(row.supplier_id.as_ref());
    let party = supplier.and_then(|supplier| context.parties.get(supplier.party_id.as_ref()));
    let party_revision = party
        .and_then(|party| party.stable.current_revision_id.as_deref())
        .and_then(|id| context.party_revisions.get(id));
    SupplierOfferingView {
        id: row.id,
        sku_id: row.sku_id.to_string(),
        sku_no: sku.map(|value| value.sku_no.clone()),
        product_no: product.map(|value| value.product_no.clone()),
        sku_name: sku_revision.map(|value| value.name.clone()),
        specification: sku_revision.and_then(|value| value.specification.clone()),
        supplier_id: row.supplier_id.to_string(),
        supplier_no: supplier.map(|value| value.supplier_no.clone()),
        supplier_name: party_revision.map(|value| value.legal_name.clone()),
        supplier_product_code: row.supplier_product_code,
        supplier_sku_code: row.supplier_sku_code,
        source_type: row.source_type,
        source_connection_id: row.source_connection_id.map(|value| value.to_string()),
        status: row.status,
        current_revision_id: row.current_revision_id,
        current_revision_no: revision.as_ref().map(|value| value.revision.revision_no),
        dropship_supply_price_gross: revision
            .as_ref()
            .map(|value| value.dropship_supply_price_gross.to_string()),
        dropship_supply_price_net: revision
            .as_ref()
            .map(|value| value.dropship_supply_price_net.to_string()),
        bulk_supply_price_gross: revision
            .as_ref()
            .map(|value| value.bulk_supply_price_gross.to_string()),
        bulk_supply_price_net: revision
            .as_ref()
            .map(|value| value.bulk_supply_price_net.to_string()),
        input_tax_rate: revision.as_ref().map(|value| value.input_tax_rate.to_string()),
        bulk_minimum_order_quantity: revision
            .as_ref()
            .map(|value| value.bulk_minimum_order_quantity.to_string()),
        supply_region: revision
            .as_ref()
            .map(|value| value.supply_region.clone())
            .unwrap_or_default(),
        product_capabilities: revision
            .as_ref()
            .map(|value| value.product_capabilities.clone())
            .unwrap_or_default(),
        dropship_express: revision.as_ref().and_then(|value| value.dropship_express.clone()),
        freight_amount: revision
            .as_ref()
            .and_then(|value| value.freight_amount.map(|amount| amount.to_string())),
        service_fee_amount: revision
            .as_ref()
            .and_then(|value| value.service_fee_amount.map(|amount| amount.to_string())),
        valid_from: revision.as_ref().map(|value| value.valid_from.to_string()),
        valid_to: revision
            .as_ref()
            .and_then(|value| value.valid_to.map(|date| date.to_string())),
        availability_status: availability.as_ref().map(|value| value.availability_status),
        available_quantity: availability
            .as_ref()
            .and_then(|value| value.available_quantity.map(|quantity| quantity.to_string())),
        availability_source_updated_at: availability
            .as_ref()
            .map(|value| value.source_updated_at.unix_secs()),
        availability_version: availability.as_ref().map(|value| value.base.version),
        version: row.version,
        created_at: row.created_at,
    }
}

trait HasId {
    fn id(&self) -> &str;
}

impl HasId for Sku {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for SkuRevision {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for Product {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for SupplierAccount {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for Party {
    fn id(&self) -> &str {
        &self.base.id
    }
}
impl HasId for PartyRevision {
    fn id(&self) -> &str {
        &self.base.id
    }
}

fn by_id<T: HasId>(values: Vec<T>) -> HashMap<String, T> {
    values
        .into_iter()
        .map(|value| (value.id().to_string(), value))
        .collect()
}
