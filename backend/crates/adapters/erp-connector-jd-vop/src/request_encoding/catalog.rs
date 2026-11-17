//! 商品和地址网关参数；保留商品池请求对象，其余方法使用已实测的顶层字段。
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::*;

/// 编码商品和地址接口的官方业务载荷。
/// # 参数
/// 类型化读取请求及受信账号上下文。
/// # 返回
/// 符合各方法参数层级的业务 JSON。
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
            flat(
                json!({"bizPoolId":pool_id,"offset":identifier_or_zero(offset)?,"pageSize":page_size.get()}),
                context,
            )
        },
        R::Detail { sku_id, query_ext_set } => {
            if query_ext_set.iter().any(|value| *value > u32::try_from(i32::MAX).expect("int正上限可表示"))
            {
                return Err(mapping("京东商品扩展枚举超出接口范围"));
            }
            flat(json!({"skuId":identifier(sku_id)?,"queryExtSet":query_ext_set}), context)
        },
        R::Price { sku_id, quantity } => {
            identifier(sku_id)?;
            flat(json!({"sku":sku_id,"num":price_quantity(quantity)?}), context)
        },
        R::Images { sku_id } => flat(json!({"skuId":[identifier(sku_id)?]}), context),
        R::Categories { category_ids } => flat(json!({"categoryId":ids(category_ids,100)?}), context),
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
                fields["jdAreaId"] = json!(identifier_or_zero(parent)?);
            }
            flat(fields, context)
        },
        JdVopReadRequest::VerifyAddress { province, city, county, town } => {
            flat(area(&[province.clone(), city.clone(), county.clone(), town.clone()])?, context)
        },
        _ => Err(mapping("京东商品或地址读取方法不匹配")),
    }
}
