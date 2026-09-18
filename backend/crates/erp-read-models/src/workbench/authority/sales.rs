//! Sales-order object facts for remaining-domain work-item authorization.

use std::collections::HashSet;

use erp_sales::entity::sales_order::{SalesOrder, SalesOrderSubmission, SubmissionStatus};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::ports::OrderTaskSource;
use persistence_core::Executor;

use super::amount::non_empty;
use super::{ObjectFact, ObjectFactMap, ObjectKind, object_ids};
use crate::errors::Result;

impl super::WorkItemFactsReader {
    /// Load sales-order identity, customer and impact for authorization facts.
    pub(in crate::workbench) async fn load_sales_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let orders = self.read_sales_orders(&ids, executor).await?;
        if orders.is_empty() {
            return Ok(());
        }
        let submissions = self.sales_submissions_for_orders(&orders, executor).await?;
        insert_sales_order_facts(facts, &orders, &submissions);
        Ok(())
    }

    /// 读取本批销售单的全部提交，避免按单 N+1。
    ///
    /// # 参数
    /// * `orders` - 本批销售单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回这些销售单上的全部提交。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(in crate::workbench) async fn sales_submissions_for_orders(
        &self,
        orders: &[SalesOrder],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderSubmission>> {
        let order_ids = orders.iter().map(|order| order.base.id.clone()).collect::<Vec<_>>();
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
        facts.insert((ObjectKind::SalesOrder, order.base.id.clone()), sales_order_fact(order, submissions));
    }
}

pub(in crate::workbench) fn sales_order_fact(
    order: &SalesOrder,
    submissions: &[SalesOrderSubmission],
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        order.base.id.clone(),
        format!("销售单 {}", order.order_no),
        order.stable.created_by.clone(),
    );
    fact.order_scope_source = Some(OrderTaskSource::Sales(order.base.id.clone()));
    let Some(submission) = preferred_submission(&order.base.id, submissions) else {
        return fact;
    };
    fact.counterparty_label = non_empty(&submission.customer_snapshot.customer_name);
    fact.impact_summary = Some(sales_order_impact(submission.business_type.label()).to_string());
    fact
}

/// 优先取该销售单上最新的审核中提交；没有审核中时回退最新提交。
///
/// # 参数
/// * `order_id` - 销售单 ID
/// * `submissions` - 本批销售提交
///
/// # 返回
/// 没有该单提交时返回 `None`。
///
/// # 错误
/// 无。
pub(in crate::workbench) fn preferred_submission<'a>(
    order_id: &str,
    submissions: &'a [SalesOrderSubmission],
) -> Option<&'a SalesOrderSubmission> {
    let mut for_order =
        submissions.iter().filter(|item| item.sales_order_id.to_string() == order_id).collect::<Vec<_>>();
    for_order.sort_by_key(|item| item.submission_no);
    for_order
        .iter()
        .copied()
        .rev()
        .find(|item| item.stable.status == SubmissionStatus::InReview)
        .or_else(|| for_order.last().copied())
}

/// 按业务性质给出销售单审批的业务影响。
///
/// # 参数
/// * `business_type_label` - 业务性质中文标签
///
/// # 返回
/// 卡券与实物使用不同后果文案。
///
/// # 错误
/// 无。
pub(in crate::workbench) fn sales_order_impact(business_type_label: &str) -> &'static str {
    if business_type_label == "卡券" {
        "不审批则卡券销售不能生效"
    } else {
        "不审批则销售单不能生效、不能履约"
    }
}
