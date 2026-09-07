//! 原履约实体 JSON/BSON 数据形态测试；仅在内存运行。

mod purchase_receipt {
    use crate::entity::fulfillment::purchase_receipt::tests::receipt_data;
    use crate::entity::fulfillment::*;
    use erp_core::common::time::Instant;

    /// 序列化：状态/质量结果枚举输出稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&PurchaseReceiptState::Posted).unwrap(),
            "\"POSTED\""
        );
        assert_eq!(
            serde_json::to_string(&QualityResult::Partial).unwrap(),
            "\"PARTIAL\""
        );
        assert_eq!(PurchaseReceiptState::Reversed.label(), "已冲正");

        let mut receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-3"), receipt_data()).unwrap();
        receipt
            .mark_posted(Instant::from_unix_secs(1_700_000_000), "operator-1")
            .unwrap();
        let roundtrip: PurchaseReceipt =
            bson::deserialize_from_document(bson::serialize_to_document(&receipt).unwrap()).unwrap();
        assert_eq!(roundtrip, receipt);
    }
}

mod delivery {
    use crate::entity::fulfillment::delivery::tests::delivery_data;
    use crate::entity::fulfillment::*;
    use erp_core::common::time::Instant;

    /// 序列化：枚举稳定代码；实体 BSON 往返（含密文与指纹字段）。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&DeliveryType::SupplierDirect).unwrap(),
            "\"SUPPLIER_DIRECT\""
        );
        assert_eq!(
            serde_json::to_string(&DeliveryState::Signed).unwrap(),
            "\"SIGNED\""
        );
        assert_eq!(DeliveryState::Shipped.label(), "已发货");

        let mut delivery = Delivery::new(DeliveryId::new("delivery-4"), delivery_data()).unwrap();
        delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        let roundtrip: Delivery =
            bson::deserialize_from_document(bson::serialize_to_document(&delivery).unwrap()).unwrap();
        assert_eq!(roundtrip, delivery);
    }
}

mod electronic_delivery {
    use crate::entity::fulfillment::electronic_delivery::tests::data;
    use crate::entity::fulfillment::*;

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&FulfillmentResult::PartialSuccess).unwrap(),
            "\"PARTIAL_SUCCESS\""
        );
        assert_eq!(
            serde_json::to_string(&ElectronicDeliveryState::Confirmed).unwrap(),
            "\"CONFIRMED\""
        );
        assert_eq!(ElectronicDeliveryState::Confirmed.label(), "已确认");

        let delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-9"), data()).unwrap();
        let roundtrip: ElectronicDelivery =
            bson::deserialize_from_document(bson::serialize_to_document(&delivery).unwrap()).unwrap();
        assert_eq!(roundtrip, delivery);
    }
}

mod service_fulfillment {
    use crate::entity::fulfillment::service_fulfillment::tests::data;
    use crate::entity::fulfillment::*;

    /// 序列化：状态枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&ServiceFulfillmentState::Confirmed).unwrap(),
            "\"CONFIRMED\""
        );
        assert_eq!(ServiceFulfillmentState::Reversed.label(), "已冲正");

        let fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-9"), data()).unwrap();
        let roundtrip: ServiceFulfillment =
            bson::deserialize_from_document(bson::serialize_to_document(&fulfillment).unwrap()).unwrap();
        assert_eq!(roundtrip, fulfillment);
    }
}

mod customer_acceptance {
    use crate::entity::fulfillment::customer_acceptance::tests::data;
    use crate::entity::fulfillment::*;

    /// 序列化：结果枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&AcceptanceResult::ServiceFailed).unwrap(),
            "\"SERVICE_FAILED\""
        );
        assert_eq!(
            serde_json::to_string(&CustomerAcceptanceState::Posted).unwrap(),
            "\"POSTED\""
        );
        assert_eq!(AcceptanceResult::Shortage.label(), "短少");

        let mut acceptance = CustomerAcceptance::new(CustomerAcceptanceId::new("a8"), data()).unwrap();
        acceptance.mark_posted().unwrap();
        let roundtrip: CustomerAcceptance =
            bson::deserialize_from_document(bson::serialize_to_document(&acceptance).unwrap()).unwrap();
        assert_eq!(roundtrip, acceptance);
    }
}

mod acceptance_fulfillment_allocation {
    use crate::entity::fulfillment::acceptance_fulfillment_allocation::tests::apply_data;
    use crate::entity::fulfillment::*;

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&FulfillmentFactType::ElectronicDelivery).unwrap(),
            "\"ELECTRONIC_DELIVERY\""
        );
        assert_eq!(
            serde_json::to_string(&AllocationAction::Reverse).unwrap(),
            "\"REVERSE\""
        );
        assert_eq!(FulfillmentFactType::ServiceFulfillment.label(), "服务履约");

        let allocation = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-5"),
            apply_data(),
        )
        .unwrap();
        let roundtrip: AcceptanceFulfillmentAllocation =
            bson::deserialize_from_document(bson::serialize_to_document(&allocation).unwrap()).unwrap();
        assert_eq!(roundtrip, allocation);
    }
}
