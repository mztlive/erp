//! 查询规范化：拒绝不支持的口径、分组及排序，不静默忽略筛选。
use super::dto::ProfitLossQuery;
use crate::{Error, Result};
use chrono::{Days, NaiveDate};
use erp_finance::entity::cost::CostType;

pub const PERIOD_BASIS: &str = "sales_order_effective_date";
pub const BASIS_LABEL: &str = "销售单生效日（累计实际成本）";
pub const FORMULA_VERSION: &str = "non-voucher-net-v1";
/// 已校验的上海自然日边界。
pub struct PeriodBounds {
    pub from: i64,
    pub until: i64,
}
impl ProfitLossQuery {
    /// 单次最多一年；分页和所有枚举在读取事实前验证。
    pub fn validate(&self) -> Result<PeriodBounds> {
        if self.period_basis != PERIOD_BASIS {
            return invalid("请选择销售单生效日口径");
        }
        if !["covered", "uncovered", "all"].contains(&self.coverage.as_str()) {
            return invalid("成本覆盖筛选无效");
        }
        if !["sales_order", "customer", "scenario"].contains(&self.dimension.as_str()) {
            return invalid("不支持该盈亏分组");
        }
        if self.page == 0 || self.page > 1_000_000 || ![20, 50, 100].contains(&self.page_size) {
            return invalid("分页参数无效");
        }
        self.validate_sort()?;
        self.validate_text()?;
        let from = parse_date(&self.from)?;
        let to = parse_date(&self.to)?;
        if to < from || (to - from).num_days() > 366 {
            return invalid("查询期间必须按先后排列且不超过 367 天");
        }
        let next = to
            .checked_add_days(Days::new(1))
            .ok_or_else(|| Error::ValidationError("结束日期无效".into()))?;
        Ok(PeriodBounds {
            from: midnight(from),
            until: midnight(next),
        })
    }
    /// 使用界面列排序白名单，空值统一排最后。
    fn validate_sort(&self) -> Result<()> {
        let Some((field, direction)) = self.sort.split_once(':') else {
            return invalid("排序参数无效");
        };
        if !["asc", "desc"].contains(&direction)
            || ![
                "identityLabel",
                "netSalesRevenue",
                "actualProcurementCostNet",
                "actualFulfillmentCostNet",
                "reductionsNet",
                "actualProfitLossNet",
                "marginRate",
                "coverageState",
            ]
            .contains(&field)
        {
            return invalid("不支持该排序字段");
        }
        Ok(())
    }
    /// 文本有界，成本类别只接受财务领域已有代码。
    fn validate_text(&self) -> Result<()> {
        for value in [
            &self.q,
            &self.customer_id,
            &self.sales_order_id,
            &self.benefit_scenario,
            &self.cost_types,
        ]
        .into_iter()
        .flatten()
        {
            if value.len() > 512 {
                return invalid("筛选文本过长");
            }
        }
        for code in self.cost_codes() {
            if !cost_types().iter().any(|t| t.as_str() == code) {
                return invalid("成本类型无效");
            }
        }
        Ok(())
    }
    /// 成本筛选命中销售单后保留整单成本，不能只减所选类别而虚增利润。
    pub fn cost_codes(&self) -> Vec<&str> {
        self.cost_types
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    }
}
/// 完整费用字典，也用于空结果时仍可修改筛选。
pub fn cost_types() -> [CostType; 9] {
    [
        CostType::Product,
        CostType::Logistics,
        CostType::Printing,
        CostType::Storage,
        CostType::Delivery,
        CostType::PlatformTech,
        CostType::OfflineService,
        CostType::Rebate,
        CostType::Other,
    ]
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
    fn dates_use_shanghai_and_reject_unsupported_filters() {
        let mut q = ProfitLossQuery {
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            period_basis: PERIOD_BASIS.into(),
            coverage: "all".into(),
            dimension: "sales_order".into(),
            sort: "actualProfitLossNet:asc".into(),
            page: 1,
            page_size: 20,
            ..Default::default()
        };
        assert_eq!(
            chrono::DateTime::from_timestamp(q.validate().unwrap().from, 0)
                .unwrap()
                .to_rfc3339(),
            "2026-08-31T16:00:00+00:00"
        );
        q.period_basis = "cost_occurred_date".into();
        assert!(q.validate().is_err());
        q.period_basis = PERIOD_BASIS.into();
        q.to = "2026-08-01".into();
        assert!(q.validate().is_err());
        q.to = "2026-09-30".into();
        q.cost_types = Some("$where".into());
        assert!(q.validate().is_err());
    }
}
