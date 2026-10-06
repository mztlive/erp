use std::str::FromStr;

use serde_json::json;

use super::*;

fn dictionary(raw_name: &str) -> DictionaryInput {
    DictionaryInput { raw_name: raw_name.into(), selected_id: None, expected_version: None }
}

fn input() -> NewProductInput {
    NewProductInput {
        name: "  保温杯  ".into(),
        product_kind: ProductKind::Physical,
        brand: dictionary(" 新品牌建议 "),
        category: dictionary(" 礼品 > 杯具 "),
        model: Some("  M-1  ".into()),
        description: Some("供应商原始描述".into()),
        image_asset_ids: vec!["image-1".into()],
        file_asset_ids: vec!["file-1".into()],
        skus: vec![DraftSku {
            row_id: "row-1".into(),
            name: "  保温杯红色  ".into(),
            spec_entries: vec![SpecEntryInput {
                attribute_code: " 颜色 ".into(),
                attribute_value_code: " 红色 ".into(),
            }],
            unit: dictionary(" pcs（单个） "),
            packaging: None,
            quote_basis: None,
            barcode: Some("BAR-1".into()),
            image_asset_id: Some("sku-image-1".into()),
            ordering_code: "  SUP-1  ".into(),
            supply_terms: json!({"price": "120.00", "tax_rate": "0.13"}),
            available_quantity: None,
            reported_at: Instant::from_unix_secs(1_700_000_000),
        }],
    }
}

#[test]
fn final_submission_images_support_common_fallback_and_require_each_sku_source() {
    let mut input = input();
    input.skus.push(DraftSku { row_id: "row-2".into(), image_asset_id: None, ..input.skus[0].clone() });
    assert!(input.ensure_submission_images().is_ok(), "公共图片可供所有 SKU 共同引用");
    input.image_asset_ids.clear();
    assert!(matches!(input.ensure_submission_images(), Err(Error::ValidationError(_))));
    input.skus[1].image_asset_id = Some("sku-2-image".into());
    assert!(input.ensure_submission_images().is_ok(), "逐 SKU 图片完整时不要求公共图");
    input.skus[1].image_asset_id = Some(" ".into());
    assert!(input.ensure_submission_images().is_err());
    input.skus.clear();
    assert!(input.ensure_submission_images().is_err());
}

fn normalized() -> NormalizedProduct {
    NormalizedProduct {
        name: "保温杯".into(),
        brand_id: "brand-1".into(),
        brand_version: 3,
        category_id: "category-1".into(),
        category_version: 4,
        category_hierarchy: vec![CategoryHierarchyNode {
            id: "category-1".into(),
            version: 4,
            name: "杯具".into(),
            parent_id: None,
            product_kind: ProductKind::Physical,
        }],
        sku_mappings: vec![NormalizedSku {
            row_id: "row-1".into(),
            name: "保温杯红色".into(),
            unit_id: "unit-1".into(),
            unit_version: 2,
            unit_synonym_confirmation: None,
            target_sku: None,
        }],
    }
}

#[test]
fn review_category_mapping_requires_a_complete_contiguous_frozen_chain() {
    let input = input();
    assert!(input.ensure_normalized(&normalized()).is_ok());
    let mut mapping = normalized();
    mapping.category_hierarchy.clear();
    assert!(input.ensure_normalized(&mapping).is_err());
    let mut mapping = normalized();
    mapping.category_hierarchy[0].parent_id = Some("missing-root".into());
    assert!(input.ensure_normalized(&mapping).is_err());
    let mut mapping = normalized();
    mapping.category_hierarchy[0].version += 1;
    assert!(input.ensure_normalized(&mapping).is_err());
    let mut mapping = normalized();
    mapping.category_hierarchy[0].product_kind = ProductKind::Voucher;
    assert!(input.ensure_normalized(&mapping).is_err());
    let mut mapping = normalized();
    let mut root = mapping.category_hierarchy[0].clone();
    root.id = "root".into();
    mapping.category_hierarchy[0].parent_id = Some("root".into());
    mapping.category_hierarchy.insert(0, root);
    assert!(input.ensure_normalized(&mapping).is_ok());
    mapping.category_hierarchy[1].parent_id = Some("other-root".into());
    assert!(input.ensure_normalized(&mapping).is_err());
}

