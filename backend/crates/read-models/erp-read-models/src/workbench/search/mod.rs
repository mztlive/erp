//! 审批业务搜索资料。
mod approvals;
mod related_sales;
mod sources;
use erp_workflow::service::work_item::read::search::{SearchDocument, approval_label, collect_options};
pub use erp_workflow::service::work_item::read::search::{SearchOption, SearchOptions, WorkspaceFilters};

use super::brief::BriefSection;
// 组装审批业务检索时，从已授权业务键值构造文本及筛选维度。
//
// 调用参数：
// 参数只接受已授权事项的可读业务字段。
//
// 更新当前检索资料，匹配方法返回命中说明或空结果。
fn append_sections(document: &mut SearchDocument, sections: &[BriefSection]) {
    for section in sections {
        if ["提交人", "责任人"].contains(&section.label.as_str()) {
            continue;
        }

        document.text(&section.label, &section.value);
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
            document.facet(key, &section.value, &section.value);
        }
        if let Some(id) = &section.object_id {
            if section.label.contains("销售单") {
                document.facet("sales_orders", id, &section.value);
            }
            if section.label.contains("采购单") {
                document.facet("purchase_orders", id, &section.value);
            }
        }
    }
}
