//! Entity BSON round-trip contracts moved out of inventory entity modules.

use crate::entity::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationEntryType, ReservationStatus,
    StockAdjustment, StockAdjustmentData, StockBalance, StockBalanceData, StockMovement, StockMovementData,
    StockReservation, StockReservationData, StockReservationSourceType,
};
use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{
    PurchaseLineSalesAllocationId, PurchaseReceiptLineId, SalesOrderLineId, SkuId, StockAdjustmentId,
    StockBalanceId, StockMovementId, StockReservationId, WarehouseId,
};
use erp_core::money::Quantity;
use std::str::FromStr;

fn quantity(value: &str) -> Quantity {
    Quantity::from_str(value).unwrap()
}

#[test]
fn bson_roundtrip_preserves_stock_balance() {
    let balance = StockBalance::new(
        StockBalanceId::new("b-6"),
        StockBalanceData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            on_hand_quantity: quantity("10"),
            reserved_quantity: quantity("2"),
            available_quantity: quantity("8"),
            last_movement_id: None,
        },
    )
    .unwrap();
    let roundtrip: StockBalance =
        mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&balance).unwrap())
            .unwrap();
    assert_eq!(roundtrip, balance);
}

#[test]
fn bson_roundtrip_preserves_stock_movement() {
    let movement = StockMovement::new(
        StockMovementId::new("m-8"),
        StockMovementData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            movement_type: MovementType::StockGain,
            direction: MovementDirection::Increase,
            quantity: quantity("1"),
            source_document_id: "adj-1".to_string(),
            source_line_id: None,
            reversal_of_movement_id: None,
            fact_no: "fact-1".to_string(),
            occurred_at: Instant::from_unix_secs(1),
            recorded_at: Instant::from_unix_secs(1),
            recorded_by: "u-1".to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )
    .unwrap();
    let roundtrip: StockMovement =
        mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&movement).unwrap())
            .unwrap();
    assert_eq!(roundtrip, movement);
}

#[test]
fn bson_roundtrip_preserves_stock_adjustment() {
    let mut adjustment = StockAdjustment::new(
        StockAdjustmentId::new("a6"),
        StockAdjustmentData {
            adjustment_no: "ADJ-1".to_string(),
            warehouse_id: WarehouseId::new("wh-1"),
            reason_type: AdjustmentReasonType::StockGain,
            prepared_by: "operator-1".to_string(),
            note: None,
            occurred_at: None,
        },
        "creator-1",
    )
    .unwrap();
    adjustment.submit_for_warehouse_review("reviewer-1").unwrap();
    let roundtrip: StockAdjustment =
        mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&adjustment).unwrap())
            .unwrap();
    assert_eq!(roundtrip, adjustment);
}

#[test]
fn bson_roundtrip_preserves_stock_reservation() {
    let reservation = StockReservation::new(
        StockReservationId::new("rsv-10"),
        StockReservationData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            source_type: StockReservationSourceType::PurchaseReceipt,
            purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("pla-1")),
            source_receipt_line_id: Some(PurchaseReceiptLineId::new("receipt-line-1")),
            source_allocation_id: None,
            reserved_quantity: quantity("3"),
            consumed_quantity: quantity("0"),
            released_quantity: quantity("0"),
            status: ReservationStatus::Active,
        },
    )
    .unwrap();
    let roundtrip: StockReservation =
        mongodb::bson::deserialize_from_document(mongodb::bson::serialize_to_document(&reservation).unwrap())
            .unwrap();
    assert_eq!(roundtrip, reservation);
}

#[test]
fn legacy_missing_created_by_defaults_empty() {
    let adjustment = StockAdjustment::new(
        StockAdjustmentId::new("creator-adj"),
        StockAdjustmentData {
            adjustment_no: "ADJ-1".to_string(),
            warehouse_id: WarehouseId::new("wh-1"),
            reason_type: AdjustmentReasonType::Damage,
            prepared_by: "operator-1".to_string(),
            note: None,
            occurred_at: None,
        },
        "creator-1",
    )
    .unwrap();
    let mut legacy = mongodb::bson::serialize_to_document(&adjustment).unwrap();
    legacy.remove("created_by");
    let legacy: StockAdjustment = mongodb::bson::deserialize_from_document(legacy).unwrap();
    assert!(legacy.created_by.is_empty());
    let _ = ReservationEntryType::Release;
}
