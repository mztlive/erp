//! 商品和地址的 wrapper 及字段依据对应官方 API 5850/6741/5832/5847/5845/5846/5680/5710。
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::*;

/// 编码商品和地址接口的官方业务载荷。
/// # 参数
/// 类型化读取请求及受信账号上下文。
/// # 返回
/// 含正确 wrapper 的业务 JSON。
/// # 错误
/// 非法标识、数量或未支持方法时拒绝。
pub(super) fn read(
    request: &JdVopReadRequest,
    context: &JdVopRequestContext,
) -> ConnectorResult<Box<RawValue>> {
    use JdVopReadRequest as R;
    match request {
        R::Pools => nested("getSkuPoolInfoGoodsReq", json!({}), context),
        R::PoolPage { pool_id, offset, page_size } => {
            if page_size.get() > 100 {
                return Err(mapping("京东商品池分页大小超过100"));
            }
            nested(
                "rpcReq",
                json!({"bizPoolId":pool_id,"offset":number(offset,true)?,"pageSize":page_size.get()}),
                context,
            )
        },
        R::Detail { sku_id, query_ext_set } => {
            if query_ext_set.iter().any(|value| *value > u32::try_from(i32::MAX).expect("int正上限可表示"))
            {
                return Err(mapping("京东商品扩展枚举超出接口范围"));
            }
            nested(
                "getSkuDetailGoodsReq",
                json!({"skuId":identifier(sku_id)?,"queryExtSet":query_ext_set}),
                context,
            )
        },
        R::Price { sku_id, quantity } => {
            identifier(sku_id)?;
            nested(
                "getSellPriceGoodsReq",
                json!({"skuInfos":[{"skuId":sku_id,"num":price_quantity(quantity)?}]}),
                context,
            )
        },
        R::Images { sku_id } => {
            nested("getSkuImageGoodsReq", json!({"skuIdList":[identifier(sku_id)?]}), context)
        },
        R::Categories { category_ids } => {
            nested("getCategoryInfoGoodsReq", json!({"categoryIdSet":ids(category_ids,100)?}), context)
        },
        _ => address(request, context),
    }
}

fn price_quantity(raw: &str) -> ConnectorResult<String> {
    let quantity: Decimal = raw.parse().map_err(|_| mapping("京东报价数量须为正整数"))?;
    if quantity <= Decimal::ZERO || !quantity.fract().is_zero() {
        return Err(mapping("京东报价数量须为正整数"));
    }
    quantity
        .to_u32()
        .filter(|value| *value <= u32::try_from(i32::MAX).expect("int正上限可表示"))
        .map(|value| value.to_string())
        .ok_or_else(|| mapping("京东报价数量超出接口范围"))
}

fn address(request: &JdVopReadRequest, context: &JdVopRequestContext) -> ConnectorResult<Box<RawValue>> {
    match request {
        JdVopReadRequest::Areas { level, parent_code } => {
            if !(1..=4).contains(level) || (*level != 1 && parent_code.is_none()) {
                return Err(mapping("京东地区级别或父级编号无效"));
            }
            let mut fields = json!({"areaLevel":level});
            if let Some(parent) = parent_code {
                fields["jdAreaId"] = json!(number(parent, true)?);
            }
            nested("queryJdAreaIdListOpenReq", fields, context)
        },
        JdVopReadRequest::VerifyAddress { province, city, county, town } => nested(
            "verifyAreaFourIdOpenReq",
            area(&[province.clone(), city.clone(), county.clone(), town.clone()])?,
            context,
        ),
        _ => Err(mapping("京东商品或地址读取方法不匹配")),
    }
}
