use std::collections::BTreeMap;
use std::num::NonZeroU32;

use serde_json::from_str;

use super::super::order_payload::{
    Consignee, ExactNumber, JdVopInvoiceProfile, PaymentInfo, SubmitLine, SubmitOrder,
};
use super::*;

fn context() -> JdVopRequestContext {
    JdVopRequestContext::new(3).unwrap()
}
fn payload(request: JdVopReadRequest) -> Value {
    let value = from_str(read_payload(&request, &context()).unwrap().get()).unwrap();
    assert_eq!(business_fields(&value, 3), 1);
    value
}

#[test]
fn catalog_and_address_match_verified_gateway_parameters() {
    let cases = [
        (JdVopReadRequest::Pools, json!({"getSkuPoolInfoGoodsReq":{"sourceType":3}})),
        (
            JdVopReadRequest::PoolPage {
                pool_id: "pool".into(),
                offset: "0".into(),
                page_size: NonZeroU32::new(20).unwrap(),
            },
            json!({"bizPoolId":"pool","offset":0,"pageSize":20,"sourceType":3}),
        ),
        (
            JdVopReadRequest::Detail { sku_id: "123".into(), query_ext_set: vec![1, 13] },
            json!({"skuId":123,"queryExtSet":[1,13],"sourceType":3}),
        ),
        (
            JdVopReadRequest::Price { sku_id: "123".into(), quantity: "17.0000".into() },
            json!({"sku":"123","num":"17","sourceType":3}),
        ),
        (JdVopReadRequest::Images { sku_id: "123".into() }, json!({"skuId":[123],"sourceType":3})),
        (
            JdVopReadRequest::Categories { category_ids: vec!["123".into(), "456".into()] },
            json!({"categoryId":[123,456],"sourceType":3}),
        ),
        (JdVopReadRequest::Areas { level: 1, parent_code: None }, json!({"areaLevel":1,"sourceType":3})),
        (
            JdVopReadRequest::Areas { level: 2, parent_code: Some("1".into()) },
            json!({"areaLevel":2,"jdAreaId":1,"sourceType":3}),
        ),
        (
            JdVopReadRequest::VerifyAddress {
                province: "1".into(),
                city: "2".into(),
                county: "3".into(),
                town: "0".into(),
            },
            json!({"provinceId":1,"cityId":2,"countyId":3,"townId":0,"sourceType":3}),
        ),
    ];
    for (request, expected) in cases {
        assert_eq!(payload(request), expected);
    }
}

#[test]
fn order_stock_freight_and_target_queries_preserve_scope() {
    let lines = vec![("123".into(), 17)];
    let address = ["1".into(), "2".into(), "3".into(), "0".into()];
    for request in [
        JdVopReadRequest::Stock { lines: lines.clone(), area: address.clone() },
        JdVopReadRequest::SaleEligibility { lines: lines.clone(), area: address.clone() },
    ] {
        let value = payload(request);
        assert_eq!(value["getStockByIdGoodsReq"]["skuNumInfoList"], json!([{"skuId":123,"skuNumber":17}]));
        assert_eq!(value["getStockByIdGoodsReq"]["areaInfo"]["townId"], 0);
        assert_eq!(value["sourceType"], 3);
    }

    let value = payload(JdVopReadRequest::Freight { lines, area: address, payment_type: 4 });
    assert_eq!(value["freightQueryOpenReq"]["skuInfoList"], json!([{"skuId":123,"skuNum":17}]));
    assert_eq!(value["freightQueryOpenReq"]["paymentType"], 4);

    let order = payload(JdVopReadRequest::Order {
        merchant_order_no: Some("original".into()),
        external_order_no: None,
    });
    assert_eq!(order["thirdOrderId"], "original");
    assert!(order.get("jdOrderId").is_none());
    assert_eq!(order["queryExt"], "qcjbylx,qyfwsp,kmjhyy,medicine");

    assert_eq!(
        payload(JdVopReadRequest::Logistics { external_order_no: "123".into() }),
        json!({"orderLogisticsBillOpenReq":{"jdOrderId":123,"sourceType":3}})
    );
    for request in [
        JdVopReadRequest::DeliveryInfo { external_order_no: "123".into(), source_type: 3 },
        JdVopReadRequest::CancelProgress { external_order_no: "123".into() },
    ] {
        assert_eq!(payload(request), json!({"jdOrderId":123,"sourceType":3}));
    }
    assert_eq!(
        payload(JdVopReadRequest::CancellationEligibility {
            external_order_nos: vec!["123".into(), "9007199254740993".into()]
        }),
        json!({"jdOrderId":"123,9007199254740993","sourceType":3})
    );
}

