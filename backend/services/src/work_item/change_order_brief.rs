//! 销售变更与采购变更审批任务的事项简报装载。
//!
//! 变更单审批走通用 DocumentApproval 工作面；简报展示来源单号、变更类型和原因。

use std::collections::{HashMap, HashSet};

use database::{Executor, PurchaseOrderExt, SalesOrderExt, SalesReviewExt};

use super::brief::{join_list_summary, non_empty, push_document_section, push_section, ObjectBriefSource};
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

impl WorkItemService {
    /// 销售变更审批任务的对象事实：任务对象是变更单本身。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入变更类型、原因和来源销售单号。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_sales_change_review_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesChangeOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let changes = self
            .db
            .sales_change_orders()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        if changes.is_empty() {
            return Ok(());
        }
        let sales_order_ids = changes
            .iter()
            .map(|item| item.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let sales_nos = self
            .db
            .sales_orders()
            .list_work_item_brief_entities_by_ids(&sales_order_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect::<HashMap<_, _>>();
        for change in changes {
            let sales_no = sales_nos.get(&change.sales_order_id.to_string()).cloned();
            let mut fact = ObjectFact::new(
                change.base.id.clone(),
                sales_no
                    .as_deref()
                    .map(|no| format!("销售变更单 {no}"))
                    .unwrap_or_else(|| format!("销售变更单 {}", change.sales_order_id)),
                change.stable.created_by,
            );
            fact.impact_summary = Some("不审批则销售变更不能生效".to_string());
            let mut sections = Vec::new();
            let sales_order_id = change.sales_order_id.to_string();
            push_document_section(
                &mut sections,
                "来源销售单",
                sales_no.as_deref(),
                Some(sales_order_id.as_str()),
            );
            push_section(&mut sections, "变更类型", Some(change.change_type.label()), false);
            push_section(&mut sections, "原因", non_empty(&change.reason).as_deref(), false);
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: None,
                extra_sections: sections,
                list_summary: join_list_summary([
                    sales_no.map(|no| format!("销售单 {no}")),
                    Some(change.change_type.label().to_string()),
                    non_empty(&change.reason),
                ]),
                lines: Vec::new(),
                more_count: 0,
                submitter_name: None,
            });
            facts.insert((ObjectKind::SalesChangeOrder, change.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 采购变更审批任务的对象事实：任务对象是变更单本身。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入原因和来源采购单号。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_purchase_change_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchaseChangeOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let changes = self
            .db
            .purchase_change_orders()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        if changes.is_empty() {
            return Ok(());
        }
        let purchase_ids = changes
            .iter()
            .map(|item| item.purchase_order_id.to_string())
            .collect::<Vec<_>>();
        let purchase_nos = self
            .db
            .purchase_orders()
            .list_work_item_brief_entities_by_ids(&purchase_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect::<HashMap<_, _>>();
        for change in changes {
            let purchase_no = purchase_nos.get(&change.purchase_order_id.to_string()).cloned();
            let mut fact = ObjectFact::new(
                change.base.id.clone(),
                purchase_no
                    .as_deref()
                    .map(|no| format!("采购变更单 {no}"))
                    .unwrap_or_else(|| format!("采购变更单 {}", change.purchase_order_id)),
                change.stable.created_by,
            );
            fact.impact_summary = Some("不审批则采购变更不能生效".to_string());
            let mut sections = Vec::new();
            push_section(&mut sections, "来源采购单", purchase_no.as_deref(), false);
            push_section(&mut sections, "原因", non_empty(&change.reason).as_deref(), false);
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: None,
                extra_sections: sections,
                list_summary: join_list_summary([
                    purchase_no.map(|no| format!("采购单 {no}")),
                    non_empty(&change.reason),
                ]),
                lines: Vec::new(),
                more_count: 0,
                submitter_name: None,
            });
            facts.insert((ObjectKind::PurchaseChangeOrder, change.base.id.clone()), fact);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::join_list_summary;

    #[test]
    fn change_list_summary_joins_source_and_reason() {
        let summary = join_list_summary([
            Some("销售单 SO-1".into()),
            Some("数量".into()),
            Some("客户减量".into()),
        ]);
        assert_eq!(summary, "销售单 SO-1 · 数量 · 客户减量");
    }
}
