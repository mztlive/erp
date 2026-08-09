//! 供给、首版商业条款与实时可供投影的事务持久化回归测试。

use std::str::FromStr;

use database::{ensure_indexes, SupplierOfferingExt, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SkuId, SupplierAccountId, SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId,
};
use entities::money::{Quantity, Rate, UnitPrice};
use entities::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, PrefillSourceRefs, SupplierOffering,
    SupplierOfferingAvailability, SupplierOfferingAvailabilityData, SupplierOfferingData,
    SupplierOfferingRevision, SupplierOfferingRevisionData,
};
use test_support::{require_mongo, TestDb};

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn initial_offering_facts_are_created_atomically() {
    require_mongo!(async {
        let fixture = TestDb::new("supplier_offering_initial")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let offering_id = SupplierOfferingId::new("offering-1");
        let revision_id = SupplierOfferingRevisionId::new("offering-revision-1");
        let mut offering = SupplierOffering::new(
            offering_id.clone(),
            SupplierOfferingData {
                sku_id: SkuId::new("sku-1"),
                supplier_id: SupplierAccountId::new("supplier-1"),
                supplier_product_code: Some("SPU-001".to_string()),
                supplier_sku_code: "SKU-001".to_string(),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
            },
            "tester",
        )
        .expect("供给构造失败");
        let revision = SupplierOfferingRevision::new(
            revision_id.clone(),
            SupplierOfferingRevisionData {
                supplier_offering_id: offering_id.clone(),
                revision_no: 1,
                dropship_supply_price_gross: UnitPrice::from_str("11.30").unwrap(),
                dropship_supply_price_net: UnitPrice::from_str("9.83").unwrap(),
                bulk_supply_price_gross: UnitPrice::from_str("9.04").unwrap(),
                bulk_supply_price_net: UnitPrice::from_str("7.86").unwrap(),
                input_tax_rate: Rate::from_str("0.13").unwrap(),
                dropship_express: None,
                freight_amount: None,
                service_fee_amount: None,
                bulk_minimum_order_quantity: Quantity::from_str("10").unwrap(),
                supply_region: vec!["全国".to_string()],
                product_capabilities: Vec::new(),
                valid_from: BusinessDate::from_str("2026-08-08").unwrap(),
                valid_to: None,
                prefill_source_refs: PrefillSourceRefs::default(),
            },
        )
        .expect("供给修订构造失败");
        let availability = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new("availability-1"),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: offering_id.clone(),
                availability_status: AvailabilityStatus::Available,
                available_quantity: Some(Quantity::from_str("20").unwrap()),
                source_updated_at: Instant::now(),
                received_at: Instant::now(),
                source_revision_token: Some("v1".to_string()),
                updated_by: "tester".to_string(),
            },
        )
        .expect("可供投影构造失败");
        offering.stable.current_revision_id = Some(revision_id.to_string());

        let db = fixture.db().clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_offering_repository()
                        .create_with_revision_and_availability(&offering, &revision, &availability, session)
                        .await
                })
            })
            .await
            .expect("首次创建事务失败");

        let stored = fixture
            .db()
            .supplier_offerings()
            .find_by_id("offering-1", &mut database::NoTransaction)
            .await
            .expect("供给查询失败")
            .expect("供给未写入");
        let availability = fixture
            .db()
            .supplier_offering_availabilities()
            .find_by_offering_id(&offering_id, &mut database::NoTransaction)
            .await
            .expect("可供投影查询失败")
            .expect("可供投影未写入");

        assert_eq!(
            stored.stable.current_revision_id.as_deref(),
            Some("offering-revision-1")
        );
        assert_eq!(stored.supplier_sku_code, "SKU-001");
        assert!(availability.is_available());
    });
}
