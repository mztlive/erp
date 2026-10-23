//! 工作台搜索条件；仅收窄已授权事项，不参与权限授予。
use chrono::{FixedOffset, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

use super::super::WorkItemListParams;
use super::SearchDocument;
use crate::{Error, Result};

/// 多视图共用查询条件，线协议以 filters JSON 传输。
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceFilters {
    pub due: Option<String>,
    pub item_types: Vec<String>,
    pub counterparties: Vec<String>,
    pub handlers: Vec<String>,
    pub sales_orders: Vec<String>,
    pub purchase_orders: Vec<String>,
    pub warehouses: Vec<String>,
    pub business_types: Vec<String>,
    pub statuses: Vec<String>,
    pub date_field: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub undated: bool,
    pub blocked: bool,
    pub option_query: Option<String>,
    pub include_options: bool,
}

impl WorkspaceFilters {
    /// 解析并校验查询边界。
    /// # 参数
    /// `raw` 为可选 JSON 条件。
    /// # 返回
    /// 返回去重后的合法条件。
    /// # 错误
    /// 非法字段、日期、类型或过长条件返回校验错误。
    pub fn parse(raw: Option<&str>) -> Result<Self> {
        let mut value: Self = match raw {
            Some(raw) if raw.len() <= 32000 => serde_json::from_str(raw)
                .map_err(|_| Error::ValidationError("工作台筛选条件格式不正确".into()))?,
            Some(_) => return Err(Error::ValidationError("工作台筛选条件过长".into())),
            None => Self::default(),
        };
        for values in [
            &mut value.item_types,
            &mut value.counterparties,
            &mut value.handlers,
            &mut value.sales_orders,
            &mut value.purchase_orders,
            &mut value.warehouses,
            &mut value.business_types,
            &mut value.statuses,
        ] {
            if values.len() > 100 || values.iter().any(|s| s.trim().is_empty() || s.chars().count() > 256) {
                return Err(Error::ValidationError("筛选值不能为空，每项至多256字且最多100项".into()));
            }
            for item in values.iter_mut() {
                *item = item.trim().to_owned();
            }
            values.sort();
            values.dedup();
        }
        value.validate_dates()?;
        if value.due.as_deref().is_some_and(|due| !["today", "overdue"].contains(&due)) {
            return Err(Error::ValidationError("截止条件不支持".into()));
        }
        if value.due.is_some() && (value.date_from.is_some() || value.date_to.is_some() || value.undated) {
            return Err(Error::ValidationError("截止快捷条件不能与日期区间同时使用".into()));
        }
        if value.option_query.as_ref().is_some_and(|q| q.chars().count() > 128) {
            return Err(Error::ValidationError("选项检索词不能超过128字".into()));
        }
        Ok(value)
    }

    /// 校验日期字段与含首尾日的区间。
    fn validate_dates(&self) -> Result<()> {
        if self.date_field.as_deref().is_some_and(|v| !["created", "due", "completed"].contains(&v)) {
            return Err(Error::ValidationError("时间类型不支持".into()));
        }
        if self.undated && (self.date_from.is_some() || self.date_to.is_some()) {
            return Err(Error::ValidationError("未设置日期不能与日期区间同时使用".into()));
        }
        let from = self.date_from.as_deref().map(date_seconds).transpose()?;
        let to = self.date_to.as_deref().map(date_seconds).transpose()?;
        if from.zip(to).is_some_and(|(a, b)| a > b) {
            return Err(Error::ValidationError("开始日期不能晚于结束日期".into()));
        }
        Ok(())
    }

    /// 返回当前维度的已选值，用于保留候选标签。
    /// # 参数
    /// `key` 为筛选字段名。
    /// # 返回
    /// 该字段的已选值；未知字段返回空切片。
    /// # 错误
    /// 无。
    pub(super) fn selected(&self, key: &str) -> &[String] {
        match key {
            "item_types" => &self.item_types,
            "counterparties" => &self.counterparties,
            "handlers" => &self.handlers,
            "sales_orders" => &self.sales_orders,
            "purchase_orders" => &self.purchase_orders,
            "warehouses" => &self.warehouses,
            "business_types" => &self.business_types,
            "statuses" => &self.statuses,
            _ => &[],
        }
    }

    /// 判断全部结构化条件；同维度 OR，不同维度 AND。
    /// # 参数
    /// `document` 为已授权事项检索资料。
    /// # 返回
    /// 全部条件成立时返回 true。
    /// # 错误
    /// 无；输入条件须先经过 parse 校验。
    pub(in crate::workbench) fn matches(&self, document: &SearchDocument) -> bool {
        let facets = [
            (&self.item_types, "item_types"),
            (&self.counterparties, "counterparties"),
            (&self.handlers, "handlers"),
            (&self.sales_orders, "sales_orders"),
            (&self.purchase_orders, "purchase_orders"),
            (&self.warehouses, "warehouses"),
            (&self.business_types, "business_types"),
            (&self.statuses, "statuses"),
        ];
        facets.iter().all(|(selected, key)| {
            selected.is_empty()
                || document
                    .values
                    .get(*key)
                    .is_some_and(|values| values.iter().any(|v| selected.contains(&v.value)))
        }) && (!self.blocked || document.blocked)
            && self.matches_date(document)
    }

    fn matches_date(&self, document: &SearchDocument) -> bool {
        if !self.undated && self.date_from.is_none() && self.date_to.is_none() {
            return true;
        }
        let timestamp = match self.date_field.as_deref().unwrap_or("due") {
            "created" => document.created_at,
            "completed" => document.completed_at,
            _ => document.due_at,
        };
        if self.undated {
            return timestamp.is_none();
        }
        let Some(timestamp) = timestamp else {
            return false;
        };
        self.date_from.as_deref().is_none_or(|date| date_seconds(date).is_ok_and(|v| timestamp >= v))
            && self
                .date_to
                .as_deref()
                .is_none_or(|date| date_seconds(date).is_ok_and(|v| timestamp < v + 86400))
    }
}

fn date_seconds(value: &str) -> Result<i64> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .filter(|d| d.format("%Y-%m-%d").to_string() == value)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .and_then(|d| FixedOffset::east_opt(8 * 3600)?.from_local_datetime(&d).single());
    date.map(|d| d.timestamp()).ok_or_else(|| Error::ValidationError("日期须为有效的 YYYY-MM-DD".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_dates_and_unknown_filters() {
        for raw in [
            r#"{"date_from":"2026-02-30"}"#,
            r#"{"date_from":"2026-10-24","date_to":"2026-10-23"}"#,
            r#"{"undated":true,"date_to":"2026-10-23"}"#,
            r#"{"owner_scope":"company"}"#,
        ] {
            assert!(WorkspaceFilters::parse(Some(raw)).is_err());
        }
    }
    #[test]
    fn dates_include_end_day_and_missing_is_not_zero() {
        let filter =
            WorkspaceFilters::parse(Some(r#"{"date_from":"2026-10-23","date_to":"2026-10-23"}"#)).unwrap();
        let mut doc = SearchDocument::default();
        assert!(!filter.matches(&doc));
        doc.due_at = Some(date_seconds("2026-10-23").unwrap() + 86399);
        assert!(filter.matches(&doc));
        doc.due_at = Some(date_seconds("2026-10-24").unwrap());
        assert!(!filter.matches(&doc));
    }
    #[test]
    fn dimensions_intersect_and_values_union() {
        let filter =
            WorkspaceFilters::parse(Some(r#"{"handlers":["a","b","a"],"counterparties":["客户"]}"#)).unwrap();
        assert_eq!(filter.handlers, ["a", "b"]);
        let mut doc = SearchDocument::default();
        doc.facet("handlers", "b", "李四");
        assert!(!filter.matches(&doc));
        doc.facet("counterparties", "客户", "客户");
        assert!(filter.matches(&doc));
    }
}

/// 工作台 HTTP 查询扩展；保留既有领域调用入参。
#[derive(Debug, Clone)]
pub struct WorkItemSearchParams {
    pub base: WorkItemListParams,
    pub filters: Option<String>,
}
