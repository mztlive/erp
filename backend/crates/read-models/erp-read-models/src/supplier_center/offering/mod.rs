//! 供应商供给的跨域只读列表。查询、当前指针与成本脱敏 wire 保持原合同。
use std::collections::HashMap;
use std::sync::Arc;

use application_core::{normalized_text, page_or_default, page_size_or_default};
use entity_core::HasBaseModel;
use erp_catalog::{Product, Sku, SkuRevision};
use erp_party::{Party, PartyRevision};
use erp_supplier::SupplierAccount;
use erp_supply::OfferingDataScopePort;
use erp_supply::dto::delivery_template::OfferingDeliveryView;
use erp_supply::entity::supplier_offering::{
    SupplierOfferingAvailability, SupplierOfferingRevision, resolve_delivery,
};
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
pub mod portal;
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
    /// 返回服务实例。构造不执行查询。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        data_scope: Arc<dyn OfferingDataScopePort>,
        procurement: Arc<dyn OfferingProcurementOwners>,
    ) -> Self {
        Self { db, data_scope, procurement }
    }
}

/// 把列表参数规范成仓储查询。
///
/// # 参数
/// * `params` - 供给列表查询参数。
///
/// # 返回
/// 返回尚未套用数据范围的 `OfferingListQuery`。
///
/// # 错误
/// 参数校验失败、排序字段不在白名单或排序方向非法时返回 `ValidationError`。
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
        offering_ids: None,
    })
}

/// 按已准备的查询读取一页供给及展示事实。
///
/// # 参数
/// * `db` - 数据库。
/// * `query` - 已规范化的列表查询。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回当前页供给和跨域展示事实。
///
/// # 错误
/// 仓储或下层读取失败时返回对应错误。
pub(super) async fn repository_page(
    db: &Database,
    query: &OfferingListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<SupplierOfferingListBundle> {
    SupplierOfferingReadRepository::new(db).load_offering_list_page(query, executor).await.map_err(Into::into)
}

/// 把仓储页装配成供给视图页。
///
/// # 参数
/// * `bundle` - 当前页供给及关联事实。
/// * `query` - 提供页码和页大小的查询。
///
/// # 返回
/// 返回装配后的分页视图。缺失关联保留空值。
///
/// # 错误
/// 没有失败路径。
pub(super) fn page_view(
    bundle: SupplierOfferingListBundle,
    query: &OfferingListQuery,
) -> PageView<SupplierOfferingView> {
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
            let delivery = resolve_delivery(
                &row.supplier_id,
                row.delivery_template_id.as_ref(),
                &bundle.delivery_templates,
            );
            let mut view = build_view(
                row,
                bundle.revisions.get(&id).cloned(),
                bundle.availabilities.get(&id).cloned(),
                &context,
            );
            view.delivery_template = Some(OfferingDeliveryView::from_resolved(&delivery));
            view
        })
        .collect();
    PageView { items, total: bundle.page.total, page: query.page, page_size: query.page_size }
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
        view.availability_source_updated_at = availability.source_updated_at.map(|at| at.unix_secs());
        view.availability_version = Some(availability.base.version);
    }
    view
}

fn by_id<T: HasBaseModel>(values: Vec<T>) -> HashMap<String, T> {
    values.into_iter().map(|value| (value.id().to_string(), value)).collect()
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::revision::RevisionBase;
    use erp_core::ids::PartyId;
    use persistence_core::PageResult;

    use super::*;

    fn row(id: &str) -> SupplierOfferingRow {
        serde_json::from_value(serde_json::json!({
            "id": id, "sku_id": "sku-a", "supplier_id": "supplier-a",
            "supplier_sku_code": "0000123", "source_type": "MANUAL", "status": "PAUSED",
            "current_revision_id": "revision-a", "version": 3, "created_at": 1,
            "maintainer_user_id": "owner-a", "business_org_unit_id": "org-a"
        }))
        .unwrap()
    }

    #[test]
    fn detail_and_list_mapping_preserve_identity_when_related_records_are_missing() {
        let view = build_view(row("offering-a"), None, None, &OfferingListContext::default());
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
    fn page_mapping_preserves_row_order_totals_and_missing_display_facts() {
        for rows in [vec![row("offering-b"), row("offering-a")], vec![]] {
            let expected_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
            let bundle = SupplierOfferingListBundle {
                page: PageResult { items: rows, total: 37 },
                revisions: HashMap::new(),
                availabilities: HashMap::new(),
                skus: vec![],
                sku_revisions: vec![],
                products: vec![],
                suppliers: vec![],
                parties: vec![],
                party_revisions: vec![],
                delivery_templates: vec![],
            };
            let query = OfferingListQuery { page: 3, page_size: 15, ..Default::default() };
            let page = page_view(bundle, &query);
            assert_eq!((page.total, page.page, page.page_size), (37, 3, 15));
            assert_eq!(page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>(), expected_ids);
            assert!(page.items.iter().all(|row| {
                row.sku_name.is_none() && row.supplier_name.is_none() && row.delivery_template.is_some()
            }));
        }
    }

    #[test]
    fn entity_maps_keep_last_duplicate_and_accept_empty_input() {
        let first = PartyRevision {
            base: BaseModel { id: "revision-a".into(), ..BaseModel::fake() },
            revision: RevisionBase { revision_no: 1 },
            party_id: PartyId::new("party-a"),
            legal_name: "原名称".into(),
            short_name: None,
            change_reason: "初始建档".into(),
        };
        let last = PartyRevision { legal_name: "新名称".into(), ..first.clone() };
        let mapped = by_id(vec![first, last.clone()]);
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped.get("revision-a"), Some(&last));
        assert!(by_id(Vec::<PartyRevision>::new()).is_empty());
    }

    #[test]
    fn prepare_list_query_maps_owner_ids_not_created_by() {
        let params: SupplierOfferingListParams = serde_json::from_value(serde_json::json!({
            "owner_user_ids": "user-1",
            "procurement_owner_user_ids": "buyer-1",
        }))
        .unwrap();
        let query = prepare_list_query(&params).unwrap();
        assert_eq!(query.maintainer_user_ids.as_deref(), Some(["user-1".to_string()].as_slice()));
        assert!(query.offering_ids.is_none());
        assert!(query.scope.is_none());
    }
}
