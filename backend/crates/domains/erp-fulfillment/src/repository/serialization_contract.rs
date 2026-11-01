//! 原履约实体 JSON/BSON 数据形态测试；仅在内存运行。

/// BSON 往返断言 helper（六节往返测试共用脚手架）。
#[cfg(test)]
fn assert_bson_roundtrip<T>(entity: &T)
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let roundtrip: T = bson::deserialize_from_document(bson::serialize_to_document(entity).unwrap()).unwrap();
    assert_eq!(&roundtrip, entity);
}

mod purchase_receipt {
    use erp_core::common::time::Instant;

    use super::assert_bson_roundtrip;
    use crate::entity::fulfillment::purchase_receipt::tests::receipt_data;
    use crate::entity::fulfillment::*;

    /// 序列化：状态/质量结果枚举输出稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(serde_json::to_string(&PurchaseReceiptState::Posted).unwrap(), "\"POSTED\"");
        assert_eq!(serde_json::to_string(&QualityResult::Partial).unwrap(), "\"PARTIAL\"");
        assert_eq!(PurchaseReceiptState::Reversed.label(), "已冲正");

        let mut receipt = PurchaseReceipt::new(PurchaseReceiptId::new("receipt-3"), receipt_data()).unwrap();
        receipt.mark_posted(Instant::from_unix_secs(1_700_000_000), "operator-1").unwrap();
        assert_bson_roundtrip(&receipt);
    }
}

mod delivery {
    use erp_core::common::time::Instant;

    use super::assert_bson_roundtrip;
    use crate::entity::fulfillment::delivery::tests::delivery_data;
    use crate::entity::fulfillment::*;

    /// 序列化：枚举稳定代码；实体 BSON 往返（含密文与指纹字段）。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(serde_json::to_string(&DeliveryType::SupplierDirect).unwrap(), "\"SUPPLIER_DIRECT\"");
        assert_eq!(serde_json::to_string(&DeliveryState::Signed).unwrap(), "\"SIGNED\"");
        assert_eq!(DeliveryState::Shipped.label(), "已发货");

        let mut delivery = Delivery::new(DeliveryId::new("delivery-4"), delivery_data()).unwrap();
        delivery.mark_shipped(Instant::from_unix_secs(1_700_000_000)).unwrap();
        assert_bson_roundtrip(&delivery);
    }
}

mod electronic_delivery {
    use super::assert_bson_roundtrip;
    use crate::entity::fulfillment::electronic_delivery::tests::data;
    use crate::entity::fulfillment::*;

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(serde_json::to_string(&FulfillmentResult::PartialSuccess).unwrap(), "\"PARTIAL_SUCCESS\"");
        assert_eq!(serde_json::to_string(&ElectronicDeliveryState::Confirmed).unwrap(), "\"CONFIRMED\"");
        assert_eq!(ElectronicDeliveryState::Confirmed.label(), "已确认");

        let delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-9"), data()).unwrap();
        assert_bson_roundtrip(&delivery);
    }
}

mod service_fulfillment {
    use super::assert_bson_roundtrip;
    use crate::entity::fulfillment::service_fulfillment::tests::data;
    use crate::entity::fulfillment::*;

    /// 序列化：状态枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(serde_json::to_string(&ServiceFulfillmentState::Confirmed).unwrap(), "\"CONFIRMED\"");
        assert_eq!(ServiceFulfillmentState::Reversed.label(), "已冲正");

        let fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-9"), data()).unwrap();
        assert_bson_roundtrip(&fulfillment);
    }
}

mod customer_acceptance {
    use super::assert_bson_roundtrip;
    use crate::entity::fulfillment::customer_acceptance::tests::data;
    use crate::entity::fulfillment::*;

    /// 序列化：结果枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(serde_json::to_string(&AcceptanceResult::ServiceFailed).unwrap(), "\"SERVICE_FAILED\"");
        assert_eq!(serde_json::to_string(&CustomerAcceptanceState::Posted).unwrap(), "\"POSTED\"");
        assert_eq!(AcceptanceResult::Shortage.label(), "短少");

        let mut acceptance = CustomerAcceptance::new(CustomerAcceptanceId::new("a8"), data()).unwrap();
        acceptance.mark_posted().unwrap();
        assert_bson_roundtrip(&acceptance);
    }
}

mod acceptance_fulfillment_allocation {
    use super::assert_bson_roundtrip;
    use crate::entity::fulfillment::acceptance_fulfillment_allocation::tests::apply_data;
    use crate::entity::fulfillment::*;

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&FulfillmentFactType::ElectronicDelivery).unwrap(),
            "\"ELECTRONIC_DELIVERY\""
        );
        assert_eq!(serde_json::to_string(&AllocationAction::Reverse).unwrap(), "\"REVERSE\"");
        assert_eq!(FulfillmentFactType::ServiceFulfillment.label(), "服务履约");

        let allocation = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-5"),
            apply_data(),
        )
        .unwrap();
        assert_bson_roundtrip(&allocation);
    }
}