fn result() -> CatalogDraftResult {
    CatalogDraftResult {
        product_id: "product-1".into(),
        product_created: true,
        skus: vec![CatalogDraftSkuResult {
            row_id: "row-1".into(),
            sku_id: "sku-1".into(),
            revision_id: "sku-revision-1".into(),
            sku_created: true,
            listing_status: ListingStatus::Unlisted,
            offering_id: Some("offering-1".into()),
        }],
    }
}

fn effective_command(
    expected_version: u64,
    normalized: NormalizedProduct,
    result: CatalogDraftResult,
) -> CatalogDraftEffectiveCommand {
    CatalogDraftEffectiveCommand {
        draft_id: "draft-1".into(),
        expected_version,
        normalized,
        result,
        actor_id: "buyer".into(),
        reason: "已核对原始品牌分类、规格、单位与明确建档匹配结果".into(),
    }
}

fn draft() -> NewProductDraft {
    let mut draft =
        NewProductDraft::new("draft-1".into(), "supplier-1".into(), "supplier-user".into(), input()).unwrap();
    draft.base = BaseModel { id: "draft-1".into(), ..BaseModel::fake() };
    draft
}

fn pending() -> NewProductDraft {
    let mut draft = draft();
    draft
        .submit(
            1,
            "submission-1".into(),
            "task-1".into(),
            "supplier-user".into(),
            Instant::from_unix_secs(1_700_000_010),
        )
        .unwrap();
    draft
}

fn state(status: DraftStatus) -> NewProductDraft {
    if status == DraftStatus::Draft {
        return draft();
    }
    let mut draft = pending();
    let at = Instant::from_unix_secs(1_700_000_100);
    match status {
        DraftStatus::Returned => {
            draft.return_to_supplier(1, "submission-1", "补充单位说明".into(), "buyer", at).unwrap()
        },
        DraftStatus::Withdrawn => draft.withdraw(1, "submission-1", "supplier-user", at).unwrap(),
        DraftStatus::Effective => {
            draft.mark_effective("submission-1", effective_command(1, normalized(), result()), at).unwrap()
        },
        DraftStatus::Draft | DraftStatus::Pending => {},
    }
    draft
}

fn snapshot(draft: &NewProductDraft) -> Value {
    serde_json::to_value(draft).unwrap()
}

fn packaging() -> PackagingInput {
    PackagingInput {
        original_unit: " 箱 ".into(),
        base_unit: "瓶".into(),
        units_per_package: Quantity::from_str("12").unwrap(),
        original_unit_price: " 120.00 ".into(),
        conversion_confirmed_by_supplier: true,
    }
}

fn packaged_input() -> NewProductInput {
    let mut input = input();
    input.skus[0].unit = dictionary(" 瓶 ");
    input.skus[0].packaging = Some(packaging());
    input.skus[0].quote_basis = Some(" 120元每箱，每箱12瓶；我确认按瓶填报供价和数量 ".into());
    input.skus[0].supply_terms = json!({"price": "10.00", "tax_rate": "0.13"});
    input.skus[0].available_quantity = Some(Quantity::from_str("24").unwrap());
    input
}

#[test]
fn supplier_confirmed_packaging_and_raw_quote_basis_are_preserved_without_conversion() {
    let input = packaged_input();
    let original = serde_json::to_value(&input).unwrap();
    assert!(input.validate_submission().is_ok());
    assert!(input.ensure_normalized(&normalized()).is_ok());
    let mut draft =
        NewProductDraft::new("draft-1".into(), "supplier-1".into(), "supplier-user".into(), input).unwrap();
    draft.submit(1, "submission-1".into(), "task-1".into(), "supplier-user".into(), Instant::now()).unwrap();
    assert_eq!(serde_json::to_value(draft.submitted().unwrap()).unwrap(), original);
    let sku = &draft.submitted().unwrap().skus[0];
    assert_eq!(sku.supply_terms["price"], json!("10.00"));
    assert_eq!(sku.available_quantity, Some(Quantity::from_str("24").unwrap()));
    assert_eq!(sku.packaging.as_ref().unwrap().original_unit_price, " 120.00 ");
    assert_eq!(sku.packaging.as_ref().unwrap().original_unit, " 箱 ");
    assert_eq!(sku.packaging.as_ref().unwrap().units_per_package.to_string(), "12");
    assert_eq!(sku.quote_basis.as_deref(), Some(" 120元每箱，每箱12瓶；我确认按瓶填报供价和数量 "));
}

