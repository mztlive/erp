//! 采购二次确认任务的事项简报装载。
//!
//! 批量读取销售单、提交和提交行，生成队列只读简报，不进入确认表单。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, Executor, SalesOrderExt, SalesReviewExt};
use mongodb::bson::doc;

use super::brief::{brief_line_from_submission, object_brief_source, BriefLine, ObjectBriefSource};
use super::presentation::{procurement_impact_summary, sales_order_object_label};
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

/// 采购确认在对象事实中的展示包。
#[derive(Debug, Clone)]
pub(super) struct ProcurementConfirmationDisplay {
    pub label: String,
    pub counterparty: Option<String>,
    pub impact: String,
    pub brief: ObjectBriefSource,
}

impl WorkItemService {
    /// 读取采购确认对应的销售单身份、客户、提交行和简报。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入销售单号、客户、影响和事项简报；关联对象缺失时仍保留最小标题。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_procurement_confirmation_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ProcurementConfirmation);
        if ids.is_empty() {
            return Ok(());
        }
        let confirmations = self
            .db
            .procurement_confirmations()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?;
        let displays = self
            .procurement_confirmation_displays(&confirmations, executor)
            .await?;
        for confirmation in confirmations {
            let display = displays.get(&confirmation.base.id);
            facts.insert(
                (ObjectKind::ProcurementConfirmation, confirmation.base.id.clone()),
                ObjectFact {
                    root_document_id: confirmation.sales_order_id.to_string(),
                    label: display
                        .map(|item| item.label.clone())
                        .unwrap_or_else(|| sales_order_object_label("")),
                    created_by: confirmation.stable.created_by,
                    subject_versions: Vec::new(),
                    counterparty_label: display.and_then(|item| item.counterparty.clone()),
                    impact_summary: display.map(|item| item.impact.clone()),
                    brief_source: display.map(|item| item.brief.clone()),
                },
            );
        }
        Ok(())
    }

    /// 按确认记录批量解析销售单号、客户、提交行和提交人。
    ///
    /// # 参数
    /// * `confirmations` - 本批采购确认
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回确认 ID 到展示字段的映射。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn procurement_confirmation_displays(
        &self,
        confirmations: &[entities::sales_review::ProcurementConfirmation],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ProcurementConfirmationDisplay>> {
        let order_ids = confirmations
            .iter()
            .map(|item| item.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let submission_ids = confirmations
            .iter()
            .map(|item| item.submission_id.to_string())
            .collect::<Vec<_>>();
        let orders = self
            .db
            .sales_orders()
            .find_many(doc! { "id": { "$in": order_ids } }, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id.clone(), order.order_no))
            .collect::<HashMap<_, _>>();
        let submissions = self
            .db
            .sales_order_submissions()
            .find_many(doc! { "id": { "$in": submission_ids.clone() } }, executor)
            .await?
            .into_iter()
            .map(|submission| (submission.base.id.clone(), submission))
            .collect::<HashMap<_, _>>();
        let lines_by_submission = self.submission_brief_lines(&submission_ids, executor).await?;
        let submitter_names = self
            .account_names(
                &submissions
                    .values()
                    .map(|submission| submission.submitted_by.clone())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        Ok(confirmations
            .iter()
            .map(|confirmation| {
                let submission = submissions.get(&confirmation.submission_id.to_string());
                let lines = lines_by_submission
                    .get(&confirmation.submission_id.to_string())
                    .cloned()
                    .unwrap_or_default();
                let customer = submission
                    .map(|item| item.customer_snapshot.customer_name.clone())
                    .filter(|name| !name.trim().is_empty());
                let submitter_name =
                    submission.and_then(|item| submitter_names.get(&item.submitted_by).cloned());
                let brief = object_brief_source(
                    customer.clone(),
                    submission.map(|item| &item.gross_amount),
                    lines,
                    submitter_name,
                );
                (
                    confirmation.base.id.clone(),
                    ProcurementConfirmationDisplay {
                        label: sales_order_object_label(
                            orders
                                .get(&confirmation.sales_order_id.to_string())
                                .map(String::as_str)
                                .unwrap_or(""),
                        ),
                        counterparty: customer,
                        impact: procurement_impact_summary(
                            Some(brief.lines.len() + brief.more_count as usize).filter(|count| *count > 0),
                            submission.map(|item| &item.gross_amount),
                        ),
                        brief,
                    },
                )
            })
            .collect())
    }

    /// 读取本批销售提交行并转成按提交分组的简报行。
    ///
    /// # 参数
    /// * `submission_ids` - 提交 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回提交 ID 到已按行号排序的简报行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn submission_brief_lines(
        &self,
        submission_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<BriefLine>>> {
        if submission_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut grouped: HashMap<String, Vec<(u32, BriefLine)>> = HashMap::new();
        for line in self
            .db
            .sales_order_submission_lines()
            .find_many(doc! { "submission_id": { "$in": submission_ids } }, executor)
            .await?
        {
            grouped.entry(line.submission_id.to_string()).or_default().push((
                line.line_no,
                brief_line_from_submission(
                    &line.item_name_snapshot,
                    line.spec_snapshot.as_deref(),
                    line.quantity.as_ref(),
                    line.unit_snapshot.as_deref(),
                    line.fulfillment_due_at,
                ),
            ));
        }
        Ok(grouped
            .into_iter()
            .map(|(submission_id, mut rows)| {
                rows.sort_by_key(|(line_no, _)| *line_no);
                (submission_id, rows.into_iter().map(|(_, line)| line).collect())
            })
            .collect())
    }

    /// 按账号 ID 批量读取姓名。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID，可重复
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回账号 ID 到姓名的映射；空名丢弃。
    ///
    /// # 错误
    /// 账号查询失败时返回仓储错误。
    async fn account_names(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let mut ids = account_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .db
            .accounts()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
            .into_iter()
            .filter_map(|account| {
                let name = account.name.trim();
                if name.is_empty() || name == "当前处理人" {
                    None
                } else {
                    Some((account.base.id, name.to_string()))
                }
            })
            .collect())
    }
}
