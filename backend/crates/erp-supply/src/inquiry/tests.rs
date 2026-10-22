use entity_core::BaseModel;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{deserialize_from_document, serialize_to_document};

use super::*;

fn terms() -> QuoteTerms {
    QuoteTerms {
        unit_cost_gross: "9.90".parse().unwrap(),
        input_tax_rate: "0.13".parse().unwrap(),
        available_quantity: "20".parse().unwrap(),
        address_limits: vec![address("20")],
        expected_delivery_date: "2026-10-22".parse().unwrap(),
        valid_to: Some("2026-11-01".parse().unwrap()),
    }
}
fn address(quantity: &str) -> QuoteAddressLimit {
    QuoteAddressLimit {
        allocation_id: "address-1".into(),
        address_fingerprint: "a".repeat(64),
        quantity: quantity.parse().unwrap(),
    }
}
fn usage(quantity: &str) -> InquiryQuoteUse {
    InquiryQuoteUse {
        inquiry_id: "inquiry-1".into(),
        requirement_version: 1,
        line_id: "line-1".into(),
        sku_id: "sku-1".to_string().into(),
        supplier_id: "supplier-1".to_string().into(),
        quantity: quantity.parse().unwrap(),
        addresses: vec![address(quantity)],
        expected_delivery_date: "2026-10-25".parse().unwrap(),
        on_date: "2026-10-21".parse().unwrap(),
        consumed_quantity: "0".parse().unwrap(),
    }
}
fn quote() -> InquiryQuote {
    InquiryQuote::new(
        "quote-revision-1".into(),
        InquiryQuoteData {
            quote_id: "quote-1".into(),
            revision_no: 1,
            feedback_round: 1,
            scope: InquiryQuoteScope {
                inquiry_id: "inquiry-1".into(),
                requirement_version: 1,
                line_id: "line-1".into(),
            },
            supplier_id: "supplier-1".to_string().into(),
            supplier_offering_id: "offering-1".to_string().into(),
            sku_id: "sku-1".to_string().into(),
            terms: terms(),
            internal_note: "采购议价依据".into(),
            recorded_by: "buyer-1".into(),
        },
    )
    .unwrap()
}
#[test]
fn exact_quote_accepts_original_terms_without_mutating_history() {
    let quote = quote();
    let before = serde_json::to_value(&quote).unwrap();
    quote.ensure_applicable(&usage("10")).unwrap();
    assert_eq!(serde_json::to_value(&quote).unwrap(), before);
}
#[test]
fn scope_supplier_sku_and_version_cannot_be_reused() {
    let quote = quote();
    for field in 0..5 {
        let mut value = usage("10");
        match field {
            0 => value.inquiry_id = "other".into(),
            1 => value.requirement_version = 2,
            2 => value.line_id = "other".into(),
            3 => value.sku_id = "other".to_string().into(),
            _ => value.supplier_id = "other".to_string().into(),
        }
        assert!(quote.ensure_applicable(&value).is_err());
    }
}
#[test]
fn expired_quote_changed_address_and_early_delivery_are_rejected() {
    let quote = quote();
    let mut expired = usage("10");
    expired.on_date = "2026-11-01".parse().unwrap();
    assert!(quote.ensure_applicable(&expired).is_err());
    let mut changed = usage("10");
    changed.addresses[0].address_fingerprint = "b".repeat(64);
    assert!(quote.ensure_applicable(&changed).is_err());
    quote.ensure_applicable(&usage("5")).unwrap();
    let mut early = usage("10");
    early.expected_delivery_date = "2026-10-21".parse().unwrap();
    assert!(quote.ensure_applicable(&early).is_err());
}
#[test]
fn quote_without_expiry_allows_partial_adoption_but_rejects_overbooking() {
    let mut quote = quote();
    quote.data.terms.valid_to = None;
    let mut value = usage("5");
    value.on_date = "2030-01-01".parse().unwrap();
    quote.ensure_applicable(&value).unwrap();
    value.consumed_quantity = "16".parse().unwrap();
    assert!(quote.ensure_applicable(&value).is_err());
}
#[test]
fn simplified_terms_reject_invalid_price_tax_quantity_and_retired_fields() {
    let value = serde_json::to_value(terms()).unwrap();
    for (key, invalid) in [("unit_cost_gross", "0"), ("available_quantity", "0"), ("input_tax_rate", "1.1")] {
        let mut data = value.clone();
        data[key] = invalid.into();
        let terms: QuoteTerms = serde_json::from_value(data).unwrap();
        assert!(terms.validated().is_err());
    }
    for key in ["minimum_order_quantity", "valid_from", "payment_term_code", "allow_cumulative_batches"] {
        let mut data = value.clone();
        data[key] = "legacy".into();
        assert!(serde_json::from_value::<QuoteTerms>(data).is_err());
    }
    let mut data = value;
    data.as_object_mut().unwrap().remove("valid_to");
    let terms: QuoteTerms = serde_json::from_value(data).unwrap();
    assert!(terms.validated().unwrap().valid_to.is_none());
}
#[test]
fn aggregate_supply_and_address_quantity_are_conserved() {
    let quote = quote();
    let mut value = usage("10");
    value.consumed_quantity = "11".parse().unwrap();
    assert!(quote.ensure_applicable(&value).is_err());
    let mut value = usage("10");
    value.addresses[0].quantity = "9".parse().unwrap();
    assert!(quote.ensure_applicable(&value).is_err());
    let mut data = quote.data.clone();
    data.terms.address_limits.push(address("1"));
    assert!(InquiryQuote::new("new".into(), data).is_err());
}
#[test]
fn internal_fee_basis_requires_precise_scope_and_payment_identity() {
    let data = InquiryFeeBasisData {
        inquiry_id: "inquiry-1".into(),
        requirement_version: 1,
        feedback_round: 2,
        fee_id: "fee-1".into(),
        payee_supplier_id: "supplier-1".to_string().into(),
        payment_term_code: "POSTPAY_NET30".into(),
        internal_note: "内部收款依据".into(),
        attachment_ids: vec![],
        recorded_by: "buyer-1".into(),
    };
    let basis = InquiryFeeBasis::new("basis".into(), data.clone()).unwrap();
    let mut document = serialize_to_document(&basis).unwrap();
    document.insert("_id", ObjectId::new());
    let restored: InquiryFeeBasis = deserialize_from_document(document).unwrap();
    assert_eq!(serde_json::to_value(&restored).unwrap(), serde_json::to_value(&basis).unwrap());
    let mut invalid = data.clone();
    invalid.feedback_round = 0;
    assert!(InquiryFeeBasis::new("basis".into(), invalid).is_err());
    let mut invalid = data;
    invalid.payee_supplier_id = " ".to_string().into();
    assert!(InquiryFeeBasis::new("basis".into(), invalid).is_err());
}