#[test]
fn unconfirmed_packaging_can_be_saved_but_cannot_be_submitted_or_mapped() {
    let mut input = packaged_input();
    input.skus[0].packaging.as_mut().unwrap().conversion_confirmed_by_supplier = false;
    let mut draft =
        NewProductDraft::new("draft-1".into(), "supplier-1".into(), "supplier-user".into(), input.clone())
            .unwrap();
    let original = snapshot(&draft);
    assert!(input.validate_submission().is_err());
    assert!(input.ensure_normalized(&normalized()).is_err());
    assert!(
        draft
            .submit(1, "submission-1".into(), "task-1".into(), "supplier-user".into(), Instant::now())
            .is_err()
    );
    assert_eq!(snapshot(&draft), original);
}

#[test]
fn packaging_requires_positive_count_and_the_exact_submitted_base_unit() {
    for count in ["0", "-1", "-0.000001"] {
        let mut input = packaged_input();
        input.skus[0].packaging.as_mut().unwrap().units_per_package = Quantity::from_str(count).unwrap();
        assert!(matches!(input.validate_submission(), Err(Error::ValidationError(_))));
    }
    let mut input = packaged_input();
    input.skus[0].packaging.as_mut().unwrap().base_unit = "件".into();
    assert!(input.validate_submission().is_err());
    let mut input = packaged_input();
    input.skus[0].packaging.as_mut().unwrap().base_unit = " 瓶 ".into();
    assert!(input.validate_submission().is_err());
    let mut input = packaged_input();
    input.skus[0].packaging.as_mut().unwrap().units_per_package = Quantity::from_str("0.000001").unwrap();
    assert!(input.validate_submission().is_ok());
}

#[test]
fn packaging_and_quote_basis_text_must_be_present_within_limits() {
    let mut cases = Vec::new();
    let mut changed = packaged_input();
    changed.skus[0].packaging.as_mut().unwrap().original_unit = " ".into();
    cases.push(changed);
    let mut changed = packaged_input();
    changed.skus[0].packaging.as_mut().unwrap().base_unit.clear();
    cases.push(changed);
    let mut changed = packaged_input();
    changed.skus[0].packaging.as_mut().unwrap().original_unit_price = " ".into();
    cases.push(changed);
    let mut changed = packaged_input();
    changed.skus[0].packaging.as_mut().unwrap().original_unit_price = "1".repeat(65);
    cases.push(changed);
    let mut changed = packaged_input();
    changed.skus[0].packaging.as_mut().unwrap().original_unit = "长".repeat(65);
    cases.push(changed);
    let mut changed = packaged_input();
    changed.skus[0].quote_basis = Some(" ".into());
    cases.push(changed);
    let mut changed = packaged_input();
    changed.skus[0].quote_basis = Some("长".repeat(513));
    cases.push(changed);
    for input in cases {
        assert!(input.validate_submission().is_err());
    }
    let mut input = packaged_input();
    input.skus[0].packaging.as_mut().unwrap().original_unit_price = "供应商原报价待供给Port校验".into();
    assert!(input.validate_submission().is_ok(), "商品领域不解析原报价数值或金额规则");
}

#[test]
fn packaging_changes_require_supplier_resubmission_and_preserve_old_evidence() {
    let mut draft =
        NewProductDraft::new("draft-1".into(), "supplier-1".into(), "supplier-user".into(), packaged_input())
            .unwrap();
    draft.submit(1, "submission-1".into(), "task-1".into(), "supplier-user".into(), Instant::now()).unwrap();
    draft.map(1, normalized()).unwrap();
    let original = serde_json::to_value(&draft.submissions[0].input).unwrap();
    assert_eq!(serde_json::to_value(draft.submitted().unwrap()).unwrap(), original);
    let mut changed = packaged_input();
    changed.skus[0].packaging.as_mut().unwrap().units_per_package = Quantity::from_str("6").unwrap();
    changed.skus[0].packaging.as_mut().unwrap().original_unit_price = "60.00".into();
    changed.skus[0].quote_basis = Some("60元每箱，每箱6瓶；重新确认".into());
    assert!(draft.update(1, changed.clone()).is_err());
    draft.return_to_supplier(1, "submission-1", "重新核对包装".into(), "buyer", Instant::now()).unwrap();
    draft.update(1, changed).unwrap();
    draft.submit(1, "submission-2".into(), "task-2".into(), "supplier-user".into(), Instant::now()).unwrap();
    assert!(draft.normalized_product.is_none());
    assert_eq!(serde_json::to_value(&draft.submissions[0].input).unwrap(), original);
    assert_eq!(
        draft.submissions[0].input.skus[0].packaging.as_ref().unwrap().original_unit_price,
        " 120.00 "
    );
    assert_eq!(draft.submitted().unwrap().skus[0].packaging.as_ref().unwrap().original_unit_price, "60.00");
    assert_eq!(
        draft.submitted().unwrap().skus[0].quote_basis.as_deref(),
        Some("60元每箱，每箱6瓶；重新确认")
    );
}

