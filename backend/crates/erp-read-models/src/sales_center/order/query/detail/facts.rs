//! 销售详情的独立事实并行读取与冻结提交组装。
use std::collections::HashMap;

use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
use erp_finance::entity::receivable::SalesOrderReceivableAmountSummary;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::dto::sales_order::{RevisionView, SalesOrderLineView, SubmissionView, WorkingCopyView};
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderSubmission, SalesOrderSubmissionLine, WorkingPurpose,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::mapper::submission_view;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::NoTransaction;

use super::SalesOrderReadService;
use crate::{Error, Result};

impl SalesOrderReadService {
    /// 并行加载详情第一组事实：稳定行、提交、版本视图与采购数。
    ///
    /// # 参数
    /// * `order_id` - 销售单稳定主键
    ///
    /// # 返回
    /// 返回稳定行、提交头、版本视图与有效采购数。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn load_detail_batch_a(
        &self,
        order_id: SalesOrderId,
    ) -> Result<(Vec<SalesOrderLine>, Vec<SalesOrderSubmission>, Vec<RevisionView>, u64)> {
        let lines_db = self.db.clone();
        let lines_id = order_id.clone();
        let submissions_db = self.db.clone();
        let submissions_id = order_id.clone();
        let revisions_service = self.sales();
        let revisions_id = order_id.clone();
        let count_db = self.db.clone();
        let count_id = order_id;
        tokio::try_join!(
            async move {
                lines_db
                    .sales_order_lines()
                    .list_lines_by_order(&lines_id, &mut NoTransaction)
                    .await
                    .map_err(Error::from)
            },
            async move {
                submissions_db
                    .sales_order_submissions()
                    .list_by_order_newest_first(&submissions_id, &mut NoTransaction)
                    .await
                    .map_err(Error::from)
            },
            async move { revisions_service.load_revision_views(&revisions_id).await.map_err(Error::from) },
            async move {
                count_db
                    .purchase_orders()
                    .count_active_by_sales_order(&count_id, &mut NoTransaction)
                    .await
                    .map_err(Error::from)
            },
        )
    }

    /// 并行加载详情第二组事实：草稿视图、应收摘要与审批绑定。
    ///
    /// # 参数
    /// * `order_id` - 销售单稳定主键
    /// * `document_id` - 审批绑定查询用的单据主键文本
    ///
    /// # 返回
    /// 返回草稿视图、应收金额摘要与审批定义绑定。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn load_detail_batch_b(
        &self,
        order_id: SalesOrderId,
        document_id: String,
    ) -> Result<(SalesOrderReceivableAmountSummary, Option<ApprovalDefinitionBinding>, Option<WorkingCopyView>)>
    {
        let summary_db = self.db.clone();
        let summary_id = order_id.clone();
        let binding_db = self.db.clone();
        let working_db = self.db.clone();
        let working_id = order_id;
        let working_sales = self.sales();
        tokio::try_join!(
            async move {
                summary_db
                    .receivable_accounts()
                    .sales_order_amount_summary(&summary_id, &mut NoTransaction)
                    .await
                    .map_err(Error::from)
            },
            async move {
                // 保留原详情对绑定读取错误回退为空的合同。
                Ok(find_approval_binding(&binding_db, &document_id, &mut NoTransaction).await.ok().flatten())
            },
            async move {
                let working_copy = working_db
                    .sales_order_working_copies()
                    .find_active_by_order_and_purpose(
                        &working_id,
                        WorkingPurpose::FirstSubmission,
                        &mut NoTransaction,
                    )
                    .await?;
                match working_copy {
                    Some(copy) => Ok(Some(working_sales.working_copy_view(&copy).await?)),
                    None => Ok(None),
                }
            },
        )
    }

    /// 并行加载相互独立的详情事实，再按提交身份批量读取提交行。
    ///
    /// # 参数
    /// * `order` - 已授权的销售稳定单
    ///
    /// # 返回
    /// 返回稳定行、草稿视图、提交、版本视图、采购数、应收摘要与审批绑定。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅限 `NoTransaction` 展示查询并行；`&mut Executor` 不可并发借用。
    pub(super) async fn load_detail_facts(&self, order: &SalesOrder) -> Result<DetailFacts> {
        let order_id = SalesOrderId::new(order.base.id.clone());
        let document_id = order.base.id.clone();
        let (
            (stable_lines, submissions, revisions, purchase_order_count),
            (receivable_summary, binding, working_copy_view),
        ) = tokio::try_join!(
            self.load_detail_batch_a(order_id.clone()),
            self.load_detail_batch_b(order_id, document_id),
        )?;
        let submission_ids = submissions
            .iter()
            .map(|item| SalesOrderSubmissionId::new(item.base.id.clone()))
            .collect::<Vec<_>>();
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&submission_ids, &mut NoTransaction)
            .await?;
        Ok(DetailFacts {
            stable_lines: stable_lines
                .into_iter()
                .map(|line| SalesOrderLineView {
                    id: line.base.id,
                    line_no: line.line_no,
                    line_status: line.line_status,
                })
                .collect(),
            working_copy_view,
            submissions: assemble_submission_views(submissions, submission_lines),
            revisions,
            purchase_order_count,
            receivable_summary,
            binding,
        })
    }
}

/// 已加载的详情事实；提交行已按提交身份归组并按行号排序。
pub(super) struct DetailFacts {
    pub(super) stable_lines: Vec<SalesOrderLineView>,
    pub(super) working_copy_view: Option<WorkingCopyView>,
    pub(super) submissions: Vec<SubmissionView>,
    pub(super) revisions: Vec<RevisionView>,
    pub(super) purchase_order_count: u64,
    pub(super) receivable_summary: SalesOrderReceivableAmountSummary,
    pub(super) binding: Option<ApprovalDefinitionBinding>,
}

/// 组装提交视图，纯内存分组排序不触库。
///
/// # 参数
/// * `submissions` - 已并行加载的提交头，新提交在前
/// * `lines` - 已按提交批量加载的提交行
///
/// # 返回
/// 返回按提交分组、行号排序的提交视图。
///
/// # 错误
/// 无。
fn assemble_submission_views(
    submissions: Vec<SalesOrderSubmission>,
    lines: Vec<SalesOrderSubmissionLine>,
) -> Vec<SubmissionView> {
    let mut lines_by_submission: HashMap<String, Vec<SalesOrderSubmissionLine>> = HashMap::new();
    for line in lines {
        lines_by_submission.entry(line.submission_id.to_string()).or_default().push(line);
    }
    submissions
        .into_iter()
        .map(|submission| {
            let mut lines = lines_by_submission.remove(&submission.base.id).unwrap_or_default();
            lines.sort_by_key(|line| line.line_no);
            submission_view(submission, lines)
        })
        .collect()
}