#[test]
fn stable_quote_balance_prevents_cross_sales_and_cross_revision_reset() {
    let mut quote = quote();
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    balance.reserve(&quote, &usage("10")).unwrap();
    assert_eq!(balance.reserved_quantity.to_string(), "10");
    quote.data.revision_no = 2;
    let before = serde_json::to_value(&balance).unwrap();
    assert!(balance.reserve(&quote, &usage("15")).is_err());
    assert_eq!(serde_json::to_value(&balance).unwrap(), before);
    balance.reserve(&quote, &usage("10")).unwrap();
    assert_eq!(balance.reserved_quantity.to_string(), "20");
    assert!(balance.reserve(&quote, &usage("10")).is_err());
}

#[test]
fn formal_void_releases_exact_quantity_and_preserves_history_payload() {
    let quote = quote();
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    balance.reserve(&quote, &usage("10")).unwrap();
    let reservation = InquiryQuoteReservation {
        base: BaseModel::new("po:line".into()),
        purchase_order_id: "po".into(),
        released: false,
        quote_id: quote.data.quote_id.clone(),
        quote_revision_id: quote.base.id.clone(),
        quantity: "10".parse().unwrap(),
        addresses: vec![address("10")],
        history: vec![],
    };
    balance.release(&reservation).unwrap();
    assert_eq!(balance.reserved_quantity.to_string(), "0");
    assert!(balance.address_quantities.is_empty());
    assert_eq!(reservation.quantity.to_string(), "10");
    assert!(balance.release(&reservation).is_err());
}

#[test]
fn internal_quote_roundtrips_flattened_persistence_and_decimal_strings() {
    let quote = quote();
    let value = serde_json::to_value(&quote).unwrap();
    let restored: InquiryQuote = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), value);
    assert!(value["terms"]["unit_cost_gross"].is_string());
}

#[test]
fn pending_change_does_not_release_original_supply_and_effective_change_resizes_it() {
    let quote = quote();
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    let first = usage("10");
    balance.reserve(&quote, &first).unwrap();
    let original = InquiryQuoteReservation {
        base: BaseModel::new("po:line".into()),
        purchase_order_id: "po".into(),
        released: false,
        quote_id: quote.data.quote_id.clone(),
        quote_revision_id: quote.base.id.clone(),
        quantity: first.quantity,
        addresses: first.addresses.clone(),
        history: vec![],
    };
    let pending = balance.resized(&quote, &usage("5"), &original, true).unwrap();
    assert_eq!(balance.reserved_quantity.to_string(), "10");
    assert_eq!(pending.quantity.to_string(), "10");
    let effective = balance.resized(&quote, &usage("5"), &pending, false).unwrap();
    assert_eq!(balance.reserved_quantity.to_string(), "5");
    assert_eq!(effective.quantity.to_string(), "5");
    assert_eq!(effective.history.len(), 1);
    assert_eq!(effective.history[0].quantity.to_string(), "10");
}

