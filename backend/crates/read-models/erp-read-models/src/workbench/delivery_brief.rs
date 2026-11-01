//! 发货工作台只读资料；数量限定本批次，地址密文不进入公共简报。
use std::collections::HashMap;

use erp_core::ids::SalesOrderRevisionLineId;
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryLine, DeliveryState};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::WorkflowAuthorizationPort;
use persistence_core::Executor;
use serde::Serialize;

use super::brief::{
    BriefSection, format_instant_date, format_instant_datetime, push_document_section, push_section,
};
use super::{ObjectKind, WorkbenchObjectFactMap, WorkbenchReadService};
use crate::errors::Result;

/// 发货批次明细。日期来自实际关联版本，缺失时保持空值。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryTaskLine {
    pub id: String,
    pub title: String,
    pub specification: Option<String>,
    pub unit: Option<String>,
    pub quantity: String,
    pub shipped_quantity: String,
    pub pending_quantity: String,
    pub customer_due_date: Option<String>,
    pub expected_delivery_date: Option<String>,
    pub tracking: Vec<String>,
}

#[derive(Default)]
struct SalesLine {
    title: String,
    specification: Option<String>,
    unit: Option<String>,
    due: Option<String>,
}

#[derive(Default)]
struct DeliveryContext {
    sales: HashMap<String, SalesLine>,
    sections: HashMap<String, Vec<BriefSection>>,
    expected: HashMap<String, (String, String)>,
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 为授权发货对象批量补齐来源资料和本批次数量。
    ///
    /// # 参数
    /// * `rows` / `lines` - 当前页实际发货表头和行
    /// * `facts` - 已装载的任务对象
    /// * `executor` - 当前查询执行器
    /// # 返回
    /// 原地追加只读展示，不改变责任、权限或业务状态。
    /// # 错误
    /// 仓储读取失败时返回对应错误。
    pub(super) async fn enrich_delivery_briefs(
        &self,
        rows: &[Delivery],
        lines: &[DeliveryLine],
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut context = DeliveryContext::default();
        self.delivery_sales_context(rows, &mut context, executor).await?;
        self.delivery_purchase_context(rows, &mut context, executor).await?;
        self.delivery_expected_dates(lines, &mut context, executor).await?;
        for row in rows {
            let Some(fact) = facts.get_mut(&(ObjectKind::Delivery, row.base.id.clone())) else { continue };
            let mut visible =
                lines.iter().filter(|l| l.delivery_id.as_ref() == row.base.id).collect::<Vec<_>>();
            visible.sort_by_key(|line| line.line_no);
            fact.display.delivery_lines = visible.iter().map(|line| task_line(row, line, &context)).collect();
            let source = fact.display.brief_source.get_or_insert_with(Default::default);
            source
                .extra_sections
                .extend(context.sections.get(&row.sales_order_id.to_string()).cloned().unwrap_or_default());
            if let Some(id) = &row.purchase_order_id {
                source.extra_sections.extend(context.sections.get(id.as_ref()).cloned().unwrap_or_default());
            }
            append_delivery_sections(&mut source.extra_sections, row, &fact.display.delivery_lines);
        }
        Ok(())
    }