#[test]
fn internal_mapping_cannot_carry_packaging_or_quote_changes() {
    let mut encoded = serde_json::to_value(normalized()).unwrap();
    encoded["sku_mappings"][0]["packaging"] = serde_json::to_value(packaging()).unwrap();
    assert!(serde_json::from_value::<NormalizedProduct>(encoded).is_err());
    let mut encoded = serde_json::to_value(normalized()).unwrap();
    encoded["sku_mappings"][0]["quote_basis"] = json!("后台静默换算");
    assert!(serde_json::from_value::<NormalizedProduct>(encoded).is_err());
    let mut draft =
        NewProductDraft::new("draft-1".into(), "supplier-1".into(), "supplier-user".into(), packaged_input())
            .unwrap();
    draft.submit(1, "submission-1".into(), "task-1".into(), "supplier-user".into(), Instant::now()).unwrap();
    let original = serde_json::to_value(draft.submitted().unwrap()).unwrap();
    draft.map(1, normalized()).unwrap();
    assert_eq!(serde_json::to_value(draft.submitted().unwrap()).unwrap(), original);
}

#[test]
fn draft_saves_incomplete_input_but_submission_requires_complete_input() {
    let mut input = input();
    input.name.clear();
    input.brand.raw_name.clear();
    input.category.raw_name.clear();
    input.skus.clear();
    let mut draft =
        NewProductDraft::new("draft-1".into(), "supplier-1".into(), "supplier-user".into(), input).unwrap();
    let original = snapshot(&draft);
    assert!(
        draft
            .submit(1, "submission-1".into(), "task-1".into(), "supplier-user".into(), Instant::now())
            .is_err()
    );
    assert_eq!(snapshot(&draft), original);
    assert_eq!(draft.status, DraftStatus::Draft);
}

#[test]
fn sufficient_unmatched_dictionary_input_can_be_submitted_and_raw_values_are_preserved() {
    let original = serde_json::to_value(input()).unwrap();
    let draft = pending();
    assert_eq!(serde_json::to_value(draft.submitted().unwrap()).unwrap(), original);
    assert_eq!(serde_json::to_value(&draft.submissions[0].input).unwrap(), original);
    assert_eq!(serde_json::to_value(&draft.draft).unwrap(), original);
    assert_eq!(draft.current_submission_id.as_deref(), Some("submission-1"));
    assert_eq!(draft.task_id.as_deref(), Some("task-1"));
    assert_eq!(draft.submissions[0].submitted_by, "supplier-user");
    assert!(draft.result.is_none());
}

#[test]
fn dictionaries_require_id_and_positive_version_together() {
    for dictionary in [
        DictionaryInput {
            raw_name: "品牌".into(),
            selected_id: Some("brand-1".into()),
            expected_version: None,
        },
        DictionaryInput { raw_name: "品牌".into(), selected_id: None, expected_version: Some(1) },
        DictionaryInput {
            raw_name: "品牌".into(),
            selected_id: Some("brand-1".into()),
            expected_version: Some(0),
        },
    ] {
        let mut input = input();
        input.brand = dictionary;
        assert!(matches!(input.validate_submission(), Err(Error::ValidationError(_))));
    }
    let mut input = input();
    input.brand = DictionaryInput {
        raw_name: "品牌".into(),
        selected_id: Some("brand-1".into()),
        expected_version: Some(3),
    };
    assert!(input.validate_submission().is_ok());
}

#[test]
fn submission_rejects_missing_fields_and_nonobject_terms() {
    let mut cases = Vec::new();
    let mut changed = input();
    changed.name = " ".into();
    cases.push(changed);
    let mut changed = input();
    changed.brand.raw_name = " ".into();
    cases.push(changed);
    let mut changed = input();
    changed.category.raw_name = " ".into();
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].name.clear();
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].unit.raw_name.clear();
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].ordering_code = " ".into();
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].supply_terms = json!([]);
    cases.push(changed);
    let mut changed = input();
    changed.skus.clear();
    cases.push(changed);
    for input in cases {
        assert!(matches!(input.validate_submission(), Err(Error::ValidationError(_))));
    }
}