#[test]
fn funds_and_messages_keep_top_level_filters_and_original_write_targets() {
    for request in [JdVopReadRequest::Balance, JdVopReadRequest::FundsBalance { source_type: 3 }] {
        assert_eq!(payload(request)["checkAccountBalanceOpenReq"]["sourceType"], 3);
    }
    let value = payload(JdVopReadRequest::BalanceChanges {
        source_type: 3,
        start_date: "start".into(),
        end_date: "end".into(),
        page_size: NonZeroU32::new(100).unwrap(),
        page_index: 2,
    });
    assert_eq!(
        value,
        json!({"startDate":"start","endDate":"end","pageSize":100,"pageIndex":2,"sourceType":3})
    );

    let value = payload(JdVopReadRequest::Messages { types: vec![1, 5, 12], read_type: 2, source_type: 3 });
    assert_eq!(value, json!({"type":"1,5,12","readType":2,"sourceType":3}));

    for (request, reason) in [
        (
            JdVopWriteRequest::ConfirmReceipt {
                merchant_order_no: "original".into(),
                external_order_no: "123".into(),
                source_type: 3,
            },
            None,
        ),
        (
            JdVopWriteRequest::Confirm {
                merchant_order_no: "original".into(),
                external_order_no: "123".into(),
                source_type: 3,
            },
            None,
        ),
        (
            JdVopWriteRequest::Cancel {
                merchant_order_no: "original".into(),
                external_order_no: "123".into(),
                reason: "reason".into(),
                source_type: 3,
            },
            Some("reason"),
        ),
    ] {
        let value: Value = from_str(write_payload(&request, &context()).unwrap().get()).unwrap();
        let mut expected = json!({"jdOrderId":123,"thirdOrderId":"original","sourceType":3});
        if let Some(reason) = reason {
            expected["cancelReason"] = json!(reason);
        }
        assert_eq!(value, expected);
    }

    let value: Value = from_str(
        write_payload(
            &JdVopWriteRequest::DeleteMessages {
                ids: vec!["123".into(), "9007199254740993".into()],
                source_type: 3,
            },
            &context(),
        )
        .unwrap()
        .get(),
    )
    .unwrap();
    assert_eq!(value, json!({"id":[123,9007199254740993u64],"sourceType":3}));
}

fn submit_order() -> SubmitOrder {
    let invoice = JdVopInvoiceProfile {
        invoice_type: 23,
        invoice_put_type: 2,
        invoice_selected_title: 4,
        invoice_content_type: 1,
        invoice_company_name: "company".into(),
        invoice_reg_code: None,
        invoice_reg_company_name: None,
        invoice_reg_address: None,
        invoice_reg_phone: None,
        invoice_reg_bank: None,
        invoice_reg_bank_account: None,
        recipient: None,
    };
    SubmitOrder {
        third_order_id: "original".into(),
        submit_state_type: 0,
        sku_info_list: vec![SubmitLine {
            sku_id: 123,
            sku_num: 1,
            sku_unit_price: ExactNumber("123456789012345.123456".into()),
            sku_need_gift: false,
        }],
        payment_info: PaymentInfo { payment_type: 4 },
        consignee_info: Consignee {
            consignee_name: "name".into(),
            consignee_mobile: "phone".into(),
            consignee_address: "address".into(),
            consignee_province_id: 1,
            consignee_city_id: 2,
            consignee_county_id: 3,
            consignee_town_id: 0,
        },
        invoice_info: invoice,
        naked_price_order: false,
    }
}

#[test]
fn submit_preserves_exact_decimal_lexeme_and_context_is_outside_order() {
    let encoded = write_payload(
        &JdVopWriteRequest::Submit { order: Box::new(submit_order()), source_type: 3 },
        &context(),
    )
    .unwrap();
    assert!(encoded.get().contains("\"skuUnitPrice\":123456789012345.123456"));
    let fields: BTreeMap<String, Box<RawValue>> = from_str(encoded.get()).unwrap();
    assert_eq!(fields["sourceType"].get(), "3");
    assert!(!fields["submitOrderOpenReq"].get().contains("sourceType"));
}

#[test]
fn invalid_identifiers_quantities_channels_and_batches_stop_encoding() {
    for request in [
        JdVopReadRequest::Order { merchant_order_no: None, external_order_no: None },
        JdVopReadRequest::Price { sku_id: "123".into(), quantity: "0.5".into() },
        JdVopReadRequest::DeliveryInfo { external_order_no: "123".into(), source_type: 4 },
        JdVopReadRequest::CancellationEligibility { external_order_nos: vec!["1".into(); 21] },
        JdVopReadRequest::VerifyAddress {
            province: "0".into(),
            city: "2".into(),
            county: "3".into(),
            town: "0".into(),
        },
    ] {
        assert!(read_payload(&request, &context()).is_err());
    }
    for request in [
        JdVopReadRequest::Messages { types: vec![1], read_type: 0, source_type: 3 },
        JdVopReadRequest::Messages { types: vec![], read_type: 2, source_type: 3 },
        JdVopReadRequest::Messages { types: vec![2; 101], read_type: 2, source_type: 3 },
        JdVopReadRequest::CancellationEligibility { external_order_nos: vec![] },
        JdVopReadRequest::CancellationEligibility { external_order_nos: vec!["123,456".into()] },
        JdVopReadRequest::Areas { level: 2, parent_code: None },
        JdVopReadRequest::Price { sku_id: "123".into(), quantity: "2147483648".into() },
        JdVopReadRequest::Detail { sku_id: "123".into(), query_ext_set: vec![u32::MAX] },
        JdVopReadRequest::BalanceChanges {
            source_type: 3,
            start_date: "start".into(),
            end_date: "end".into(),
            page_size: NonZeroU32::new(1).unwrap(),
            page_index: u32::MAX,
        },
    ] {
        assert!(read_payload(&request, &context()).is_err());
    }

    assert!(JdVopRequestContext::new(0).is_err());
    assert!(JdVopRequestContext::new(u32::MAX).is_err());
    assert!(JdVopRequestContext::new(u32::try_from(i32::MAX).unwrap()).is_ok());
}

