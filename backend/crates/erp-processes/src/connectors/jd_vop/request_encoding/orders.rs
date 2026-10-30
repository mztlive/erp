//! 首期订单、资金和消息请求；字段名以对应官方 API 文档为准。
use super::*;

/// 编码原订单、预检、资金及消息读取。
/// # 参数
/// 类型化读取意图及受信账号上下文。
/// # 返回
/// 含准确字段层级的业务 JSON。
/// # 错误
/// 原身份或数量不符合接口范围时拒绝。
pub(super) fn read(
    request: &JdVopReadRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    use JdVopReadRequest as R;
    if matches!(request, R::Stock { .. } | R::SaleEligibility { .. } | R::Freight { .. }) {
        return inventory(request, context);
    }
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
            nested("queryOrderOpenReq", fields, context)
        },
        R::Logistics { external_order_no } => {
            nested("orderLogisticsBillOpenReq", json!({"jdOrderId":identifier(external_order_no)?}), context)
        },
        R::DeliveryInfo { external_order_no, source_type } => {
            context.source(*source_type)?;
            nested("deliveryInfoQueryOpenReq", json!({"jdOrderId":identifier(external_order_no)?}), context)
        },
        R::CancellationEligibility { external_order_nos } => {
            nested("CheckCancelOrderOpenReq", json!({"jdOrderIdSet":ids(external_order_nos,20)?}), context)
        },
        R::CancelProgress { external_order_no } => nested(
            "orderCancelProcessQueryOpenReq",
            json!({"jdOrderId":identifier(external_order_no)?}),
            context,
        ),
        _ => funds_messages(request, context),
    }
}

fn inventory(request: &JdVopReadRequest, context: &JdVopRequestContext) -> ConnectorResult<Box<RawValue>> {
    use JdVopReadRequest as R;
    match request {
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
        _ => Err(mapping("京东预检读取方法不匹配")),
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
            nested(
                "checkBalanceChangeInfoOpenReq",
                json!({"startDate":start_date,"endDate":end_date,"pageSize":page_size.get(),"pageIndex":page_index}),
                context,
            )
        },
        R::Messages { types, read_type, source_type } => {
            context.source(*source_type)?;
            if types.len() > 100 || *read_type != 2 {
                return Err(mapping("京东消息类型批次或读取模式超出首期范围"));
            }
            nested("queryMsgByTenantReq", json!({"typeSet":types,"readType":read_type}), context)
        },
        _ => Err(mapping("京东订单读取方法不匹配")),
    }
}

/// 编码唯一原外部写动作，不执行网络。
/// # 参数
/// 持久化写意图及其受信账号上下文。
/// # 返回
/// 可直接用于签名的业务 JSON。
/// # 错误
/// 原目标、渠道或批次范围无效时拒绝。
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
        W::Confirm { merchant_order_no, external_order_no, source_type } => {
            context.source(*source_type)?;
            merchant(merchant_order_no)?;
            nested(
                "confirmOrderOpenReq",
                json!({"thirdOrderId":merchant_order_no,"jdOrderId":identifier(external_order_no)?}),
                context,
            )
        },
        W::ConfirmReceipt { merchant_order_no, external_order_no, source_type } => {
            context.source(*source_type)?;
            merchant(merchant_order_no)?;
            nested(
                "confirmReceiveOrderOpenReq",
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
            nested(
                "cancelOrderOpenReq",
                json!({"thirdOrderId":merchant_order_no,"jdOrderId":identifier(external_order_no)?,"cancelReason":reason}),
                context,
            )
        },
        W::DeleteMessages { ids: messages, source_type } => {
            context.source(*source_type)?;
            nested("deleteClientMsgByIdListReq", json!({"recordIdList":ids(messages,100)?}), context)
        },
    }
}