#[test]
fn submission_rejects_duplicate_rows_specifications_and_ordering_codes() {
    let mut original = input();
    let mut second = original.skus[0].clone();
    second.row_id = "row-2".into();
    second.ordering_code = "SUP-2".into();
    original.skus.push(second);
    assert!(original.validate_submission().is_err(), "原始规格规范化后仍重复");
    original.skus[1].spec_entries[0].attribute_value_code = "蓝色".into();
    assert!(original.validate_submission().is_ok());
    original.skus[1].ordering_code = "SUP-1".into();
    assert!(original.validate_submission().is_err(), "订货编码按trim后判重");
    original.skus[1].ordering_code = "SUP-2".into();
    original.skus[1].row_id = "row-1".into();
    assert!(original.validate_submission().is_err());
}

#[test]
fn empty_specification_is_valid_but_two_empty_combinations_are_duplicates() {
    let mut input = input();
    input.skus[0].spec_entries.clear();
    assert!(input.validate_submission().is_ok());
    let mut second = input.skus[0].clone();
    second.row_id = "row-2".into();
    second.ordering_code = "SUP-2".into();
    input.skus.push(second);
    assert!(input.validate_submission().is_err());
}

#[test]
fn specification_delimiters_cannot_change_the_formal_attribute_meaning() {
    let mut input = input();
    input.skus[0].spec_entries[0].attribute_code = "容量=包装".into();
    assert!(input.validate_submission().is_err());
    input.skus[0].spec_entries[0].attribute_code = "容量".into();
    input.skus[0].spec_entries[0].attribute_value_code = "500ml|包装=箱".into();
    assert!(input.validate_submission().is_err());
    input.skus[0].spec_entries[0].attribute_value_code = "500ml".into();
    assert!(input.validate_submission().is_ok());
}

#[test]
fn invalid_counts_quantity_dates_ids_and_long_text_are_rejected() {
    let mut cases = Vec::new();
    let mut changed = input();
    changed.name = "长".repeat(129);
    cases.push(changed);
    let mut changed = input();
    changed.description = Some("长".repeat(10_001));
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].available_quantity = Some(Quantity::from_str("-0.001").unwrap());
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].reported_at = Instant::from_unix_secs(-1);
    cases.push(changed);
    let mut changed = input();
    changed.skus[0].row_id = " row-1 ".into();
    cases.push(changed);
    let mut changed = input();
    changed.image_asset_ids.push("image-1".into());
    cases.push(changed);
    let mut changed = input();
    changed.skus.resize(101, changed.skus[0].clone());
    cases.push(changed);
    for input in cases {
        assert!(input.validate_submission().is_err());
    }
    let mut valid = input();
    valid.skus[0].available_quantity = Some(Quantity::from_str("0.000001").unwrap());
    assert!(valid.validate_submission().is_ok());
}

#[test]
fn mapping_only_trims_names_and_preserves_raw_terms_and_specifications() {
    let input = input();
    let original = serde_json::to_value(&input).unwrap();
    assert!(input.ensure_normalized(&normalized()).is_ok());
    assert_eq!(serde_json::to_value(&input).unwrap(), original);
    let mut changed = normalized();
    changed.name = "其他商品".into();
    assert!(input.ensure_normalized(&changed).is_err());
    let mut changed = normalized();
    changed.sku_mappings[0].name = "保温杯蓝色".into();
    assert!(input.ensure_normalized(&changed).is_err());
    let mut changed = normalized();
    changed.sku_mappings[0].name = " 保温杯红色 ".into();
    assert!(input.ensure_normalized(&changed).is_err());
}

#[test]
fn mapping_requires_exact_rows_unique_targets_and_valid_target_versions() {
    let input = input();
    let mut changed = normalized();
    changed.sku_mappings.clear();
    assert!(input.ensure_normalized(&changed).is_err());
    let mut changed = normalized();
    changed.sku_mappings[0].row_id = "other-row".into();
    assert!(input.ensure_normalized(&changed).is_err());
    let mut changed = normalized();
    changed.sku_mappings[0].unit_version = 0;
    assert!(input.ensure_normalized(&changed).is_err());
    let mut changed = normalized();
    changed.sku_mappings[0].target_sku =
        Some(ExistingSkuRef { sku_id: "sku-1".into(), revision_id: "revision-1".into(), version: 0 });
    assert!(input.ensure_normalized(&changed).is_err());
    let mut input = input;
    let mut second = input.skus[0].clone();
    second.row_id = "row-2".into();
    second.ordering_code = "SUP-2".into();
    second.spec_entries[0].attribute_value_code = "蓝色".into();
    input.skus.push(second);
    let mut mapping = normalized();
    mapping.sku_mappings[0].target_sku =
        Some(ExistingSkuRef { sku_id: "sku-1".into(), revision_id: "revision-1".into(), version: 1 });
    let mut second = mapping.sku_mappings[0].clone();
    second.row_id = "row-2".into();
    mapping.sku_mappings.push(second);
    assert!(input.ensure_normalized(&mapping).is_err(), "同一个已有SKU不能匹配两行");
}

