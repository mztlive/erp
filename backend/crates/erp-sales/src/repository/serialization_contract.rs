//! 销售持久化 BSON 往返合同，保留原实体测试的样例和断言。

mod sales_order_entity {
    use crate::entity::sales_order::{
        BusinessType, OriginSystem, SalesOrder, SalesOrderData, SalesOrderLine, SalesOrderLineData,
    };
    use erp_core::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId};

    fn data() -> SalesOrderData {
        SalesOrderData {
            sales_owner_user_id: "admin-1".to_string(),
            business_org_unit_id: "org-sales".to_string(),
            order_no: " SO-2026-0001 ".to_string(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(ContractId::new("contract-1")),
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        }
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let order = SalesOrder::new(SalesOrderId::new("o-1"), data(), "admin-1").unwrap();
        let roundtrip: SalesOrder =
            bson::deserialize_from_document(bson::serialize_to_document(&order).unwrap()).unwrap();
        assert_eq!(roundtrip, order);
        let mut missing_owner = bson::serialize_to_document(&order).unwrap();
        missing_owner.remove("sales_owner_user_id");
        assert!(bson::deserialize_from_document::<SalesOrder>(missing_owner).is_err());

        let line = SalesOrderLine::new(
            SalesOrderLineId::new("l-1"),
            SalesOrderId::new("o-1"),
            SalesOrderLineData { line_no: 1 },
        )
        .unwrap();
        let roundtrip_line: SalesOrderLine =
            bson::deserialize_from_document(bson::serialize_to_document(&line).unwrap()).unwrap();
        assert_eq!(roundtrip_line, line);
    }
}

mod sales_order_revision {
    use crate::entity::sales_order::{
        CardForm, RevisionSource, SalesOrderRevision, SalesOrderRevisionData, SalesOrderVoucherLineRevision,
        SalesOrderVoucherLineRevisionData,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ContractRevisionId, PartyRevisionId, SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId,
        SalesOrderVoucherLineRevisionId,
    };
    use erp_core::money::{Amount, UnitPrice};
    use std::str::FromStr;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn header_data() -> SalesOrderRevisionData {
        SalesOrderRevisionData {
            sales_order_id: SalesOrderId::new("o-1"),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            previous_revision_id: None,
            content_hash: " abc123def456 ".to_string(),
            customer_revision_id: Some(PartyRevisionId::new("party-rev-1")),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            snapshot: crate::entity::sales_order::snapshot::HeaderSnapshotData {
                customer_name: " 东方企业 ".to_string(),
                contract_no: Some(" HT-2026-0088 ".to_string()),
                settlement_party_name: Some(" 集团结算中心 ".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: " 月结 30 天 ".to_string(),
                invoice_type: " 增值税专用发票 ".to_string(),
                tax_point: " 6 ".to_string(),
            },
            project_name: Some(" 端午福利项目 ".to_string()),
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            effective_at: Instant::from_unix_secs(1_800_000_000),
            recorded_at: Instant::from_unix_secs(1_800_000_100),
        }
    }

    fn voucher_data() -> SalesOrderVoucherLineRevisionData {
        SalesOrderVoucherLineRevisionData {
            revision_line_id: SalesOrderRevisionLineId::new("rl-1"),
            face_value: amt("100.00"),
            card_count: 3,
            unit_price_gross: price("90.0000"),
            card_form: CardForm::Electronic,
        }
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let revision = SalesOrderRevision::new(SalesOrderRevisionId::new("rev-1"), header_data()).unwrap();
        let roundtrip: SalesOrderRevision =
            bson::deserialize_from_document(bson::serialize_to_document(&revision).unwrap()).unwrap();
        assert_eq!(roundtrip, revision);

        let line =
            SalesOrderVoucherLineRevision::new(SalesOrderVoucherLineRevisionId::new("v-1"), voucher_data())
                .unwrap();
        let roundtrip_line: SalesOrderVoucherLineRevision =
            bson::deserialize_from_document(bson::serialize_to_document(&line).unwrap()).unwrap();
        assert_eq!(roundtrip_line, line);
    }
}

mod sales_order_submission {
    use crate::entity::sales_order::{
        BusinessType, GoodsLineFields, LineType, SalesOrderSubmission, SalesOrderSubmissionData,
        SalesOrderSubmissionLineData, WelfareScenario,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ContractRevisionId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId,
        SalesOrderSubmissionId, SalesOrderWorkingCopyId, SkuId, SkuRevisionId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
    use std::str::FromStr;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn goods_line() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            service_region: Some("east".to_string()),
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn line_data(line_no: u32) -> SalesOrderSubmissionLineData {
        SalesOrderSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: " 年货礼盒 ".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: Some(goods_line()),
            voucher: None,
        }
    }