fn business_fields(value: &Value, source: u32) -> usize {
    match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(name, value)| {
                assert!(!matches!(name.as_str(), "appKey" | "pin" | "remoteIp"));
                let source_field = usize::from(name == "sourceType");
                if source_field == 1 {
                    assert_eq!(value, &json!(source));
                }
                source_field + business_fields(value, source)
            })
            .sum(),
        Value::Array(items) => items.iter().map(|value| business_fields(value, source)).sum(),
        _ => 0,
    }
}

fn supported_reads(source_type: u32) -> Vec<JdVopReadRequest> {
    let lines = vec![("123".into(), 17)];
    let area = ["1".into(), "2".into(), "3".into(), "0".into()];
    vec![
        JdVopReadRequest::Pools,
        JdVopReadRequest::PoolPage {
            pool_id: "pool".into(),
            offset: "0".into(),
            page_size: NonZeroU32::new(20).unwrap(),
        },
        JdVopReadRequest::Detail { sku_id: "123".into(), query_ext_set: vec![13] },
        JdVopReadRequest::Price { sku_id: "123".into(), quantity: "17".into() },
        JdVopReadRequest::Images { sku_id: "123".into() },
        JdVopReadRequest::Categories { category_ids: vec!["123".into()] },
        JdVopReadRequest::Areas { level: 1, parent_code: None },
        JdVopReadRequest::VerifyAddress {
            province: "1".into(),
            city: "2".into(),
            county: "3".into(),
            town: "0".into(),
        },
        JdVopReadRequest::Order {
            merchant_order_no: Some("original".into()),
            external_order_no: Some("123".into()),
        },
        JdVopReadRequest::Logistics { external_order_no: "123".into() },
        JdVopReadRequest::DeliveryInfo { external_order_no: "123".into(), source_type },
        JdVopReadRequest::CancellationEligibility { external_order_nos: vec!["123".into()] },
        JdVopReadRequest::CancelProgress { external_order_no: "123".into() },
        JdVopReadRequest::SaleEligibility { lines: lines.clone(), area: area.clone() },
        JdVopReadRequest::Stock { lines: lines.clone(), area: area.clone() },
        JdVopReadRequest::Freight { lines, area, payment_type: 4 },
        JdVopReadRequest::Balance,
        JdVopReadRequest::FundsBalance { source_type },
        JdVopReadRequest::BalanceChanges {
            source_type,
            start_date: "start".into(),
            end_date: "end".into(),
            page_size: NonZeroU32::new(20).unwrap(),
            page_index: 1,
        },
        JdVopReadRequest::Messages { types: vec![1, 5, 12], read_type: 2, source_type },
    ]
}

fn supported_writes(source_type: u32) -> Vec<JdVopWriteRequest> {
    vec![
        JdVopWriteRequest::Submit { order: Box::new(submit_order()), source_type },
        JdVopWriteRequest::Confirm {
            merchant_order_no: "original".into(),
            external_order_no: "123".into(),
            source_type,
        },
        JdVopWriteRequest::ConfirmReceipt {
            merchant_order_no: "original".into(),
            external_order_no: "123".into(),
            source_type,
        },
        JdVopWriteRequest::Cancel {
            merchant_order_no: "original".into(),
            external_order_no: "123".into(),
            reason: "reason".into(),
            source_type,
        },
        JdVopWriteRequest::DeleteMessages { ids: vec!["123".into()], source_type },
    ]
}

#[test]
fn every_supported_business_method_omits_system_fields_for_default_and_overridden_source() {
    for source in [3, 7] {
        let context = JdVopRequestContext::new(source).unwrap();
        for request in supported_reads(source) {
            let value: Value = from_str(read_payload(&request, &context).unwrap().get()).unwrap();
            assert_eq!(business_fields(&value, source), 1, "{}", request.method());
        }
        for request in supported_writes(source) {
            let value: Value = from_str(write_payload(&request, &context).unwrap().get()).unwrap();
            assert_eq!(business_fields(&value, source), 1, "{}", request.method());
        }
    }
}

#[test]
fn business_encoding_keeps_long_identifiers_exact() {
    let encoded = read_payload(
        &JdVopReadRequest::Detail { sku_id: "9007199254740993".into(), query_ext_set: vec![13] },
        &context(),
    )
    .unwrap();
    assert!(encoded.get().contains("\"skuId\":9007199254740993"));
}
