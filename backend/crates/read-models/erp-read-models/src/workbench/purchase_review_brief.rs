//! 采购单财务审核任务的事项简报装载。
//!
//! 批量读取采购提交、提交行、供应商快照和来源销售单，生成队列只读简报。
//! 正式通过/驳回仍在采购单审核页提交。

mod mapping;

use std::collections::{HashMap, HashSet};

use erp_procurement::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use mapping::{
    assemble_purchase_review_displays, insert_purchase_order_facts, purchase_brief_lines,
    source_sales_orders_by_purchase_order,
};
use persistence_core::Executor;

use super::authority::purchase::{purchase_facts, purchase_line_counts};
use super::brief::BriefLine;
use super::change_order_brief::{LineStateMap, purchase_order_submission_line_states};
use super::{ObjectKind, WorkbenchObjectFactMap, WorkbenchReadService};
use crate::errors::Result;

/// 采购审核在对象事实中按提交版本保存的展示包。
#[derive(Debug, Clone)]
struct PurchaseReviewDisplay {
    subject_version: Option<u32>,
    purchase_order_id: String,
    counterparty: Option<String>,
    impact: String,
    brief: super::brief::ObjectBriefSource,
}

#[derive(Default)]
struct PurchaseSubmissionLineContext {
    line_counts: HashMap<String, usize>,
    brief_lines: HashMap<String, Vec<BriefLine>>,
    line_states: HashMap<String, LineStateMap>,
}

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 读取采购单身份，并按提交版本写入供应商、金额、付款条件和明细简报。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入采购单号；关联提交缺失时仍保留最小标题。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_purchase_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let orders = self.facts_reader().purchase_orders_for_keys(keys, executor).await?;
        if orders.is_empty() {
            return Ok(());
        }
        let (displays, authority) = self.purchase_review_displays(&orders, executor).await?;
        insert_purchase_order_facts(facts, &orders, &displays, &authority);
        Ok(())
    }

    /// 按采购单批量解析提交、销售单号、提交人和简报。
    ///
    /// # 参数
    /// * `orders` - 本批采购单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回提交 ID 到展示字段的映射。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn purchase_review_displays(
        &self,
        orders: &[PurchaseOrder],
        executor: &mut dyn Executor,
    ) -> Result<(HashMap<String, PurchaseReviewDisplay>, erp_workflow::ports::ObjectFactMap)> {
        let submissions = self.facts_reader().purchase_submissions_for_orders(orders, executor).await?;
        let sales_order_nos = self.sales_order_numbers_for_orders(orders, executor).await?;
        let line_context = self.purchase_submission_brief_lines(&submissions, executor).await?;
        let submitter_names = HashMap::<String, String>::new();
        let _ = executor;
        let authority = purchase_facts(orders, &submissions, &line_context.line_counts);
        Ok((
            assemble_purchase_review_displays(
                &submissions,
                &source_sales_orders_by_purchase_order(orders, &sales_order_nos),
                &line_context.brief_lines,
                &line_context.line_states,
                &submitter_names,
            ),
            authority,
        ))
    }

    /// 读取本批采购单来源销售单号。
    ///
    /// # 参数
    /// * `orders` - 本批采购单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回销售单 ID 到单号的映射。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn sales_order_numbers_for_orders(
        &self,
        orders: &[PurchaseOrder],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let sales_order_ids = orders.iter().map(|order| order.sales_order_id.to_string()).collect::<Vec<_>>();
        if sales_order_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .facts_reader()
            .read_sales_orders(&sales_order_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id.clone(), order.order_no))
            .collect())
    }

    /// 读取本批采购提交行并转成按提交分组的简报行。
    ///
    /// # 参数
    /// * `submissions` - 本批采购提交
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回提交 ID 到已按行号排序的简报行和稳定比较状态。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn purchase_submission_brief_lines(
        &self,
        submissions: &[PurchaseOrderSubmission],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseSubmissionLineContext> {
        let lines = self.facts_reader().purchase_submission_lines(submissions, executor).await?;
        let line_counts = purchase_line_counts(&lines);
        let line_states = purchase_order_submission_line_states(&lines);
        let mut grouped: HashMap<String, Vec<(u32, PurchaseOrderSubmissionLine)>> = HashMap::new();
        for line in lines {
            grouped
                .entry(line.purchase_order_submission_id.to_string())
                .or_default()
                .push((line.line_no, line));
        }
        let brief_lines = grouped
            .into_iter()
            .map(|(submission_id, mut rows)| {
                rows.sort_by_key(|(line_no, _)| *line_no);
                (submission_id, purchase_brief_lines(rows.into_iter().map(|(_, line)| line)))
            })
            .collect();
        Ok(PurchaseSubmissionLineContext { line_counts, brief_lines, line_states })
    }
}
