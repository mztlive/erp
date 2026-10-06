use super::*;
use crate::portal::repository::category_mapping_filter;

fn hierarchy() -> Vec<CategoryHierarchyNode> {
    vec![
        CategoryHierarchyNode {
            id: "root-1".into(),
            version: 2,
            name: "公司礼品".into(),
            parent_id: None,
            product_kind: ProductKind::Physical,
        },
        CategoryHierarchyNode {
            id: "category-1".into(),
            version: 4,
            name: "杯具".into(),
            parent_id: Some("root-1".into()),
            product_kind: ProductKind::Physical,
        },
    ]
}

fn confirmation(submission_id: &str) -> CategoryMappingConfirmation {
    let hierarchy = hierarchy();
    CategoryMappingConfirmation {
        draft_id: "draft-1".into(),
        submission_id: submission_id.into(),
        source_original_category_path: "  供应商礼品 > 杯具  ".into(),
        category_id: "category-1".into(),
        category_version: 4,
        category_path: hierarchy_path(&hierarchy),
        hierarchy,
        confirmed_by: "buyer-1".into(),
        confirmed_at: Instant::from_unix_secs(1_700_000_010),
        reason: "已核对供应商完整路径、公司层级与商品类型".into(),
    }
}

fn mapping() -> SupplierCategoryMapping {
    SupplierCategoryMapping::new(
        "mapping-1".into(),
        "supplier-1".into(),
        "  供应商礼品 > 杯具  ".into(),
        ProductKind::Physical,
        confirmation("submission-1"),
    )
    .unwrap()
}

fn candidate(nodes: &[CategoryHierarchyNode]) -> DictionaryCandidate {
    let leaf = nodes.last().unwrap();
    DictionaryCandidate {
        id: leaf.id.clone(),
        version: leaf.version,
        code: "C-001".into(),
        name: leaf.name.clone(),
        path: Some(hierarchy_path(nodes)),
        parent_id: leaf.parent_id.clone(),
        product_kind: Some(leaf.product_kind),
        quantity_scale: None,
        hierarchy: nodes.to_vec(),
    }
}

#[test]
fn source_key_isolates_supplier_complete_path_and_kind() {
    let original = category_mapping_filter("supplier-1", "礼品 > 杯具", ProductKind::Physical);
    for other in [
        category_mapping_filter("supplier-2", "礼品 > 杯具", ProductKind::Physical),
        category_mapping_filter("supplier-1", "生活用品 > 杯具", ProductKind::Physical),
        category_mapping_filter("supplier-1", "礼品 > 杯具", ProductKind::Virtual),
    ] {
        assert_ne!(original, other);
    }
    assert_eq!(original.get_str("original_category_path").unwrap(), "礼品 > 杯具");
}

#[test]
fn source_path_preserves_supplier_meaning_and_original_evidence() {
    let mapping = mapping();
    assert_eq!(mapping.original_category_path, "供应商礼品 > 杯具");
    assert_eq!(mapping.confirmations[0].source_original_category_path, "  供应商礼品 > 杯具  ");
    assert_ne!(source_path("礼品 > 杯具").unwrap(), source_path("礼品 / 杯具").unwrap());
    assert!(source_path("  ").is_err());
}

#[test]
fn new_confirmation_appends_immutable_history() {
    let mut mapping = mapping();
    let original = serde_json::to_value(&mapping.confirmations[0]).unwrap();
    let mut next = confirmation("submission-2");
    next.hierarchy[1].version = 5;
    next.category_version = 5;
    next.confirmed_by = "buyer-2".into();
    mapping.confirm(1, next).unwrap();
    assert_eq!(mapping.confirmations.len(), 2);
    assert_eq!(serde_json::to_value(&mapping.confirmations[0]).unwrap(), original);
    assert_eq!(mapping.confirmations[1].category_version, 5);
    assert_eq!(mapping.confirmations[1].confirmed_by, "buyer-2");
}

#[test]
fn stale_or_duplicate_confirmation_cannot_mutate_history() {
    let mut mapping = mapping();
    assert!(matches!(mapping.confirm(2, confirmation("submission-2")), Err(Error::ConflictError(_))));
    assert!(matches!(mapping.confirm(1, confirmation("submission-1")), Err(Error::ConflictError(_))));
    assert_eq!(mapping.confirmations.len(), 1);
}

#[test]
fn unchanged_history_still_requires_explicit_confirmation() {
    let nodes = hierarchy();
    let suggestion = mapping().suggestion(Some((candidate(&nodes), nodes))).unwrap();
    assert_eq!(suggestion.status, CategoryMappingSuggestionStatus::ConfirmationRequired);
    assert!(suggestion.requires_confirmation);
    let safe = serde_json::to_value(suggestion).unwrap();
    for internal in ["supplier_id", "confirmed_by", "reason", "draft_id", "submission_id", "hierarchy"] {
        assert!(safe.get(internal).is_none());
    }
}

#[test]
fn target_or_ancestor_change_requires_recheck() {
    for change in 0..5 {
        let mut nodes = hierarchy();
        match change {
            0 => nodes[0].version += 1,
            1 => nodes[0].name = "新公司礼品路径".into(),
            2 => nodes[1].parent_id = Some("other-root".into()),
            3 => nodes[1].version += 1,
            _ => nodes[1].product_kind = ProductKind::Virtual,
        }
        let suggestion = mapping().suggestion(Some((candidate(&nodes), nodes))).unwrap();
        assert_eq!(suggestion.status, CategoryMappingSuggestionStatus::RecheckRequired);
        assert!(suggestion.requires_confirmation);
    }
    let unavailable = mapping().suggestion(None).unwrap();
    assert_eq!(unavailable.status, CategoryMappingSuggestionStatus::RecheckRequired);
    assert!(unavailable.category.is_none());
}

#[test]
fn confirmation_rejects_changed_original_path_and_invalid_hierarchy() {
    let mut mapping = mapping();
    let mut changed_raw = confirmation("submission-2");
    changed_raw.source_original_category_path = "其他供应商路径 > 杯具".into();
    assert!(mapping.confirm(1, changed_raw).is_err());
    let mut changed_parent = confirmation("submission-2");
    changed_parent.hierarchy[1].parent_id = Some("other-root".into());
    assert!(mapping.confirm(1, changed_parent).is_err());
    let mut changed_kind = confirmation("submission-2");
    changed_kind.hierarchy[0].product_kind = ProductKind::Virtual;
    assert!(mapping.confirm(1, changed_kind).is_err());
    assert_eq!(mapping.confirmations.len(), 1);
}

#[test]
fn confirmation_requires_real_reason_and_complete_target_version() {
    let mut mapping = mapping();
    let mut empty = confirmation("submission-2");
    empty.reason = " ".into();
    assert!(mapping.confirm(1, empty).is_err());
    let mut version = confirmation("submission-2");
    version.category_version = 9;
    assert!(mapping.confirm(1, version).is_err());
    let mut path = confirmation("submission-2");
    path.category_path = "杯具".into();
    assert!(mapping.confirm(1, path).is_err());
    assert_eq!(mapping.confirmations.len(), 1);
}