    fn header_data() -> SalesOrderSubmissionData {
        SalesOrderSubmissionData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_no: 1,
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 3,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: crate::entity::sales_order::snapshot::HeaderSnapshotData {
                customer_name: " 东方企业 ".to_string(),
                contract_no: Some(" HT-2026-0088 ".to_string()),
                settlement_party_name: Some(" 集团结算中心 ".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: " 月结 30 天 ".to_string(),
                invoice_type: " 增值税专用发票 ".to_string(),
                tax_point: " 6 ".to_string(),
            },
            project_name: None,
            business_remark: Some(" 按合同执行 ".to_string()),
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            receivable_due_date: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            submitted_at: Instant::from_unix_secs(1_790_000_000),
            submitted_by: " sales-1 ".to_string(),
            lines: vec![line_data(1)],
        }
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let submission =
            SalesOrderSubmission::new(SalesOrderSubmissionId::new("s-1"), header_data()).unwrap();
        let roundtrip: SalesOrderSubmission =
            bson::deserialize_from_document(bson::serialize_to_document(&submission).unwrap()).unwrap();
        assert_eq!(roundtrip, submission);
    }
}

mod sales_change_submission {
    use crate::entity::sales_review::{
        BusinessType, GoodsLineFields, LineType, SalesChangeSubmission, SalesChangeSubmissionData,
        SalesChangeSubmissionLineData, WelfareScenario,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ContractRevisionId, CustomerAccountId, PartyId, SalesChangeOrderId, SalesChangeSubmissionId,
        SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, SalesOrderWorkingCopyId, SkuId, SkuRevisionId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
    use std::str::FromStr;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn goods_line() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            service_region: Some("east".to_string()),
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn line_data(line_no: u32) -> SalesChangeSubmissionLineData {
        SalesChangeSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: " 年货礼盒 ".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: Some(goods_line()),
            voucher: None,
        }
    }

    fn header_data() -> SalesChangeSubmissionData {
        SalesChangeSubmissionData {
            sales_change_order_id: SalesChangeOrderId::new("co-1"),
            submission_no: 1,
            base_revision_id: SalesOrderRevisionId::new("rev-1"),
            sales_order_id: SalesOrderId::new("o-1"),
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 5,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: crate::entity::sales_review::snapshot::HeaderSnapshotData {
                customer_name: " 东方企业 ".to_string(),
                contract_no: Some(" HT-2026-0088 ".to_string()),
                settlement_party_name: Some(" 集团结算中心 ".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: " 月结 30 天 ".to_string(),
                invoice_type: " 增值税专用发票 ".to_string(),
                tax_point: " 6 ".to_string(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            submitted_at: Instant::from_unix_secs(1_795_000_000),
            submitted_by: " sales-1 ".to_string(),
            lines: vec![line_data(1)],
        }
    }

    #[test]
    fn entities_roundtrip_through_bson() {
        let submission =
            SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), header_data()).unwrap();
        let roundtrip: SalesChangeSubmission =
            bson::deserialize_from_document(bson::serialize_to_document(&submission).unwrap()).unwrap();
        assert_eq!(roundtrip, submission);
    }
}
