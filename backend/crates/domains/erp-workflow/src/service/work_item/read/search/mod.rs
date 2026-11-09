mod blockers;
mod queue;
use std::collections::BTreeMap;

use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItemType;
mod filters;
pub use filters::{WorkItemSearchParams, WorkspaceFilters};
use serde::Serialize;

use super::dto::WorkItemFields;

/// 查询框选项，值仅用于筛选，不授予读取权限。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchOption {
    pub value: String,
    pub label: String,
}
/// 各筛选字段的授权候选项。
pub type SearchOptions = BTreeMap<String, Vec<SearchOption>>;

#[derive(Debug, Clone, Default)]
pub struct SearchDocument {
    pub values: SearchOptions,
    pub texts: Vec<SearchOption>,
    pub created_at: Option<i64>,
    pub due_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub blocked: bool,
}
impl SearchDocument {
    /// 合并同一授权事项的额外检索资料。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub fn merge(&mut self, source: &Self) {
        self.texts.extend(source.texts.clone());
        for (key, values) in &source.values {
            for value in values {
                self.facet(key, &value.value, &value.label);
            }
        }
    }

    /// 登记有值且不重复的筛选候选。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub fn facet(&mut self, key: &str, value: &str, label: &str) {
        if value.trim().is_empty() || label.trim().is_empty() {
            return;
        }
        let entry = SearchOption { value: value.to_owned(), label: label.to_owned() };
        let values = self.values.entry(key.to_owned()).or_default();
        if !values.contains(&entry) {
            values.push(entry);
        }
    }
    /// 登记非空的可读检索字段。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub fn text(&mut self, label: &str, value: &str) {
        if !value.trim().is_empty() {
            self.texts.push(SearchOption { value: value.to_owned(), label: label.to_owned() });
        }
    }
    /// 按字面量忽略大小写匹配，并返回最多三条命中说明。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub fn matches(&self, query: Option<&str>) -> Option<Vec<String>> {
        let needle = query.unwrap_or_default().trim().to_lowercase();
        if needle.is_empty() {
            return Some(Vec::new());
        }
        let mut hits = Vec::new();
        for entry in &self.texts {
            if entry.value.to_lowercase().contains(&needle) {
                let hit = format!("{}：{}", entry.label, entry.value);
                if !hits.contains(&hit) {
                    hits.push(hit);
                }
                if hits.len() == 3 {
                    break;
                }
            }
        }
        (!hits.is_empty()).then_some(hits)
    }
}

/// 合并完整授权候选的选项，搜索在服务端执行；不从当前页推算候选。
pub fn collect_options(options: &mut SearchOptions, document: &SearchDocument, filter: &WorkspaceFilters) {
    if !filter.include_options {
        return;
    }
    let needle = filter.option_query.as_deref().unwrap_or_default().trim().to_lowercase();
    for (key, values) in &document.values {
        let collected = options.entry(key.clone()).or_default();
        for value in values {
            if ((collected.len() < 100 && value.label.to_lowercase().contains(&needle))
                || filter.selected(key).contains(&value.value))
                && !collected.contains(value)
            {
                collected.push(value.clone());
            }
        }
    }
}

/// 将任务已授权字段转换为检索文档。
/// # 参数
/// `field` 为已授权任务；姓名不参与关键词匹配。
/// # 返回
/// 返回日期、筛选维度和可读检索文本。
/// # 错误
/// 无。
pub(super) fn task_document(field: &WorkItemFields) -> SearchDocument {
    let mut doc = SearchDocument {
        created_at: i64::try_from(field.created_at).ok(),
        due_at: field.due_at.map(|d| d.unix_secs()),
        completed_at: field.completed_at.or(field.closed_at).map(|d| d.unix_secs()),
        blocked: false,
        ..Default::default()
    };
    task_facets(&mut doc, field);
    for link in &field.links {
        let key = match link.object_type() {
            "sales_order" => "sales_orders",
            "purchase_order" => "purchase_orders",
            _ => continue,
        };
        doc.facet(key, link.object_id(), link.object_id());
    }
    doc.text("任务编号", &field.id);
    doc.text("关联对象编号", &field.business_object_id);
    if let Some(id) = &field.owner_user_id {
        doc.facet("handlers", id, id);
    }
    doc
}

