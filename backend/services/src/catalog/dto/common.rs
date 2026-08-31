use entities::money::Amount;

use crate::errors::{Error, Result};

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 code/name 需要按「空白视为空」拒绝，落入 HTTP 400）。
pub(super) use crate::query::non_blank;

/// 校验目录查询共用的销售价区间。
///
/// 先检查任一端点为负，再检查同时存在的下限是否高于上限；该顺序与对外
/// 参数错误合同一致，商品列表与公司商品池必须复用同一规则源。
///
/// # 参数
/// * `minimum` - 可选销售价下限（含）
/// * `maximum` - 可选销售价上限（含）
///
/// # 返回
/// 区间合法时返回 `Ok(())`。
///
/// # 错误
/// 金额为负或下限高于上限时返回 `ValidationError`。
pub(crate) fn validate_sales_price_range(minimum: Option<Amount>, maximum: Option<Amount>) -> Result<()> {
    if minimum.is_some_and(|value| value.to_decimal().is_sign_negative())
        || maximum.is_some_and(|value| value.to_decimal().is_sign_negative())
    {
        return Err(Error::ValidationError("销售价不能小于 0".to_string()));
    }
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum > maximum) {
        return Err(Error::ValidationError("最低销售价不能高于最高销售价".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::money::Amount;

    use super::validate_sales_price_range;
    use crate::errors::Error;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    #[test]
    fn sales_price_range_accepts_open_equal_and_ordered_bounds() {
        assert!(validate_sales_price_range(None, None).is_ok());
        assert!(validate_sales_price_range(Some(amount("10.00")), None).is_ok());
        assert!(validate_sales_price_range(None, Some(amount("20.00"))).is_ok());
        assert!(validate_sales_price_range(Some(amount("10.00")), Some(amount("10.00"))).is_ok());
        assert!(validate_sales_price_range(Some(amount("10.00")), Some(amount("20.00"))).is_ok());
    }

    #[test]
    fn sales_price_range_rejects_negative_before_inverted_bounds() {
        let negative = validate_sales_price_range(Some(amount("-1.00")), Some(amount("-2.00")))
            .expect_err("负数端点必须优先被拒绝");
        assert!(matches!(
            negative,
            Error::ValidationError(message) if message == "销售价不能小于 0"
        ));

        let inverted = validate_sales_price_range(Some(amount("20.00")), Some(amount("10.00")))
            .expect_err("倒挂区间必须被拒绝");
        assert!(matches!(
            inverted,
            Error::ValidationError(message) if message == "最低销售价不能高于最高销售价"
        ));
    }
}
