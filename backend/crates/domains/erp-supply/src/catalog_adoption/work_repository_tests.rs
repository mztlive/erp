use entity_core::BaseModel;
use serde_json::json;

use super::*;
use crate::catalog_adoption::BatchSalesValues;
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
use crate::ports::connector::offer::{Offer, OfferDetails};

fn row(id: &str) -> CatalogAdoptionBatchItem {
    let source = SupplierCatalogItem::observed(
        "connection",
        "supplier",
        "job",
        Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(1),
            },
            value: Offer {
                sku: SupplierSku { product_id: Some("product".into()), spec_id: id.into() },
                name: "$literal source".into(),
                specification: "5kg".into(),
                unit: Some("袋".into()),
                quote: None,
                details: OfferDetails::default(),
            },
        },
    )
    .unwrap();
    let mut value = serde_json::to_value(BaseModel::new(id.into())).unwrap();
    value.as_object_mut().unwrap().extend(
        json!({
            "batch_id":"batch", "source":source, "initially_adopted":false,
            "mapping":{"category_id":"", "unit_id":"", "product_kind":"PHYSICAL"},
            "sales_prices":BatchSalesValues::default(), "status":"DRAFT", "messages":[], "warnings":[],
            "warnings_confirmed":false
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    serde_json::from_value(value).unwrap()
}

#[test]
fn chunk_updates_use_per_row_cas_literal_data_and_preserve_internal_identity() {
    let rows = [row("$one"), row("two")];
    let (filter, pipeline) = draft_chunk_update(&rows, 123).unwrap();
    assert_eq!(filter.get_str("batch_id").unwrap(), "batch");
    let versions = filter.get_array("$or").unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].as_document().unwrap().get_i64("version").unwrap(), 1);
    let merge = pipeline[0].get_document("$replaceWith").unwrap().get_array("$mergeObjects").unwrap();
    assert_eq!(merge[0].as_str(), Some("$$ROOT"));
    let branches =
        merge[1].as_document().unwrap().get_document("$switch").unwrap().get_array("branches").unwrap();
    let branch = branches[0].as_document().unwrap();
    let value = branch.get_document("then").unwrap().get_document("$literal").unwrap();
    assert_eq!(value.get_str("id").unwrap(), "$one");
    assert_eq!(value.get_i64("version").unwrap(), 2);
    assert_eq!(value.get_i64("updated_at").unwrap(), 123);
    assert_eq!(value.get_document("source").unwrap().get_str("name").unwrap(), "$literal source");
    assert_eq!(rows[0].base.version, 1);
}

#[test]
fn chunk_rejects_duplicate_cross_batch_deleted_and_unbounded_updates() {
    let row = row("one");
    assert!(draft_chunk_update(&[], 1).is_err());
    assert!(draft_chunk_update(&[row.clone(), row.clone()], 1).is_err());
    assert!(draft_chunk_update(&vec![row.clone(); 201], 1).is_err());
    let mut other = row.clone();
    other.base.id = "two".into();
    other.batch_id = "other".into();
    assert!(draft_chunk_update(&[row.clone(), other], 1).is_err());
    let mut deleted = row.clone();
    deleted.base.deleted_at = 1;
    assert!(draft_chunk_update(&[deleted], 1).is_err());
    let mut overflow = row;
    overflow.base.version = i64::MAX as u64;
    assert!(draft_chunk_update(&[overflow], 1).is_err());
}