#[test]
fn invalid_change_is_atomic_in_memory_and_cannot_overbook_shared_supplier_quote() {
    let quote = quote();
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    let first = usage("10");
    balance.reserve(&quote, &first).unwrap();
    let original = InquiryQuoteReservation {
        base: BaseModel::new("po:line".into()),
        purchase_order_id: "po".into(),
        released: false,
        quote_id: quote.data.quote_id.clone(),
        quote_revision_id: quote.base.id.clone(),
        quantity: first.quantity,
        addresses: first.addresses.clone(),
        history: vec![],
    };
    let before = serde_json::to_value(&balance).unwrap();
    assert!(balance.resized(&quote, &usage("25"), &original, true).is_err());
    assert_eq!(serde_json::to_value(&balance).unwrap(), before);
}

#[test]
fn expired_quote_can_restore_only_existing_formal_exposure_without_new_eligibility() {
    let mut quote = quote();
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    let initial = usage("10");
    balance.reserve(&quote, &initial).unwrap();
    let original = InquiryQuoteReservation {
        base: BaseModel::new("po:line".into()),
        purchase_order_id: "po".into(),
        released: false,
        quote_id: quote.data.quote_id.clone(),
        quote_revision_id: quote.base.id.clone(),
        quantity: initial.quantity,
        addresses: initial.addresses.clone(),
        history: vec![],
    };
    let held = balance.resized(&quote, &usage("15"), &original, true).unwrap();
    quote.data.terms.valid_to = Some("2026-10-21".parse().unwrap());
    assert!(quote.ensure_applicable(&initial).is_err());
    let restored = balance.restore_existing_usage(&held, initial.quantity, &initial.addresses).unwrap();
    assert_eq!(restored.quantity.to_string(), "10");
    assert_eq!(balance.reserved_quantity.to_string(), "10");
    assert_eq!(restored.history.last().unwrap().quantity.to_string(), "15");
}

#[test]
fn restoration_rejects_increase_new_address_and_other_quote_without_mutating_balance() {
    let quote = quote();
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    let initial = usage("10");
    balance.reserve(&quote, &initial).unwrap();
    let original = InquiryQuoteReservation {
        base: BaseModel::new("po:line".into()),
        purchase_order_id: "po".into(),
        released: false,
        quote_id: quote.data.quote_id.clone(),
        quote_revision_id: quote.base.id.clone(),
        quantity: initial.quantity,
        addresses: initial.addresses.clone(),
        history: vec![],
    };
    let before = serde_json::to_value(&balance).unwrap();
    assert!(balance.restore_existing_usage(&original, "11".parse().unwrap(), &[address("11")]).is_err());
    let mut changed = address("10");
    changed.address_fingerprint = "b".repeat(64);
    assert!(balance.restore_existing_usage(&original, initial.quantity, &[changed]).is_err());
    let mut other = original.clone();
    other.quote_id = "other-quote".into();
    assert!(balance.restore_existing_usage(&other, initial.quantity, &initial.addresses).is_err());
    assert_eq!(serde_json::to_value(&balance).unwrap(), before);
}

#[test]
fn partial_supply_covers_all_demand_addresses_without_expanding_total_balance() {
    let mut data = quote().data;
    data.terms.available_quantity = "5".parse().unwrap();
    let mut second = address("10");
    second.allocation_id = "address-2".into();
    data.terms.address_limits = vec![address("10"), second.clone()];
    let quote = InquiryQuote::new("partial".into(), data).unwrap();
    let mut value = usage("5");
    second.quantity = "5".parse().unwrap();
    value.addresses = vec![second];
    let mut balance = InquiryQuoteBalance::empty(quote.data.quote_id.clone()).unwrap();
    balance.reserve(&quote, &value).unwrap();
    assert!(balance.reserve(&quote, &usage("1")).is_err());
    let mut invalid = quote.data.clone();
    invalid.terms.address_limits = vec![address("4")];
    assert!(InquiryQuote::new("invalid".into(), invalid).is_err());
    value.addresses[0].quantity = "4".parse().unwrap();
    assert!(quote.ensure_applicable(&value).is_err());
}

#[test]
fn saved_quote_documents_can_be_read_with_mongo_metadata() {
    let original = quote();
    for with_mongo_id in [false, true] {
        let mut document = serialize_to_document(&original).unwrap();
        if with_mongo_id {
            document.insert("_id", ObjectId::new());
        }
        let restored: InquiryQuote = deserialize_from_document(document.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), serde_json::to_value(&original).unwrap());
        document.get_document_mut("terms").unwrap().insert("input_tax_rate", "invalid");
        assert!(deserialize_from_document::<InquiryQuote>(document).is_err());
    }
}
