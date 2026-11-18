//! 首期订单、资金和消息请求；字段名以对应官方 API 文档为准。
use super::*;

// 签名前按方法编码订单、预检、资金和消息读取，核对原订单身份、批次及账号渠道。
// 原单查询同时请求特殊履约标记，供后续判断能否走普通履约。
pub(super) fn read(
    request: &JdVopReadRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    use JdVopReadRequest as R;
    match request {
        R::Order { merchant_order_no, external_order_no } => {
            if merchant_order_no.is_none() && external_order_no.is_none() {
                return Err(mapping("京东原订单查询身份缺失"));
            }
            let mut fields = json!({"queryExt":"qcjbylx,qyfwsp,kmjhyy,medicine"});
            if let Some(id) = merchant_order_no {
                merchant(id)?;
                fields["thirdOrderId"] = json!(id);
            }
            if let Some(id) = external_order_no {
                fields["jdOrderId"] = json!(identifier(id)?);
            }
            flat(fields, context)
        },
        R::Logistics { external_order_no } => {
            nested("orderLogisticsBillOpenReq", json!({"jdOrderId":identifier(external_order_no)?}), context)
        },
        R::DeliveryInfo { external_order_no, source_type } => {
            context.source(*source_type)?;
            flat(json!({"jdOrderId":identifier(external_order_no)?}), context)
        },
        R::CancellationEligibility { external_order_nos } => {
            flat(json!({"jdOrderId":csv_ids(external_order_nos,20)?}), context)
        },
        R::CancelProgress { external_order_no } => {
            flat(json!({"jdOrderId":identifier(external_order_no)?}), context)
        },
        R::Stock { lines, area: address } | R::SaleEligibility { lines, area: address } => envelope(
            "getStockByIdGoodsReq",
            &json!({"skuNumInfoList":lines_value(lines,"skuNumber")?,"areaInfo":area(address)?}),
            Some(context),
        ),
        R::Freight { lines, area: address, payment_type } => {
            if *payment_type != 4 {
                return Err(mapping("京东首期仅支持预存款运费预检"));
            }
            envelope(
                "freightQueryOpenReq",
                &json!({"skuInfoList":lines_value(lines,"skuNum")?,"areaInfo":area(address)?,"paymentType":payment_type}),
                Some(context),
            )
        },
        _ => funds_messages(request, context),
    }
}

fn lines_value(lines: &[(String, u32)], quantity_name: &str) -> ConnectorResult<Vec<Value>> {
    if lines.is_empty() || lines.len() > 100 {
        return Err(mapping("京东请求商品数量须为1至100项"));
    }
    lines
        .iter()
        .map(|(sku, quantity)| {
            if *quantity == 0 || *quantity > u32::try_from(i32::MAX).expect("正int可表示为u32") {
                return Err(mapping("京东商品数量须为正整数"));
            }
            let mut item = json!({"skuId":identifier(sku)?});
            item[quantity_name] = json!(quantity);
            Ok(item)
        })
        .collect()
}

fn funds_messages(
    request: &JdVopReadRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    use JdVopReadRequest as R;
    match request {
        R::Balance => nested("checkAccountBalanceOpenReq", json!({}), context),
        R::FundsBalance { source_type } => {
            context.source(*source_type)?;
            nested("checkAccountBalanceOpenReq", json!({}), context)
        },
        R::BalanceChanges { source_type, start_date, end_date, page_size, page_index } => {
            context.source(*source_type)?;
            if *page_index == 0
                || *page_index > u32::try_from(i32::MAX).expect("int正上限可表示")
                || page_size.get() > u32::try_from(i32::MAX).expect("int正上限可表示")
            {
                return Err(mapping("京东资金查询分页超出接口范围"));
            }
            flat(
                json!({"startDate":start_date,"endDate":end_date,"pageSize":page_size.get(),"pageIndex":page_index}),
                context,
            )
        },
        R::Messages { types, read_type, source_type } => {
            context.source(*source_type)?;
            if types.is_empty() || types.len() > 100 || *read_type != 2 {
                return Err(mapping("京东消息类型批次或读取模式超出首期范围"));
            }
            flat(
                json!({"type":types.iter().map(u32::to_string).collect::<Vec<_>>().join(","),"readType":read_type}),
                context,
            )
        },
        _ => Err(mapping("京东订单读取方法不匹配")),
    }
}

// 发送前校验原写动作的账号渠道、目标和批次，按对应方法组装可直接签名的业务 JSON。
// 例如创建使用请求对象，确认、取消和消息删除用顶层字段；这里不访问网络。
pub(super) fn write(
    request: &JdVopWriteRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    use JdVopWriteRequest as W;
    match request {
        W::Submit { order, source_type } => {
            context.source(*source_type)?;
            merchant(&order.third_order_id)?;
            envelope("submitOrderOpenReq", order.as_ref(), Some(context))
        },
        W::Confirm { merchant_order_no, external_order_no, source_type }
        | W::ConfirmReceipt { merchant_order_no, external_order_no, source_type } => {
            context.source(*source_type)?;
            merchant(merchant_order_no)?;
            flat(
                json!({"thirdOrderId":merchant_order_no,"jdOrderId":identifier(external_order_no)?}),
                context,
            )
        },
        W::Cancel { merchant_order_no, external_order_no, reason, source_type } => {
            context.source(*source_type)?;
            merchant(merchant_order_no)?;
            if reason.chars().count() > 200 {
                return Err(mapping("京东取消原因超过200字"));
            }
            flat(
                json!({"thirdOrderId":merchant_order_no,"jdOrderId":identifier(external_order_no)?,"cancelReason":reason}),
                context,
            )
        },
        W::DeleteMessages { ids: messages, source_type } => {
            context.source(*source_type)?;
            flat(json!({"id":ids(messages,100)?}), context)
        },
    }
}

// 批量校验可取消资格前，将有效的京东单号按网关约定拼成逗号分隔串；转换不经浮点数。
fn csv_ids(values: &[String], maximum: usize) -> ConnectorResult<String> {
    Ok(ids(values, maximum)?.iter().map(u64::to_string).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_payloads_keep_exact_bytes_and_validation_order() {
        let context = JdVopRequestContext::new(3).unwrap();
        for (merchant, external, source, expected) in [
            ("original", "9007199254740993", 3, None),
            ("", "0", 4, Some("京东请求渠道与受控账号不一致")),
            ("", "0", 3, Some("京东原第三方订单号无效")),
            ("original", "0", 3, Some("京东编号超出接口范围")),
            ("original", "9223372036854775808", 3, Some("京东编号超出接口范围")),
        ] {
            for request in [
                JdVopWriteRequest::Confirm {
                    merchant_order_no: merchant.into(),
                    external_order_no: external.into(),
                    source_type: source,
                },
                JdVopWriteRequest::ConfirmReceipt {
                    merchant_order_no: merchant.into(),
                    external_order_no: external.into(),
                    source_type: source,
                },
            ] {
                let result = write_payload(&request, &context);
                if let Some(summary) = expected {
                    assert_eq!(result.unwrap_err(), mapping(summary));
                } else {
                    assert_eq!(
                        result.unwrap().get(),
                        r#"{"jdOrderId":9007199254740993,"sourceType":3,"thirdOrderId":"original"}"#
                    );
                }
            }
        }
    }
}
