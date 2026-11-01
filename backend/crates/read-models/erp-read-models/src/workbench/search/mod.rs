//! 已授权事项的服务端检索、筛选与候选项；先匹配，再计数及分页。
mod approvals;
mod blockers;
mod filters;
mod queue;
mod related_sales;
mod sources;
use std::collections::{BTreeMap, HashMap};

use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItemType;
pub use filters::{WorkItemSearchParams, WorkspaceFilters};
use persistence_core::Executor;
use serde::Serialize;

use super::WorkbenchReadService;
use super::brief::{BriefSection, ObjectBriefSource};
use super::dto::WorkItemFields;
use crate::Result;

/// 查询框选项，值仅用于筛选，不授予读取权限。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchOption {
    pub value: String,
    pub label: String,
}
/// 各筛选字段的授权候选项。
pub type SearchOptions = BTreeMap<String, Vec<SearchOption>>;

#[derive(Debug, Clone, Default)]
pub(super) struct SearchDocument {
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
    pub(super) fn merge(&mut self, source: &Self) {
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
    pub(super) fn facet(&mut self, key: &str, value: &str, label: &str) {
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
    pub(super) fn text(&mut self, label: &str, value: &str) {
        if !value.trim().is_empty() {
            self.texts.push(SearchOption { value: value.to_owned(), label: label.to_owned() });
        }
    }
    /// 从已授权摘要构造检索资料。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub(super) fn source(&mut self, source: &ObjectBriefSource) {
        if let Some(customer) = &source.customer {
            self.text("客户", customer);
            self.facet("counterparties", customer, customer);
        }
        for line in &source.lines {
            self.text("商品", &line.title);
        }
        self.text("事项摘要", &source.list_summary);
        self.sections(&source.extra_sections);
    }
    /// 从已授权业务键值构造文本及筛选维度。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub(super) fn sections(&mut self, sections: &[BriefSection]) {
        for section in sections {
            if ["提交人", "责任人"].contains(&section.label.as_str()) {
                continue;
            }
            self.text(&section.label, &section.value);
            let key = match section.label.as_str() {
                "仓库" | "发货仓库" | "收货仓库" => Some("warehouses"),
                "业务性质" | "采购类型" | "履约方式" | "申请类型" | "发票类型" | "业务类型" | "开票类型"
                | "开票要求" | "履约责任" => Some("business_types"),
                "客户" | "供应商" | "结算主体" | "我方付款主体" | "往来主体" => {
                    Some("counterparties")
                },
                _ => None,
            };
            if let Some(key) = key {
                self.facet(key, &section.value, &section.value);
            }
            if let Some(id) = &section.object_id {
                if section.label.contains("销售单") {
                    self.facet("sales_orders", id, &section.value);
                }
                if section.label.contains("采购单") {
                    self.facet("purchase_orders", id, &section.value);
                }
            }
        }
    }
    /// 按字面量忽略大小写匹配，并返回最多三条命中说明。
    /// # 参数
    /// 参数只接受已授权事项的可读业务字段。
    /// # 返回
    /// 更新当前检索资料，匹配方法返回命中说明或空结果。
    /// # 错误
    /// 无。
    pub(super) fn matches(&self, query: Option<&str>) -> Option<Vec<String>> {
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
pub(super) fn collect_options(
    options: &mut SearchOptions,
    document: &SearchDocument,
    filter: &WorkspaceFilters,
) {
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

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 授权后批量补充用于搜索的姓名、关联单据和完整版本明细。
    /// # 参数
    /// `fields` 为已授权候选；`executor` 沿用查询快照。
    /// # 返回
    /// 就地写入检索字段，不改变任务身份或授权。
    /// # 错误
    /// 姓名或版本明细读取失败时中止查询。
    pub(super) async fn search_documents(
        &self,
        fields: &mut [WorkItemFields],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = fields.iter().filter_map(|f| f.owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.load_account_names(&ids, executor).await?;
        for field in fields.iter_mut() {
            field.search = task_document(field, &names);
        }
        self.search_order_lines(fields, executor).await
    }
}

/// 将任务已授权字段转换为检索文档。
/// # 参数
/// `field` 为已授权任务，`names` 为同批读取的处理人姓名。
/// # 返回
/// 返回日期、筛选维度和可读检索文本。
/// # 错误
/// 无。
pub(in crate::workbench) fn task_document(
    field: &WorkItemFields,
    names: &HashMap<String, String>,
) -> SearchDocument {
    let mut doc = SearchDocument {
        created_at: i64::try_from(field.created_at).ok(),
        due_at: field.due_at.map(|d| d.unix_secs()),
        completed_at: field.completed_at.or(field.closed_at).map(|d| d.unix_secs()),
        blocked: false,
        ..Default::default()
    };
    task_facets(&mut doc, field);
    doc.text("单号", &field.business_object_label);
    if let Some(name) = &field.counterparty_label {
        doc.facet("counterparties", name, name);
        doc.text("往来方", name);
    }
    if let Some(id) = &field.owner_user_id
        && let Some(name) = names.get(id)
    {
        doc.facet("handlers", id, name);
        doc.text("处理人", name);
    }
    if let Some(source) = &field.brief_source {
        doc.source(source);
    }
    for text in &field.search_lines {
        doc.text("商品", text);
    }
    for line in &field.delivery_lines {
        doc.text("商品", &line.title);
    }
    if let Some(impact) = &field.impact_summary {
        doc.text("事项说明", impact);
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
pub(super) fn approval_label(kind: &str) -> String {
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
        let mut page = super::super::query::AuthorizedPageCollector::new(2, 50).unwrap();
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
