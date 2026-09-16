//! 双口径查询规范化：拒绝不支持的口径、分组及排序，不静默忽略筛选。
use chrono::{Days, NaiveDate};

use super::dto::{CurrentQualityQuery, HistoryQualityQuery};
use crate::{Error, Result};

pub const PERIOD_BASIS: &str = "sales_order_effective_date";
pub const BASIS_LABEL: &str = "销售单生效日";
/// 已校验的上海自然日边界。
pub struct PeriodBounds {
    pub from: i64,
    pub until: i64,
}

impl CurrentQualityQuery {
    /// 单次最多一年；现任分组下钻只接受现任人员或组织精确身份。
    pub fn validate(&self) -> Result<PeriodBounds> {
        validate_paging(self.page, self.page_size, self.scope_version.as_deref())?;
        validate_sort(&self.sort, &["orderCount", "grossTotal", "customerNo", "label", "customerCount"])?;
        validate_text([&self.q, &self.customer_id])?;
        if !["customer", "owner_user", "owner_org"].contains(&self.dimension.as_str()) {
            return invalid("不支持该经营分组");
        }
        validate_group(self.owner_group.as_deref(), "owner_group", &["user", "org"])?;
        validate_period(&self.from, &self.to)
    }
}

impl HistoryQualityQuery {
    /// 单次最多一年；历史下钻只接受冻结人员或组织的精确分组身份。
    pub fn validate(&self) -> Result<PeriodBounds> {
        validate_paging(self.page, self.page_size, self.scope_version.as_deref())?;
        validate_sort(&self.sort, &["orderCount", "grossTotal", "label"])?;
        validate_text([&self.q, &self.customer_id])?;
        if !["attribution_user", "attribution_org"].contains(&self.dimension.as_str()) {
            return invalid("不支持该贡献分组");
        }
        validate_group(
            self.attribution_group.as_deref(),
            "attribution_group",
            &["attribution_user", "attribution_org"],
        )?;
        validate_period(&self.from, &self.to)
    }
}

/// 分页与跨页版本在读取事实前验证。
fn validate_paging(page: usize, page_size: usize, scope_version: Option<&str>) -> Result<()> {
    if page == 0 || page > 1_000_000 || ![20, 50, 100].contains(&page_size) {
        return invalid("分页参数无效");
    }
    if page > 1 && scope_version.is_none_or(str::is_empty) {
        return invalid("跨页查询必须携带范围版本，请从第一页刷新");
    }
    if scope_version.is_some_and(|version| version.len() > 512) {
        return invalid("范围版本无效");
    }
    Ok(())
}

/// 使用界面列排序白名单，空值统一排最后。
fn validate_sort(sort: &str, fields: &[&str]) -> Result<()> {
    let Some((field, direction)) = sort.split_once(':') else {
        return invalid("排序参数无效");
    };
    if !["asc", "desc"].contains(&direction) || !fields.contains(&field) {
        return invalid("不支持该排序字段");
    }
    Ok(())
}

/// 文本有界且不接受超长身份。
fn validate_text<'a>(values: impl IntoIterator<Item = &'a Option<String>>) -> Result<()> {
    for value in values.into_iter().flatten() {
        if value.len() > 512 {
            return invalid("筛选文本过长");
        }
    }
    Ok(())
}

/// 下钻仅接受精确分组身份；空后缀明确表示未知归属。
fn validate_group(group: Option<&str>, name: &str, dimensions: &[&str]) -> Result<()> {
    let Some(group) = group else {
        return Ok(());
    };
    let Some((dimension, id)) = group.split_once(':') else {
        return invalid(&format!("{name}条件无效"));
    };
    if !dimensions.contains(&dimension)
        || id.len() > 128
        || (!id.is_empty() && !id.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)))
    {
        return invalid(&format!("{name}必须使用有效的人员或组织身份"));
    }
    Ok(())
}

/// 严格校验零填充日期，区间左闭右开且不超过 367 天。
fn validate_period(from: &str, to: &str) -> Result<PeriodBounds> {
    let from = parse_date(from)?;
    let to = parse_date(to)?;
    if to < from || (to - from).num_days() > 366 {
        return invalid("查询期间必须按先后排列且不超过 367 天");
    }
    let next =
        to.checked_add_days(Days::new(1)).ok_or_else(|| Error::ValidationError("结束日期无效".into()))?;
    Ok(PeriodBounds { from: midnight(from), until: midnight(next) })
}

/// 严格校验零填充日期。
fn parse_date(text: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| Error::ValidationError("日期格式必须为 YYYY-MM-DD".into()))?;
    if date.format("%Y-%m-%d").to_string() != text {
        return invalid("日期格式必须为 YYYY-MM-DD");
    }
    Ok(date)
}

/// 上海自然日零点换算 UTC 秒，固定时区不依赖机器时区。
fn midnight(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0).expect("合法零点").and_utc().timestamp() - 8 * 3600
}

/// 保留统一参数错误分类。
fn invalid<T>(message: &str) -> Result<T> {
    Err(Error::ValidationError(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_and_history_dimensions_are_not_interchangeable() {
        let current = CurrentQualityQuery {
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            dimension: "attribution_user".into(),
            ..Default::default()
        };
        assert!(current.validate().is_err());
        let history = HistoryQualityQuery {
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            dimension: "owner_user".into(),
            ..Default::default()
        };
        assert!(history.validate().is_err());
    }

    #[test]
    fn unknown_fields_and_mixed_caliber_params_are_rejected() {
        let mixed = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","attribution_user_ids":"u-1"});
        assert!(serde_json::from_value::<CurrentQualityQuery>(mixed).is_err());
        let mixed = serde_json::json!({"from":"2026-09-01","to":"2026-09-30","owner_user_ids":"u-1"});
        assert!(serde_json::from_value::<HistoryQualityQuery>(mixed).is_err());
        let paged = CurrentQualityQuery { page: 2, ..Default::default() };
        assert!(paged.validate().is_err());
    }
}
