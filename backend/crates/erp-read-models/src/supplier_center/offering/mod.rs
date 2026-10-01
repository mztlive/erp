//! 供应商供给的跨域只读列表。查询、当前指针与成本脱敏 wire 保持原合同。
use std::collections::HashMap;
use std::sync::Arc;

use application_core::{normalized_text, page_or_default, page_size_or_default};
use erp_catalog::{Product, Sku, SkuRevision};
use erp_party::{Party, PartyRevision};
use erp_supplier::SupplierAccount;
use erp_supply::OfferingDataScopePort;
use erp_supply::entity::supplier_offering::{SupplierOfferingAvailability, SupplierOfferingRevision};
use erp_supply::repository::supplier_offering::SupplierOfferingRow;
use mongodb::Database;
use validator::Validate;

use super::repository::offering::{
    SupplierOfferingListBundle, SupplierOfferingListQuery as OfferingListQuery,
    SupplierOfferingReadRepository,
};
use crate::Result;
mod detail;
pub mod dto;
mod procurement;
mod scope;
use dto::{OFFERING_SORT_FIELDS, SortDir};
pub use dto::{PageView, SupplierOfferingListParams, SupplierOfferingListView, SupplierOfferingView};
pub use procurement::{
    MapOfferingProcurementOwners, MongoOfferingProcurementOwners, OfferingProcurementOwners,
};
/// 供给列表只读服务。
pub struct SupplierOfferingReadService {
    db: Database,
    data_scope: Arc<dyn OfferingDataScopePort>,
    procurement: Arc<dyn OfferingProcurementOwners>,
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
    /// * `data_scope` - 供给范围 Port
    /// * `procurement` - 采购负责人规则解析
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(
        db: Database,
        data_scope: Arc<dyn OfferingDataScopePort>,
        procurement: Arc<dyn OfferingProcurementOwners>,
    ) -> Self {
        Self { db, data_scope, procurement }
    }
}

pub(super) fn prepare_list_query(params: &SupplierOfferingListParams) -> Result<OfferingListQuery> {
    params.validate()?;
    let (sort_by, sort_dir) = dto::normalize_sort(&params.sort_by, &params.sort_dir, OFFERING_SORT_FIELDS)?;
    Ok(OfferingListQuery {
        availability_status: params.availability_status,
        keyword: normalized_text(params.q.as_deref()),
        product_no: normalized_text(params.product_no.as_deref()),
        sku_no: normalized_text(params.sku_no.as_deref()),
        sku_id: params.typed_sku_id(),
        supplier_id: params.typed_supplier_id(),
        status: params.status,
        source_type: params.source_type,
        page: page_or_default(params.page),
        page_size: page_size_or_default(params.page_size),
        sort_by: Some(sort_by.to_string()),
        sort_ascending: sort_dir == SortDir::Asc,
        scope: None,
        maintainer_user_ids: params.owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        business_org_unit_ids: params.org_unit_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        offering_ids: None,
    })
}

pub(super) async fn repository_page(
    db: &Database,
    query: &OfferingListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<SupplierOfferingListBundle> {
    SupplierOfferingReadRepository::new(db).load_offering_list_page(query, executor).await.map_err(Into::into)
}

pub(super) fn page_view(
    bundle: SupplierOfferingListBundle,
    query: &OfferingListQuery,
) -> Result<PageView<SupplierOfferingView>> {
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
    Ok(PageView { items, total: bundle.page.total, page: query.page, page_size: query.page_size })
}
/// 装配当前指针下的名称、商业条款和实时可供情况，缺失关联保留空值。
fn build_view(
    row: SupplierOfferingRow,
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
    let mut view = SupplierOfferingView::from_row(row);
    view.sku_no = sku.map(|value| value.sku_no.clone());
    view.product_id = product.map(|value| value.base.id.clone());
    view.product_no = product.map(|value| value.product_no.clone());
    view.sku_name = sku_revision.map(|value| value.name.clone());
    view.specification = sku_revision.and_then(|value| value.specification.clone());
    view.supplier_no = supplier.map(|value| value.supplier_no.clone());
    view.supplier_name = party_revision.map(|value| value.legal_name.clone());
    if let Some(revision) = revision {
        view.apply_revision(revision);
    }
    if let Some(availability) = availability {
        view.availability_status = Some(availability.availability_status);
        view.available_quantity = availability.available_quantity.map(|value| value.to_string());
        view.availability_source_updated_at = Some(availability.source_updated_at.unix_secs());
        view.availability_version = Some(availability.base.version);
    }
    view
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
    values.into_iter().map(|value| (value.id().to_string(), value)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_and_list_mapping_preserve_identity_when_related_records_are_missing() {
        let row: SupplierOfferingRow = serde_json::from_value(serde_json::json!({
            "id": "offering-a", "sku_id": "sku-a", "supplier_id": "supplier-a",
            "supplier_sku_code": "0000123", "source_type": "MANUAL", "status": "PAUSED",
            "current_revision_id": "revision-a", "version": 3, "created_at": 1,
            "maintainer_user_id": "owner-a", "business_org_unit_id": "org-a"
        }))
        .unwrap();
        let view = build_view(row, None, None, &OfferingListContext::default());
        assert_eq!(view.id, "offering-a");
        assert_eq!(view.supplier_sku_code, "0000123");
        assert_eq!(view.maintainer_user_id, "owner-a");
        assert_eq!(view.current_revision_id.as_deref(), Some("revision-a"));
        assert!(view.product_id.is_none());
        assert!(view.sku_name.is_none());
        assert!(view.supplier_name.is_none());
        assert!(view.current_revision_no.is_none());
        assert!(view.available_quantity.is_none());
        assert!(view.dropship_supply_price_gross.is_none());
    }

    #[test]
    fn prepare_list_query_maps_owner_ids_not_created_by() {
        let params: SupplierOfferingListParams = serde_json::from_value(serde_json::json!({
            "owner_user_ids": "user-1",
            "procurement_owner_user_ids": "buyer-1",
            "org_unit_ids": "org-1"
        }))
        .unwrap();
        let query = prepare_list_query(&params).unwrap();
        assert_eq!(query.maintainer_user_ids.as_deref(), Some(["user-1".to_string()].as_slice()));
        assert_eq!(query.business_org_unit_ids.as_deref(), Some(["org-1".to_string()].as_slice()));
        assert!(query.offering_ids.is_none());
        assert!(query.scope.is_none());
    }
}