fn task_facets(doc: &mut SearchDocument, field: &WorkItemFields) {
    let kind = if field.work_item_type == WorkItemType::DocumentApproval {
        format!("approval:{}", field.business_object_type)
    } else {
        format!("task:{}", field.work_item_type.as_str())
    };
    let label = if field.work_item_type == WorkItemType::DocumentApproval {
        approval_label(&field.business_object_type)
    } else {
        format!("执行 · {}", field.work_item_type.label())
    };
    doc.facet("item_types", &kind, &label);
    if field.work_item_type == WorkItemType::DocumentApproval {
        doc.facet("item_types", "task:DOCUMENT_APPROVAL", "审批 · 全部类型");
    }
    doc.facet(
        "statuses",
        field.status.as_str(),
        match field.status.as_str() {
            "OPEN" => "待处理",
            "COMPLETED" => "已完成",
            _ => "已关闭",
        },
    );
}

/// 将审批类型翻译成明确的事项名称。
/// # 参数
/// `kind` 为已登记单据类型。
/// # 返回
/// 中文审批名称；未登记类型显示通用名称。
/// # 错误
/// 无。
pub fn approval_label(kind: &str) -> String {
    DocumentType::ALL
        .iter()
        .find(|v| v.as_str() == kind)
        .map(|v| format!("审批 · {}", v.label()))
        .unwrap_or_else(|| "单据审批".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyword_is_literal_and_returns_business_match_location() {
        let mut doc = SearchDocument::default();
        doc.text("来源销售单", "XS.[2026]");
        doc.text("商品", "保温杯 AB-123");
        assert!(doc.matches(Some(".*")).is_none());
        assert_eq!(doc.matches(Some(" ab-123 ")).unwrap(), ["商品：保温杯 AB-123"]);
        assert_eq!(doc.matches(Some("XS.[2026]")).unwrap(), ["来源销售单：XS.[2026]"]);
    }
    #[test]
    fn bounded_options_keep_selected_labels_and_search_beyond_first_hundred() {
        let mut doc = SearchDocument::default();
        for i in 0..150 {
            doc.facet("handlers", &format!("u-{i}"), &format!("人员{i}"));
        }
        let filters =
            WorkspaceFilters { include_options: true, handlers: vec!["u-149".into()], ..Default::default() };
        let mut options = SearchOptions::new();
        collect_options(&mut options, &doc, &filters);
        assert_eq!(options["handlers"].len(), 101);
        assert!(options["handlers"].iter().any(|value| value.label == "人员149"));
        let mut searched = SearchOptions::new();
        collect_options(
            &mut searched,
            &doc,
            &WorkspaceFilters {
                include_options: true,
                option_query: Some("人员148".into()),
                ..Default::default()
            },
        );
        assert_eq!(searched["handlers"], [SearchOption { value: "u-148".into(), label: "人员148".into() }]);
    }
    #[test]
    fn full_content_matches_before_total_and_page_slice() {
        let mut documents = Vec::new();
        for i in 0..125 {
            let mut doc = SearchDocument::default();
            doc.facet("counterparties", if i % 2 == 0 { "客户甲" } else { "客户乙" }, "客户");
            for line in 0..6 {
                doc.text("商品", if line == 5 { "最后一行-保温杯" } else { "普通商品" });
            }
            documents.push(doc);
        }
        let filters = WorkspaceFilters { counterparties: vec!["客户甲".into()], ..Default::default() };
        let mut page = super::super::context::AuthorizedPageCollector::new(2, 50).unwrap();
        page.extend(
            documents.into_iter().filter(|doc| filters.matches(doc) && doc.matches(Some("保温杯")).is_some()),
        );
        let page = page.finish();
        assert_eq!(page.total, 63);
        assert_eq!(page.items.len(), 13);
    }
    #[test]
    fn options_search_matches_labels_not_internal_ids() {
        let mut doc = SearchDocument::default();
        doc.facet("handlers", "private-id", "陈国平");
        let mut options = SearchOptions::new();
        collect_options(
            &mut options,
            &doc,
            &WorkspaceFilters {
                include_options: true,
                option_query: Some("private-id".into()),
                ..Default::default()
            },
        );
        assert!(options["handlers"].is_empty());
    }
}
