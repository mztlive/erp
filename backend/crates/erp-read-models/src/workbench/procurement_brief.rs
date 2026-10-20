//! 供给任务的必要销售资料：当前生效版本、冻结责任行，不包含整单金额或附件。

use std::collections::HashMap;

use erp_core::ids::{SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderGoodsServiceLineRevision, SalesOrderRevisionLine,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::WorkflowAuthorizationPort;
use persistence_core::Executor;

use super::brief::{
    BRIEF_LINE_LIMIT, BriefLine, ObjectBriefSource, format_instant_due_label, format_quantity, line_title,
};
use super::dto::WorkItemFields;
use super::{ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchReadService};
use crate::Result;

#[derive(Debug, Clone)]
pub(super) struct ProcurementBrief {
    customer: String,
    lines: Vec<(String, BriefLine)>,
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 批量装载当前正式销售版本的供给资料，具体责任行在任务投影时收窄。
    ///
    /// # 参数
    /// * `orders` - 同批销售单。
    /// * `facts` - 已装载的对象事实。
    /// * `executor` - 当前读取快照。
    /// # 返回
    /// 为存在当前正式版本的销售单补齐必要资料。
    /// # 错误
    /// 版本、行或子类型仓储读取失败时传播错误。
    pub(super) async fn load_procurement_briefs(
        &self,
        orders: &[SalesOrder],
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = orders
            .iter()
            .filter_map(|order| order.current_revision_id())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let revisions = self.db.sales_order_revisions().find_revisions_by_ids(&ids, executor).await?;
        let mut lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&ids.iter().map(SalesOrderRevisionId::new).collect::<Vec<_>>(), executor)
            .await?;
        lines.sort_by_key(|line| line.line_no);
        let goods = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(
                &lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>(),
                executor,
            )
            .await?
            .into_iter()
            .map(|line| (line.revision_line_id.to_string(), line))
            .collect::<HashMap<_, _>>();
        for revision in revisions {
            let key = (ObjectKind::SalesOrder, revision.sales_order_id.to_string());
            if let Some(fact) = facts.get_mut(&key) {
                fact.display.procurement_brief = Some(ProcurementBrief {
                    customer: revision.customer_snapshot.customer_name,
                    lines: lines
                        .iter()
                        .filter(|line| line.sales_order_revision_id.as_ref() == revision.base.id)
                        .filter_map(|line| {
                            goods.get(&line.base.id).map(|goods| procurement_line(line, goods))
                        })
                        .collect(),
                });
            }
        }
        Ok(())
    }
}

fn procurement_line(
    line: &SalesOrderRevisionLine,
    goods: &SalesOrderGoodsServiceLineRevision,
) -> (String, BriefLine) {
    (
        line.sales_order_line_id.to_string(),
        BriefLine {
            title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
            quantity: Some(format_quantity(
                &goods.quantity,
                Some(line.unit_snapshot.as_deref().unwrap_or(&goods.base_unit_code)),
            )),
            due_label: Some(format_instant_due_label(goods.fulfillment_due_at)),
        },
    )
}

/// 每条任务单独收窄；同一销售单的其他任务不能复用本任务的责任行授权。
///
/// # 参数
/// * `fields` - 已授权任务及其冻结行。
/// * `fact` - 当前正式销售版本的资料。
/// # 返回
/// 替换任务简报，不回退整张销售单的审批简报。
/// # 错误
/// 不返回错误；缺正式资料时只保留任务自身摘要。
pub(super) fn apply_procurement_brief(fields: &mut WorkItemFields, fact: &WorkbenchObjectFact) {
    fields.brief_source = None;
    fields.counterparty_label = None;
    if let Some(source) = &fact.display.procurement_brief {
        fields.counterparty_label = Some(source.customer.clone());
        fields.brief_source =
            Some(scoped_brief(source, &fields.procurement_scope_ids, fields.impact_summary.as_deref()));
    }
}

fn scoped_brief(source: &ProcurementBrief, scope: &[String], impact: Option<&str>) -> ObjectBriefSource {
    let lines = source
        .lines
        .iter()
        .filter(|(id, _)| scope.contains(id))
        .map(|(_, line)| line.clone())
        .collect::<Vec<_>>();
    let more_count = u32::try_from(lines.len().saturating_sub(BRIEF_LINE_LIMIT)).unwrap_or(u32::MAX);
    ObjectBriefSource {
        customer: Some(source.customer.clone()),
        lines: lines.into_iter().take(BRIEF_LINE_LIMIT).collect(),
        more_count,
        list_summary: impact.unwrap_or("待分配供给").to_string(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_brief_only_contains_its_frozen_lines_and_no_sales_amounts() {
        let source = ProcurementBrief {
            customer: "客户".into(),
            lines: (1..=5)
                .map(|id| {
                    (
                        format!("line-{id}"),
                        BriefLine {
                            title: format!("商品{id}"),
                            quantity: Some("1 件".into()),
                            due_label: None,
                        },
                    )
                })
                .collect(),
        };
        let scope = vec!["line-2".into(), "line-4".into()];
        let brief = scoped_brief(&source, &scope, Some("2 行待分配"));
        assert_eq!(
            brief.lines.iter().map(|line| line.title.as_str()).collect::<Vec<_>>(),
            vec!["商品2", "商品4"]
        );
        assert_eq!(brief.more_count, 0);
        assert!(brief.amount_label.is_none());
        assert!(brief.extra_sections.is_empty());
        assert!(scoped_brief(&source, &[], None).lines.is_empty());
        let all = source.lines.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        let brief = scoped_brief(&source, &all, None);
        assert_eq!(brief.lines.len(), 3);
        assert_eq!(brief.more_count, 2);
    }
}