#[test]
fn mapping_cannot_replace_selected_dictionary_or_its_version() {
    let mut input = input();
    input.brand.selected_id = Some("brand-1".into());
    input.brand.expected_version = Some(3);
    input.category.selected_id = Some("category-1".into());
    input.category.expected_version = Some(4);
    input.skus[0].unit.selected_id = Some("unit-1".into());
    input.skus[0].unit.expected_version = Some(2);
    assert!(input.ensure_normalized(&normalized()).is_ok());
    let mut changed = normalized();
    changed.brand_version = 4;
    assert!(matches!(input.ensure_normalized(&changed), Err(Error::ConflictError(_))));
    let mut changed = normalized();
    changed.category_id = "category-2".into();
    assert!(matches!(input.ensure_normalized(&changed), Err(Error::ConflictError(_))));
    let mut changed = normalized();
    changed.sku_mappings[0].unit_id = "unit-box".into();
    assert!(matches!(input.ensure_normalized(&changed), Err(Error::ConflictError(_))));
}

#[test]
fn internal_mapping_is_separate_pending_data_and_new_submission_requires_rechecking() {
    let mut draft = pending();
    let frozen = serde_json::to_value(draft.submitted().unwrap()).unwrap();
    draft.map(1, normalized()).unwrap();
    assert_eq!(draft.normalized_product.as_ref().unwrap().name, "保温杯");
    assert_eq!(serde_json::to_value(draft.submitted().unwrap()).unwrap(), frozen);
    draft.return_to_supplier(1, "submission-1", "补充资料".into(), "buyer", Instant::now()).unwrap();
    assert!(draft.submissions[0].decision.as_ref().unwrap().normalized_product.is_some());
    assert!(draft.map(1, normalized()).is_err());
    draft.update(1, input()).unwrap();
    assert!(draft.normalized_product.is_none());
    draft.submit(1, "submission-2".into(), "task-2".into(), "supplier-user".into(), Instant::now()).unwrap();
    assert!(draft.normalized_product.is_none());
    assert!(draft.submissions[0].decision.as_ref().unwrap().normalized_product.is_some());
    let original = snapshot(&draft);
    assert!(draft.map(2, normalized()).is_err());
    let mut invalid = normalized();
    invalid.name = "另一商品".into();
    assert!(draft.map(1, invalid).is_err());
    assert_eq!(snapshot(&draft), original);
}

#[test]
fn status_matrix_only_allows_editable_submit_and_pending_decisions() {
    for status in [
        DraftStatus::Draft,
        DraftStatus::Pending,
        DraftStatus::Returned,
        DraftStatus::Withdrawn,
        DraftStatus::Effective,
    ] {
        let original = state(status);
        let editable = matches!(status, DraftStatus::Draft | DraftStatus::Returned | DraftStatus::Withdrawn);
        assert_eq!(original.clone().update(1, input()).is_ok(), editable, "{status:?}更新");
        assert_eq!(
            original
                .clone()
                .submit(1, "submission-2".into(), "task-2".into(), "supplier-user".into(), Instant::now())
                .is_ok(),
            editable,
            "{status:?}提交"
        );
        assert_eq!(
            original.clone().withdraw(1, "submission-1", "supplier-user", Instant::now()).is_ok(),
            status == DraftStatus::Pending
        );
        assert_eq!(
            original
                .clone()
                .return_to_supplier(1, "submission-1", "补充资料".into(), "buyer", Instant::now())
                .is_ok(),
            status == DraftStatus::Pending
        );
        assert_eq!(
            original
                .clone()
                .mark_effective("submission-1", effective_command(1, normalized(), result()), Instant::now())
                .is_ok(),
            status == DraftStatus::Pending
        );
        assert_eq!(original.submitted().is_ok(), status == DraftStatus::Pending);
    }
}

