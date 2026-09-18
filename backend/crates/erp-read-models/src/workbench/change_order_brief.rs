//! 销售变更与采购变更审批任务的事项简报装载。
//!
//! 变更简报以冻结基准版本与各次不可变提交为权威来源，展示表头前后值和行级
//! 数量、金额、单价、交期差异；不得从可变草稿或客户端计算变更事实。

mod mapping;
mod subjects;

use std::collections::{HashMap, HashSet};

use erp_core::ids::{PurchaseOrderRevisionId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_procurement::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseOrderRevision,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::entity::sales_order::SalesOrderRevision;
use erp_sales::entity::sales_review::{SalesChangeOrder, SalesChangeSubmission};
use erp_sales::repository::prelude::*;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
pub(super) use mapping::{change_diff_lines, purchase_order_submission_line_states};
use mapping::{
    purchase_base_line_states, purchase_change_brief_source, purchase_target_line_states,
    sales_base_line_states, sales_change_brief_source, sales_target_line_states,
};
use persistence_core::Executor;

use super::brief::BRIEF_LINE_LIMIT;
use super::{ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchReadService, object_ids};
use crate::errors::Result;

pub(super) type LineStateMap = HashMap<String, DiffLineState>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiffLineState {
    line_no: u32,
    title: String,
    amount: String,
    quantity: Option<String>,
    unit_price: Option<String>,
    due: Option<String>,
}

impl DiffLineState {
    /// 构造行比较状态。
    ///
    /// # 参数
    /// * `title` - 行标题
    /// * `amount` - 行金额
    ///
    /// # 返回
    /// 返回首行、可选字段全空的状态。
    ///
    /// # 错误
    /// 无。
    pub(super) fn new(title: String, amount: String) -> Self {
        Self { line_no: 1, title, amount, quantity: None, unit_price: None, due: None }
    }

    /// 设置业务行号。
    ///
    /// # 参数
    /// * `line_no` - 业务行号
    ///
    /// # 返回
    /// 返回更新后的状态。
    ///
    /// # 错误
    /// 无。
    pub(super) fn with_line_no(mut self, line_no: u32) -> Self {
        self.line_no = line_no;
        self
    }

    /// 设置数量、单价与交期。
    ///
    /// # 参数
    /// * `quantity` - 数量展示
    /// * `unit_price` - 单价展示
    /// * `due` - 交期展示
    ///
    /// # 返回
    /// 返回更新后的状态。
    ///
    /// # 错误
    /// 无。
    pub(super) fn with_details(
        mut self,
        quantity: Option<String>,
        unit_price: Option<String>,
        due: Option<String>,
    ) -> Self {
        self.quantity = quantity;
        self.unit_price = unit_price;
        self.due = due;
        self
    }
}

#[derive(Default)]
struct SalesChangeBriefContext {
    base_revisions: HashMap<String, SalesOrderRevision>,
    submissions: HashMap<String, SalesChangeSubmission>,
    base_lines: HashMap<String, LineStateMap>,
    target_lines: HashMap<String, LineStateMap>,
}

#[derive(Default)]
struct PurchaseChangeBriefContext {
    base_revisions: HashMap<String, PurchaseOrderRevision>,
    submissions: HashMap<String, PurchaseChangeSubmission>,
    base_lines: HashMap<String, LineStateMap>,
    target_lines: HashMap<String, LineStateMap>,
}

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 销售变更审批任务的对象事实：任务对象是变更单本身。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入来源销售单、原因、提交信息及冻结版本前后差异。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_sales_change_review_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesChangeOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let changes = self.facts_reader().read_sales_changes(&ids, executor).await?;
        if changes.is_empty() {
            return Ok(());
        }
        let sales_order_ids = changes.iter().map(|item| item.sales_order_id.to_string()).collect::<Vec<_>>();
        let sales_nos = self
            .facts_reader()
            .read_sales_orders(&sales_order_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect::<HashMap<_, _>>();
        let context = self.sales_change_brief_context(&changes, executor).await?;
        for change in changes {
            let sales_no = sales_nos.get(&change.sales_order_id.to_string()).cloned();
            let base = context.base_revisions.get(&change.base_revision_id.to_string());
            let submission =
                change.current_submission_id.as_ref().and_then(|id| context.submissions.get(&id.to_string()));
            let all_diff_lines = change_diff_lines(
                context.base_lines.get(&change.base_revision_id.to_string()),
                change
                    .current_submission_id
                    .as_ref()
                    .and_then(|id| context.target_lines.get(&id.to_string())),
            );
            let more_count = all_diff_lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            let mut lines = all_diff_lines;
            lines.truncate(BRIEF_LINE_LIMIT);
            let mut fact = WorkbenchObjectFact::from_authority(super::authority::changes::sales_change_fact(
                &change,
                sales_no.as_deref(),
                base,
                submission,
            ));
            fact.display.brief_source = Some(sales_change_brief_source(
                &change,
                sales_no.as_deref(),
                base,
                submission,
                lines,
                more_count,
            ));
            fact.display.root_document_id = change.sales_order_id.to_string();
            subjects::sales(&mut fact, &change, sales_no.as_deref(), &context);
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
    /// 成功时写入来源采购单、原因、提交信息及冻结版本前后差异。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_purchase_change_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchaseChangeOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let changes = self.facts_reader().read_purchase_changes(&ids, executor).await?;
        if changes.is_empty() {
            return Ok(());
        }
        let purchase_ids = changes.iter().map(|item| item.purchase_order_id.to_string()).collect::<Vec<_>>();
        let purchase_nos = self
            .facts_reader()
            .read_purchase_orders(&purchase_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect::<HashMap<_, _>>();
        let context = self.purchase_change_brief_context(&changes, executor).await?;
        for change in changes {
            let purchase_no = purchase_nos.get(&change.purchase_order_id.to_string()).cloned();
            let base = context.base_revisions.get(&change.base_revision_id.to_string());
            let submission =
                change.current_submission_id.as_ref().and_then(|id| context.submissions.get(&id.to_string()));
            let all_diff_lines = change_diff_lines(
                context.base_lines.get(&change.base_revision_id.to_string()),
                change
                    .current_submission_id
                    .as_ref()
                    .and_then(|id| context.target_lines.get(&id.to_string())),
            );
            let more_count = all_diff_lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            let mut lines = all_diff_lines;
            lines.truncate(BRIEF_LINE_LIMIT);
            let mut fact =
                WorkbenchObjectFact::from_authority(super::authority::changes::purchase_change_fact(
                    &change,
                    purchase_no.as_deref(),
                    base,
                    submission,
                ));
            fact.display.brief_source = Some(purchase_change_brief_source(
                &change,
                purchase_no.as_deref(),
                base,
                submission,
                lines,
                more_count,
            ));
            fact.display.root_document_id = change.purchase_order_id.to_string();
            subjects::purchase(&mut fact, &change, purchase_no.as_deref(), &context);
            facts.insert((ObjectKind::PurchaseChangeOrder, change.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 批量读取销售变更的冻结基准、全部提交与两侧明细。
    async fn sales_change_brief_context(
        &self,
        changes: &[SalesChangeOrder],
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeBriefContext> {
        let base_ids = changes.iter().map(|change| change.base_revision_id.to_string()).collect::<Vec<_>>();
        let base_revisions = self.facts_reader().read_sales_revisions(&base_ids, executor).await?;
        let change_ids = changes
            .iter()
            .map(|change| erp_core::ids::SalesChangeOrderId::new(change.base.id.clone()))
            .collect::<Vec<_>>();
        let submissions =
            self.db.sales_change_submissions().list_by_change_orders(&change_ids, executor).await?;
        let submission_ids = submissions
            .iter()
            .map(|row| erp_core::ids::SalesChangeSubmissionId::new(row.base.id.clone()))
            .collect::<Vec<_>>();
        let revision_ids = base_revisions
            .iter()
            .map(|revision| SalesOrderRevisionId::new(revision.base.id.clone()))
            .collect::<Vec<_>>();
        let revision_lines =
            self.db.sales_order_revision_lines().list_lines_by_revisions(&revision_ids, executor).await?;
        let revision_line_ids = revision_lines
            .iter()
            .map(|line| SalesOrderRevisionLineId::new(line.base.id.clone()))
            .collect::<Vec<_>>();
        let goods_lines = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, executor)
            .await?;
        let voucher_lines = self
            .db
            .sales_order_voucher_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, executor)
            .await?;
        let target_lines = self
            .db
            .sales_change_submission_lines()
            .list_lines_by_submissions(&submission_ids, executor)
            .await?;
        Ok(SalesChangeBriefContext {
            base_revisions: base_revisions
                .into_iter()
                .map(|revision| (revision.base.id.clone(), revision))
                .collect(),
            submissions: submissions
                .into_iter()
                .map(|submission| (submission.base.id.clone(), submission))
                .collect(),
            base_lines: sales_base_line_states(&revision_lines, &goods_lines, &voucher_lines),
            target_lines: sales_target_line_states(&target_lines),
        })
    }

    /// 批量读取采购变更的冻结基准、全部提交与两侧明细。
    async fn purchase_change_brief_context(
        &self,
        changes: &[PurchaseChangeOrder],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeBriefContext> {
        let base_ids = changes.iter().map(|change| change.base_revision_id.to_string()).collect::<Vec<_>>();
        let base_revisions = self.facts_reader().read_purchase_revisions(&base_ids, executor).await?;
        let change_ids = changes
            .iter()
            .map(|change| erp_core::ids::PurchaseChangeOrderId::new(change.base.id.clone()))
            .collect::<Vec<_>>();
        let submissions =
            self.db.purchase_change_submissions().list_by_change_orders(&change_ids, executor).await?;
        let submission_ids = submissions
            .iter()
            .map(|row| erp_core::ids::PurchaseChangeSubmissionId::new(row.base.id.clone()))
            .collect::<Vec<_>>();
        let revision_ids = base_revisions
            .iter()
            .map(|revision| PurchaseOrderRevisionId::new(revision.base.id.clone()))
            .collect::<Vec<_>>();
        let revision_lines = self
            .db
            .purchase_order_revision_lines()
            .find_lines_by_revision_ids(&revision_ids, executor)
            .await?;
        let target_lines = self
            .db
            .purchase_change_submission_lines()
            .find_lines_by_submission_ids(&submission_ids, executor)
            .await?;
        Ok(PurchaseChangeBriefContext {
            base_revisions: base_revisions
                .into_iter()
                .map(|revision| (revision.base.id.clone(), revision))
                .collect(),
            submissions: submissions
                .into_iter()
                .map(|submission| (submission.base.id.clone(), submission))
                .collect(),
            base_lines: purchase_base_line_states(&revision_lines),
            target_lines: purchase_target_line_states(&target_lines),
        })
    }
}