    async fn delivery_sales_context(
        &self,
        rows: &[Delivery],
        context: &mut DeliveryContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = rows.iter().map(|row| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let orders = self.db.sales_orders().list_active_by_ids(&ids, executor).await?;
        let owners = orders.iter().map(|row| row.sales_owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.load_account_names(&owners, executor).await?;
        let revisions = orders
            .iter()
            .filter_map(|row| row.stable.current_revision_id.clone().map(Into::into))
            .collect::<Vec<_>>();
        let lines =
            self.db.sales_order_revision_lines().list_lines_by_revisions(&revisions, executor).await?;
        let line_ids =
            lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
        let goods = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&line_ids, executor)
            .await?;
        let dates = goods
            .into_iter()
            .map(|line| (line.revision_line_id.to_string(), format_instant_date(line.fulfillment_due_at)))
            .collect::<HashMap<_, _>>();
        for line in lines {
            context.sales.insert(
                line.sales_order_line_id.to_string(),
                SalesLine {
                    due: dates.get(&line.base.id).cloned(),
                    title: line.item_name_snapshot,
                    specification: line.spec_snapshot,
                    unit: line.unit_snapshot,
                },
            );
        }
        for order in orders {
            let sections = context.sections.entry(order.base.id).or_default();
            push_section(
                sections,
                "销售负责人",
                names.get(&order.sales_owner_user_id).map(String::as_str),
                false,
            );
        }
        Ok(())
    }

    async fn delivery_purchase_context(
        &self,
        rows: &[Delivery],
        context: &mut DeliveryContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = rows
            .iter()
            .filter_map(|row| row.purchase_order_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let orders = self.db.purchase_orders().list_active_by_ids(&ids, executor).await?;
        let revision_ids =
            orders.iter().filter_map(|row| row.stable.current_revision_id.clone()).collect::<Vec<_>>();
        let revisions = self
            .db
            .purchase_order_revisions()
            .list_active_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|row| (row.base.id.clone(), row))
            .collect::<HashMap<_, _>>();
        for order in orders {
            let sections = context.sections.entry(order.base.id.clone()).or_default();
            push_document_section(sections, "关联采购单", Some(&order.purchase_no), Some(&order.base.id));
            if let Some(revision) = order.stable.current_revision_id.as_ref().and_then(|id| revisions.get(id))
                && revision.purchase_order_id.as_ref() == order.base.id
            {
                push_section(sections, "供应商", Some(&revision.supplier_snapshot.supplier_name), false);
            }
        }
        Ok(())
    }

    async fn delivery_expected_dates(
        &self,
        lines: &[DeliveryLine],
        context: &mut DeliveryContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = lines
            .iter()
            .filter_map(|line| line.purchase_line_sales_allocation_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let allocations =
            self.db.purchase_line_sales_allocations().list_active_by_ids(&ids, executor).await?;
        let line_ids =
            allocations.iter().map(|row| row.purchase_order_revision_line_id.to_string()).collect::<Vec<_>>();
        let purchase_lines =
            self.db.purchase_order_revision_lines().list_active_by_ids(&line_ids, executor).await?;
        let revision_ids =
            purchase_lines.iter().map(|row| row.purchase_order_revision_id.to_string()).collect::<Vec<_>>();
        let revisions = self
            .db
            .purchase_order_revisions()
            .list_active_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|row| (row.base.id, row.purchase_order_id.to_string()))
            .collect::<HashMap<_, _>>();
        let dates = purchase_lines
            .into_iter()
            .filter_map(|line| {
                Some((
                    line.base.id,
                    (
                        revisions.get(line.purchase_order_revision_id.as_ref())?.clone(),
                        line.expected_delivery_date?.to_string(),
                    ),
                ))
            })
            .collect::<HashMap<_, _>>();
        for allocation in allocations {
            if let Some(date) = dates.get(allocation.purchase_order_revision_line_id.as_ref()) {
                context.expected.insert(allocation.base.id, date.clone());
            }
        }
        Ok(())
    }
}

fn task_line(row: &Delivery, line: &DeliveryLine, context: &DeliveryContext) -> DeliveryTaskLine {
    let sales = context.sales.get(line.sales_order_line_id.as_ref());
    let expected = line
        .purchase_line_sales_allocation_id
        .as_ref()
        .and_then(|id| context.expected.get(id.as_ref()))
        .filter(|(purchase, _)| row.purchase_order_id.as_ref().is_some_and(|id| id.as_ref() == purchase));
    let quantity = line.quantity.to_string();
    DeliveryTaskLine {
        id: line.base.id.clone(),
        title: sales.map(|line| line.title.clone()).unwrap_or_else(|| format!("发货明细 {}", line.line_no)),
        specification: sales.and_then(|line| line.specification.clone()),
        unit: sales.and_then(|line| line.unit.clone()),
        shipped_quantity: if matches!(row.status, DeliveryState::Shipped | DeliveryState::Signed) {
            quantity.clone()
        } else {
            "0".into()
        },
        pending_quantity: if row.status == DeliveryState::Draft { quantity.clone() } else { "0".into() },
        quantity,
        customer_due_date: sales.and_then(|line| line.due.clone()),
        expected_delivery_date: expected.map(|(_, date)| date.clone()),
        tracking: row
            .tracking_entries
            .iter()
            .filter(|entry| entry.sales_order_line_id == line.sales_order_line_id)
            .map(|entry| {
                entry
                    .carrier
                    .as_ref()
                    .map(|carrier| format!("{carrier} · {}", entry.tracking_no))
                    .unwrap_or_else(|| entry.tracking_no.clone())
            })
            .collect(),
    }
}

fn append_delivery_sections(sections: &mut Vec<BriefSection>, row: &Delivery, lines: &[DeliveryTaskLine]) {
    let due = lines.iter().filter_map(|line| line.customer_due_date.as_deref()).min();
    let expected = lines.iter().filter_map(|line| line.expected_delivery_date.as_deref()).min();
    push_section(sections, "最早客户交付日", due, false);
    push_section(sections, "最早预计交付日", expected, false);
    push_section(
        sections,
        "收货信息",
        Some(if row.address_snapshot_encrypted.as_ref().is_some_and(|value| !value.trim().is_empty()) {
            "已保存收货资料，当前工作台暂不支持查看，请联系履约责任人核对"
        } else {
            "未登记，请联系销售负责人核对本次收货人、电话和地址"
        }),
        false,
    );
    push_section(sections, "批次明细", Some(&format!("{} 项商品", lines.len())), false);
    if let Some(at) = row.shipped_at {
        push_section(sections, "最近发货记录", Some(&format_instant_datetime(at)), false);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entity<T: serde::de::DeserializeOwned>(fields: serde_json::Value) -> T {
        let mut value = serde_json::to_value(entity_core::BaseModel::new("line-id".into())).unwrap();
        value.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    fn delivery() -> Delivery {
        entity(
            json!({"delivery_no":"DN-1","delivery_type":"SUPPLIER_DIRECT","sales_order_id":"sales","purchase_order_id":"purchase","status":"DRAFT",
        "tracking_entries":[{"sales_order_line_id":"sales-line","tracking_no":"SF-1","carrier":"顺丰"},{"sales_order_line_id":"another-line","tracking_no":"OTHER"}],"address_snapshot_encrypted":"private-cipher"}),
        )
    }

    fn line() -> DeliveryLine {
        entity(
            json!({"delivery_id":"delivery","line_no":1,"sales_order_line_id":"sales-line","quantity":"2.5","purchase_line_sales_allocation_id":"allocation"}),
        )
    }

    #[test]
    fn batch_quantities_follow_actual_delivery_state() {
        let mut row = delivery();
        for (status, shipped, pending) in [
            (DeliveryState::Draft, "0", "2.5"),
            (DeliveryState::Shipped, "2.5", "0"),
            (DeliveryState::Signed, "2.5", "0"),
            (DeliveryState::Reversed, "0", "0"),
        ] {
            row.status = status;
            let view = task_line(&row, &line(), &DeliveryContext::default());
            assert_eq!(view.quantity, "2.5");
            assert_eq!(view.shipped_quantity, shipped);
            assert_eq!(view.pending_quantity, pending);
            assert_eq!(view.tracking, vec!["顺丰 · SF-1"]);
            assert_eq!(view.title, "发货明细 1");
            assert_eq!(view.customer_due_date, None);
        }
    }

    #[test]
    fn dates_keep_distinct_sources_and_reject_wrong_purchase() {
        let mut context = DeliveryContext::default();
        context.sales.insert(
            "sales-line".into(),
            SalesLine { title: "果干".into(), due: Some("2026-10-12".into()), ..Default::default() },
        );
        context.expected.insert("allocation".into(), ("purchase".into(), "2026-10-11".into()));
        let row = delivery();
        let view = task_line(&row, &line(), &context);
        assert_eq!(view.customer_due_date.as_deref(), Some("2026-10-12"));
        assert_eq!(view.expected_delivery_date.as_deref(), Some("2026-10-11"));
        context.expected.insert("allocation".into(), ("other".into(), "2026-10-01".into()));
        assert_eq!(task_line(&row, &line(), &context).expected_delivery_date, None);
    }

    #[test]
    fn summary_uses_earliest_line_date_and_never_returns_private_address() {
        let mut row = delivery();
        let mut first = task_line(&row, &line(), &DeliveryContext::default());
        first.customer_due_date = Some("2026-10-12".into());
        let mut second = first.clone();
        second.customer_due_date = Some("2026-10-10".into());
        let mut fields = Vec::new();
        append_delivery_sections(&mut fields, &row, &[first, second]);
        assert!(fields.iter().any(|field| field.label == "最早客户交付日" && field.value == "2026-10-10"));
        assert!(!serde_json::to_string(&fields).unwrap().contains("private-cipher"));
        row.address_snapshot_encrypted = None;
        fields.clear();
        append_delivery_sections(&mut fields, &row, &[]);
        assert!(fields.iter().any(|field| field.label == "收货信息" && field.value.starts_with("未登记")));
        assert!(!fields.iter().any(|field| field.label == "最早客户交付日"));
    }
}