#[test]
fn return_edit_resubmit_retains_original_input_decision_and_task_history() {
    let mut draft = pending();
    let original = serde_json::to_value(&draft.submissions[0].input).unwrap();
    let at = Instant::from_unix_secs(1_700_000_100);
    draft.return_to_supplier(1, "submission-1", "  包装说明不足  ".into(), "buyer", at).unwrap();
    assert!(draft.task_id.is_none());
    let mut changed = input();
    changed.model = Some("M-2".into());
    draft.update(1, changed).unwrap();
    draft
        .submit(
            1,
            "submission-2".into(),
            "task-2".into(),
            "supplier-user-2".into(),
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
    assert_eq!(draft.submissions.len(), 2);
    assert_eq!(serde_json::to_value(&draft.submissions[0].input).unwrap(), original);
    assert_eq!(draft.submissions[0].task_id, "task-1");
    let decision = draft.submissions[0].decision.as_ref().unwrap();
    assert_eq!(decision.reason.as_deref(), Some("包装说明不足"));
    assert_eq!(decision.decided_by, "buyer");
    assert_eq!(decision.decided_at, at);
    assert!(draft.submissions[1].decision.is_none());
    assert_eq!(draft.submitted().unwrap().model.as_deref(), Some("M-2"));
    assert_eq!(draft.current_submission_id.as_deref(), Some("submission-2"));
    assert!(draft.withdraw(1, "submission-1", "supplier-user", Instant::now()).is_err());
}

#[test]
fn reuse_of_submission_or_task_identity_is_rejected_before_mutation() {
    let original = state(DraftStatus::Withdrawn);
    for (submission_id, task_id) in [("submission-1", "task-2"), ("submission-2", "task-1")] {
        let mut draft = original.clone();
        assert!(matches!(
            draft.submit(1, submission_id.into(), task_id.into(), "supplier-user".into(), Instant::now()),
            Err(Error::ConflictError(_))
        ));
        assert_eq!(snapshot(&draft), snapshot(&original));
    }
}

#[test]
fn version_conflicts_soft_deletes_and_empty_return_reasons_leave_state_unchanged() {
    let mut draft = pending();
    let original = snapshot(&draft);
    assert!(draft.withdraw(0, "submission-1", "supplier-user", Instant::now()).is_err());
    assert!(draft.return_to_supplier(2, "submission-1", "补充资料".into(), "buyer", Instant::now()).is_err());
    assert!(draft.return_to_supplier(1, "submission-1", " ".into(), "buyer", Instant::now()).is_err());
    assert!(
        draft
            .mark_effective("submission-1", effective_command(2, normalized(), result()), Instant::now())
            .is_err()
    );
    assert!(draft.withdraw(1, "submission-1", " ", Instant::now()).is_err());
    assert_eq!(snapshot(&draft), original);
    draft.base.deleted_at = 1;
    assert!(matches!(draft.ensure_version(1), Err(Error::NotFound(_))));
    assert!(draft.submitted().is_err());
}

#[test]
fn effective_records_supplier_and_internal_facts_separately_and_keeps_new_sku_unlisted() {
    let mut draft = pending();
    let raw = serde_json::to_value(draft.submitted().unwrap()).unwrap();
    let reported_at = draft.submitted().unwrap().skus[0].reported_at;
    let at = Instant::from_unix_secs(1_700_000_100);
    draft.mark_effective("submission-1", effective_command(1, normalized(), result()), at).unwrap();
    assert_eq!(draft.status, DraftStatus::Effective);
    assert!(draft.task_id.is_none());
    assert_eq!(serde_json::to_value(draft.frozen_input.as_ref().unwrap()).unwrap(), raw);
    assert_eq!(draft.submissions[0].submitted_by, "supplier-user");
    assert_eq!(draft.submissions[0].input.skus[0].reported_at, reported_at);
    let decision = draft.submissions[0].decision.as_ref().unwrap();
    assert_eq!(decision.decided_by, "buyer");
    assert_eq!(decision.decided_at, at);
    assert_eq!(decision.reason.as_deref(), Some("已核对原始品牌分类、规格、单位与明确建档匹配结果"));
    assert_eq!(decision.normalized_product.as_ref().unwrap().name, "保温杯");
    assert_eq!(draft.result.as_ref().unwrap().skus[0].listing_status, ListingStatus::Unlisted);
    assert_eq!(decision.result.as_ref().unwrap().skus[0].offering_id.as_deref(), Some("offering-1"));
    let restored: NewProductDraft = serde_json::from_value(snapshot(&draft)).unwrap();
    assert_eq!(snapshot(&restored), snapshot(&draft));
}

#[test]
fn effective_requires_a_real_review_reason_without_mutating_original_input() {
    let original = pending();
    for reason in ["".into(), "  ".into(), "长".repeat(501)] {
        let mut draft = original.clone();
        let mut command = effective_command(1, normalized(), result());
        command.reason = reason;
        assert!(draft.mark_effective("submission-1", command, Instant::now()).is_err());
        assert_eq!(snapshot(&draft), snapshot(&original));
    }
    let mut draft = original.clone();
    let mut command = effective_command(1, normalized(), result());
    command.draft_id = "other-draft".into();
    assert!(draft.mark_effective("submission-1", command, Instant::now()).is_err());
    assert_eq!(snapshot(&draft), snapshot(&original));
}

#[test]
fn incomplete_foreign_reused_or_listed_new_sku_results_cannot_mark_effective() {
    let original = pending();
    let mut cases = Vec::new();
    let mut changed = result();
    changed.skus.clear();
    cases.push(changed);
    let mut changed = result();
    changed.skus[0].row_id = "foreign".into();
    cases.push(changed);
    let mut changed = result();
    changed.skus[0].listing_status = ListingStatus::Listed;
    cases.push(changed);
    let mut changed = result();
    changed.skus[0].offering_id = None;
    cases.push(changed);
    let mut changed = result();
    changed.skus[0].sku_created = false;
    cases.push(changed);
    for result in cases {
        let mut draft = original.clone();
        assert!(
            draft
                .mark_effective("submission-1", effective_command(1, normalized(), result), Instant::now())
                .is_err()
        );
        assert_eq!(snapshot(&draft), snapshot(&original));
    }
}

#[test]
fn explicit_existing_sku_match_preserves_listing_status_and_identity() {
    let mut mapping = normalized();
    mapping.sku_mappings[0].target_sku = Some(ExistingSkuRef {
        sku_id: "existing-sku".into(),
        version: 3,
        revision_id: "existing-revision".into(),
    });
    let mut result = result();
    result.product_created = false;
    result.skus[0].sku_created = false;
    result.skus[0].sku_id = "existing-sku".into();
    result.skus[0].revision_id = "existing-revision".into();
    result.skus[0].listing_status = ListingStatus::Listed;
    let mut draft = pending();
    draft
        .mark_effective("submission-1", effective_command(1, mapping.clone(), result.clone()), Instant::now())
        .unwrap();
    assert_eq!(draft.result.unwrap().skus[0].listing_status, ListingStatus::Listed);
    let mut other = pending();
    result.skus[0].sku_id = "unmatched-sku".into();
    assert!(
        other.mark_effective("submission-1", effective_command(1, mapping, result), Instant::now()).is_err()
    );
}

#[test]
fn supplier_inputs_reject_internal_result_fields_and_json_numeric_quantities() {
    let mut encoded = serde_json::to_value(input()).unwrap();
    encoded["confirmed_by"] = json!("buyer");
    assert!(serde_json::from_value::<NewProductInput>(encoded).is_err());
    let mut encoded = serde_json::to_value(input()).unwrap();
    encoded["skus"][0]["available_quantity"] = json!(2.5);
    assert!(serde_json::from_value::<NewProductInput>(encoded).is_err());
    let encoded = serde_json::to_value(input()).unwrap();
    assert!(encoded["skus"][0]["available_quantity"].is_null());
}

#[test]
fn stable_status_codes_and_current_submission_corruption_fail_closed() {
    for (status, code) in [
        (DraftStatus::Draft, "draft"),
        (DraftStatus::Pending, "pending"),
        (DraftStatus::Returned, "returned"),
        (DraftStatus::Withdrawn, "withdrawn"),
        (DraftStatus::Effective, "effective"),
    ] {
        assert_eq!(status.as_str(), code);
        assert_eq!(serde_json::to_value(status).unwrap(), json!(code));
    }
    let mut draft = pending();
    draft.task_id = Some("other-task".into());
    assert!(draft.submitted().is_err());
    let mut draft = pending();
    draft.submissions.clear();
    assert!(draft.submitted().is_err());
    let mut draft = pending();
    draft.frozen_input = None;
    assert!(draft.submitted().is_err());
}
