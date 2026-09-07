//! Sales-order object facts for remaining-domain work-item authorization.

use std::collections::HashSet;

use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;
use {
    erp_sales::entity::sales_order::SalesOrder, erp_sales::entity::sales_order::SalesOrderSubmission,
    erp_sales::entity::sales_order::SubmissionStatus,
};

use super::amount::non_empty;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind};
use crate::errors::Result;

impl crate::work_item::ProcessObjectFacts {
    /// Load sales-order identity, customer and impact for authorization facts.
    pub(super) async fn load_sales_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let orders = self.db.sales_orders().list_active_by_ids(&ids, executor).await?;
        if orders.is_empty() {
            return Ok(());
        }
        let submissions = self.sales_submissions_for_orders(&orders, executor).await?;
        insert_sales_order_facts(facts, &orders, &submissions);
        Ok(())
    }

    async fn sales_submissions_for_orders(
        &self,
        orders: &[SalesOrder],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderSubmission>> {
        let order_ids = orders
            .iter()
            .map(|order| order.base.id.clone())
            .collect::<Vec<_>>();
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .sales_order_submissions()
            .list_work_item_brief_submissions_by_orders(&order_ids, executor)
            .await?)
    }
}

fn insert_sales_order_facts(
    facts: &mut ObjectFactMap,
    orders: &[SalesOrder],
    submissions: &[SalesOrderSubmission],
) {
    for order in orders {
        facts.insert(
            (ObjectKind::SalesOrder, order.base.id.clone()),
            sales_order_fact(order, submissions),
        );
    }
}

fn sales_order_fact(order: &SalesOrder, submissions: &[SalesOrderSubmission]) -> ObjectFact {
    let mut fact = ObjectFact::new(
        order.base.id.clone(),
        format!("销售单 {}", order.order_no),
        order.stable.created_by.clone(),
    );
    let Some(submission) = preferred_submission(&order.base.id, submissions) else {
        return fact;
    };
    fact.counterparty_label = non_empty(&submission.customer_snapshot.customer_name);
    fact.impact_summary = Some(sales_order_impact(submission.business_type.label()).to_string());
    fact
}

fn preferred_submission<'a>(
    order_id: &str,
    submissions: &'a [SalesOrderSubmission],
) -> Option<&'a SalesOrderSubmission> {
    let mut for_order = submissions
        .iter()
        .filter(|item| item.sales_order_id.to_string() == order_id)
        .collect::<Vec<_>>();
    for_order.sort_by_key(|item| item.submission_no);
    for_order
        .iter()
        .copied()
        .rev()
        .find(|item| item.stable.status == SubmissionStatus::InReview)
        .or_else(|| for_order.last().copied())
}

fn sales_order_impact(business_type_label: &str) -> &'static str {
    if business_type_label == "卡券" {
        "不审批则卡券销售不能生效"
    } else {
        "不审批则销售单不能生效、不能履约"
    }
}
