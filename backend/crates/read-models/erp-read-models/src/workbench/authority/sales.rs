//! 销售单对象事实，供其余领域的工作项授权使用。

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
    // 为销售工作项事实取数时，读取销售单身份、客户与影响，供授权事实使用。
    //
    // 调用参数：
    // * `keys` - 本批对象键；没有销售单键时不读取。
    // * `facts` - 输出事实表。
    //
    // 成功时写入读到的销售单；没有提交时仍写入身份事实。
    //
    // 销售单或提交读取失败时返回错误。
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

    // 处理销售工作项事实的版本事实时，读取本批销售单的全部提交，避免按单 N+1。
    //
    // 调用参数：
    // * `orders` - 本批销售单
    //
    // 返回这些销售单上的全部提交。
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

// 组装销售工作项事实时，从已读销售单和提交构造权威事实。
//
// 调用参数：
// * `order` - 销售单。
// * `submissions` - 本批销售提交。
//
// 返回销售单号标题与创建人。优先挂上该单最新审核中提交的客户名和影响；没有审核中时回退最新提交；没有提交则只保留身份。
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
    fact.push_counterparty_id(order.customer_id.to_string());
    fact.push_counterparty_id(order.settlement_party_id.to_string());

    let Some(submission) = preferred_submission(&order.base.id, submissions) else {
        return fact;
    };
    fact.counterparty_label = non_empty(&submission.customer_snapshot.customer_name);
    fact.impact_summary = Some(sales_order_impact(submission.business_type.label()).to_string());
    fact
}

// 处理销售工作项事实的版本事实时，优先取该销售单上最新的审核中提交；没有审核中时回退最新提交。
//
// 调用参数：
// * `order_id` - 销售单 ID
// * `submissions` - 本批销售提交
//
// 没有该单提交时返回 `None`。
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

// 组装销售工作项事实时，按业务性质给出销售单审批的业务影响。
//
// 调用参数：
// * `business_type_label` - 业务性质中文标签
//
// 卡券与实物使用不同后果文案。
pub(in crate::workbench) fn sales_order_impact(business_type_label: &str) -> &'static str {
    if business_type_label == "卡券" {
        "不审批则卡券销售不能生效"
    } else {
        "不审批则销售单不能生效、不能履约"
    }
}
