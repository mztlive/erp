//! 销售变更与采购变更审批任务的事项简报装载。
//!
//! 变更简报以冻结基准版本与当前不可变提交为权威来源，展示表头前后值和行级
//! 数量、金额、单价、交期差异；不得从可变草稿或客户端计算变更事实。

use std::collections::{HashMap, HashSet};

use database::{Executor, PurchaseOrderExt, SalesOrderExt, SalesReviewExt};
use entities::{
    ids::{PurchaseOrderRevisionId, SalesOrderRevisionId, SalesOrderRevisionLineId},
    purchase_order::{
        PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseOrderRevision,
        PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine,
    },
    sales_order::{
        SalesOrderGoodsServiceLineRevision, SalesOrderRevision, SalesOrderRevisionLine,
        SalesOrderVoucherLineRevision,
    },
    sales_review::{SalesChangeOrder, SalesChangeSubmission, SalesChangeSubmissionLine},
};

use super::brief::{
    format_instant_date, format_instant_datetime, format_quantity, join_list_summary, line_title, non_empty,
    push_document_section, push_section, BriefLine, ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::presentation::format_yuan;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
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

impl WorkItemService {
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
        let context = self.sales_change_brief_context(&changes, executor).await?;
        for change in changes {
            let sales_no = sales_nos.get(&change.sales_order_id.to_string()).cloned();
            let base = context.base_revisions.get(&change.base_revision_id.to_string());
            let submission = change
                .current_submission_id
                .as_ref()
                .and_then(|id| context.submissions.get(&id.to_string()));
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
            let mut fact = ObjectFact::new(
                change.base.id.clone(),
                sales_no
                    .as_deref()
                    .map(|no| format!("销售变更单 {no}"))
                    .unwrap_or_else(|| "销售变更单（来源单号待补全）".to_string()),
                change.stable.created_by.clone(),
            );
            fact.counterparty_label = submission
                .map(|item| item.customer_snapshot.customer_name.clone())
                .or_else(|| base.map(|item| item.customer_snapshot.customer_name.clone()));
            fact.impact_summary = Some("不审批则销售变更不能生效；通过后按目标提交形成新版本".to_string());
            fact.brief_source = Some(sales_change_brief_source(
                &change,
                sales_no.as_deref(),
                base,
                submission,
                lines,
                more_count,
            ));
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
        let context = self.purchase_change_brief_context(&changes, executor).await?;
        for change in changes {
            let purchase_no = purchase_nos.get(&change.purchase_order_id.to_string()).cloned();
            let base = context.base_revisions.get(&change.base_revision_id.to_string());
            let submission = change
                .current_submission_id
                .as_ref()
                .and_then(|id| context.submissions.get(&id.to_string()));
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
            let mut fact = ObjectFact::new(
                change.base.id.clone(),
                purchase_no
                    .as_deref()
                    .map(|no| format!("采购变更单 {no}"))
                    .unwrap_or_else(|| "采购变更单（来源单号待补全）".to_string()),
                change.stable.created_by.clone(),
            );
            fact.counterparty_label = submission
                .map(|item| item.supplier_snapshot.supplier_name.clone())
                .or_else(|| base.map(|item| item.supplier_snapshot.supplier_name.clone()));
            fact.impact_summary = Some("不审批则采购变更不能生效；通过后按目标提交形成新版本".to_string());
            fact.brief_source = Some(purchase_change_brief_source(
                &change,
                purchase_no.as_deref(),
                base,
                submission,
                lines,
                more_count,
            ));
            facts.insert((ObjectKind::PurchaseChangeOrder, change.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 批量读取销售变更的冻结基准、当前提交与两侧明细。
    async fn sales_change_brief_context(
        &self,
        changes: &[SalesChangeOrder],
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeBriefContext> {
        let base_ids = changes
            .iter()
            .map(|change| change.base_revision_id.to_string())
            .collect::<Vec<_>>();
        let base_revisions = self
            .db
            .sales_order_revisions()
            .list_work_item_brief_entities_by_ids(&base_ids, executor)
            .await?;
        let submission_ids = changes
            .iter()
            .filter_map(|change| change.current_submission_id.clone())
            .collect::<Vec<_>>();
        let submissions = self
            .db
            .sales_change_submissions()
            .list_work_item_brief_entities_by_ids(
                &submission_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let revision_ids = base_revisions
            .iter()
            .map(|revision| SalesOrderRevisionId::new(revision.base.id.clone()))
            .collect::<Vec<_>>();
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&revision_ids, executor)
            .await?;
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

    /// 批量读取采购变更的冻结基准、当前提交与两侧明细。
    async fn purchase_change_brief_context(
        &self,
        changes: &[PurchaseChangeOrder],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeBriefContext> {
        let base_ids = changes
            .iter()
            .map(|change| change.base_revision_id.to_string())
            .collect::<Vec<_>>();
        let base_revisions = self
            .db
            .purchase_order_revisions()
            .list_work_item_brief_entities_by_ids(&base_ids, executor)
            .await?;
        let submission_ids = changes
            .iter()
            .filter_map(|change| change.current_submission_id.clone())
            .collect::<Vec<_>>();
        let submissions = self
            .db
            .purchase_change_submissions()
            .list_work_item_brief_entities_by_ids(
                &submission_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                executor,
            )
            .await?;
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

/// 组装销售变更的结构化简报。
fn sales_change_brief_source(
    change: &SalesChangeOrder,
    sales_no: Option<&str>,
    base: Option<&SalesOrderRevision>,
    submission: Option<&SalesChangeSubmission>,
    lines: Vec<BriefLine>,
    more_count: u32,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    let sales_order_id = change.sales_order_id.to_string();
    push_document_section(&mut sections, "来源销售单", sales_no, Some(&sales_order_id));
    push_section(&mut sections, "变更类型", Some(change.change_type.label()), false);
    push_section(&mut sections, "原因", non_empty(&change.reason).as_deref(), false);
    if let Some(submission) = submission {
        let submission_no = format!("第 {} 次提交", submission.submission_no);
        let submitted_at = format_instant_datetime(submission.submitted_at);
        push_section(&mut sections, "目标提交", Some(&submission_no), false);
        push_section(&mut sections, "提交时间", Some(&submitted_at), false);
    }
    push_sales_header_comparisons(&mut sections, base, submission);
    push_line_difference_summary(
        &mut sections,
        base.is_some() && submission.is_some(),
        &lines,
        more_count,
    );
    let amount_label = submission
        .map(|item| format_yuan(&item.gross_amount))
        .or_else(|| base.map(|item| format_yuan(&item.gross_amount)));
    let amount_change = amount_comparison(
        base.map(|item| &item.gross_amount),
        submission.map(|item| &item.gross_amount),
    );
    ObjectBriefSource {
        customer: submission
            .map(|item| item.customer_snapshot.customer_name.clone())
            .or_else(|| base.map(|item| item.customer_snapshot.customer_name.clone())),
        amount_label,
        extra_sections: sections,
        list_summary: join_list_summary([
            sales_no.map(|no| format!("销售单 {no}")),
            Some(change.change_type.label().to_string()),
            amount_change,
            non_empty(&change.reason),
        ]),
        lines,
        more_count,
        submitter_name: submission.map(|item| item.submitted_by.clone()),
    }
}

/// 组装采购变更的结构化简报。
fn purchase_change_brief_source(
    change: &PurchaseChangeOrder,
    purchase_no: Option<&str>,
    base: Option<&PurchaseOrderRevision>,
    submission: Option<&PurchaseChangeSubmission>,
    lines: Vec<BriefLine>,
    more_count: u32,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    let purchase_order_id = change.purchase_order_id.to_string();
    push_document_section(&mut sections, "来源采购单", purchase_no, Some(&purchase_order_id));
    push_section(&mut sections, "原因", non_empty(&change.reason).as_deref(), false);
    if let Some(submission) = submission {
        push_section(
            &mut sections,
            "目标提交",
            Some(submission.submission_no.as_str()),
            false,
        );
        let submitted_at = submission.submitted_at.map(format_instant_datetime);
        push_section(&mut sections, "提交时间", submitted_at.as_deref(), false);
        push_section(
            &mut sections,
            "采购类型",
            Some(submission.purchase_type.label()),
            false,
        );
        push_section(
            &mut sections,
            "履约责任",
            Some(submission.fulfillment_responsibility.label()),
            false,
        );
    }
    push_purchase_header_comparisons(&mut sections, base, submission);
    push_line_difference_summary(
        &mut sections,
        base.is_some() && submission.is_some(),
        &lines,
        more_count,
    );
    let amount_label = submission
        .map(|item| format_yuan(&item.gross_amount))
        .or_else(|| base.map(|item| format_yuan(&item.gross_amount)));
    let amount_change = amount_comparison(
        base.map(|item| &item.gross_amount),
        submission.map(|item| &item.gross_amount),
    );
    ObjectBriefSource {
        customer: None,
        amount_label,
        extra_sections: sections,
        list_summary: join_list_summary([
            purchase_no.map(|no| format!("采购单 {no}")),
            submission.map(|item| item.supplier_snapshot.supplier_name.clone()),
            amount_change,
            non_empty(&change.reason),
        ]),
        lines,
        more_count,
        submitter_name: submission.and_then(|item| item.submitted_by.clone()),
    }
}

/// 追加销售变更表头的前后值。
fn push_sales_header_comparisons(
    sections: &mut Vec<super::brief::BriefSection>,
    base: Option<&SalesOrderRevision>,
    submission: Option<&SalesChangeSubmission>,
) {
    push_amount_comparison(
        sections,
        "含税金额（前→后）",
        base.map(|item| &item.gross_amount),
        submission.map(|item| &item.gross_amount),
    );
    push_amount_comparison(
        sections,
        "不含税金额（前→后）",
        base.map(|item| &item.net_amount),
        submission.map(|item| &item.net_amount),
    );
    push_amount_comparison(
        sections,
        "税额（前→后）",
        base.map(|item| &item.tax_amount),
        submission.map(|item| &item.tax_amount),
    );
    let customer = text_comparison(
        base.map(|item| item.customer_snapshot.customer_name.as_str()),
        submission.map(|item| item.customer_snapshot.customer_name.as_str()),
    );
    push_section(sections, "客户（前→后）", customer.as_deref(), false);
    let payment = text_comparison(
        base.map(|item| item.payment_term_snapshot.payment_term_name.as_str()),
        submission.map(|item| item.payment_term_snapshot.payment_term_name.as_str()),
    );
    push_section(sections, "付款条件（前→后）", payment.as_deref(), false);
    let invoice = text_comparison(
        base.map(|item| item.invoice_requirement_snapshot.invoice_type.as_str()),
        submission.map(|item| item.invoice_requirement_snapshot.invoice_type.as_str()),
    );
    push_section(sections, "开票要求（前→后）", invoice.as_deref(), false);
}

/// 追加采购变更表头的前后值。
fn push_purchase_header_comparisons(
    sections: &mut Vec<super::brief::BriefSection>,
    base: Option<&PurchaseOrderRevision>,
    submission: Option<&PurchaseChangeSubmission>,
) {
    push_amount_comparison(
        sections,
        "含税金额（前→后）",
        base.map(|item| &item.gross_amount),
        submission.map(|item| &item.gross_amount),
    );
    push_amount_comparison(
        sections,
        "不含税金额（前→后）",
        base.map(|item| &item.net_amount),
        submission.map(|item| &item.net_amount),
    );
    push_amount_comparison(
        sections,
        "税额（前→后）",
        base.map(|item| &item.tax_amount),
        submission.map(|item| &item.tax_amount),
    );
    let supplier = text_comparison(
        base.map(|item| item.supplier_snapshot.supplier_name.as_str()),
        submission.map(|item| item.supplier_snapshot.supplier_name.as_str()),
    );
    push_section(sections, "供应商（前→后）", supplier.as_deref(), false);
    let base_payment = base.map(|item| purchase_payment_label(&item.payment_term_snapshot));
    let target_payment = submission.map(|item| purchase_payment_label(&item.payment_term_snapshot));
    let payment = text_comparison(base_payment.as_deref(), target_payment.as_deref());
    push_section(sections, "付款条件（前→后）", payment.as_deref(), false);
}

/// 追加金额前后值。
fn push_amount_comparison(
    sections: &mut Vec<super::brief::BriefSection>,
    label: &str,
    before: Option<&entities::money::Amount>,
    after: Option<&entities::money::Amount>,
) {
    let comparison = amount_comparison(before, after);
    push_section(sections, label, comparison.as_deref(), true);
}

/// 返回金额前后值。
fn amount_comparison(
    before: Option<&entities::money::Amount>,
    after: Option<&entities::money::Amount>,
) -> Option<String> {
    text_comparison(
        before.map(format_yuan).as_deref(),
        after.map(format_yuan).as_deref(),
    )
}

/// 返回文本前后值；任一侧缺失时使用明确占位。
fn text_comparison(before: Option<&str>, after: Option<&str>) -> Option<String> {
    if before.is_none() && after.is_none() {
        return None;
    }
    Some(format!(
        "{} → {}",
        before
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("未记录"),
        after
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("未记录")
    ))
}

/// 追加行级差异数量或明确无差异结论。
fn push_line_difference_summary(
    sections: &mut Vec<super::brief::BriefSection>,
    has_both_sides: bool,
    visible_lines: &[BriefLine],
    more_count: u32,
) {
    if !has_both_sides {
        push_section(sections, "明细差异", Some("冻结基准或目标提交缺失"), false);
    } else if visible_lines.is_empty() && more_count == 0 {
        push_section(
            sections,
            "明细差异",
            Some("数量、金额、单价与交期均未变化"),
            false,
        );
    } else {
        let count = visible_lines.len() + more_count as usize;
        let label = format!("{count} 行发生变化");
        push_section(sections, "明细差异", Some(&label), false);
    }
}

/// 将采购付款条件快照转成可读标签。
fn purchase_payment_label(snapshot: &entities::purchase_order::PaymentTermSnapshot) -> String {
    if snapshot.prepay_gate {
        format!("{}（先款后货）", snapshot.payment_term_code)
    } else {
        snapshot.payment_term_code.clone()
    }
}

/// 把销售基准版本行转成按稳定销售行分组的比较状态。
fn sales_base_line_states(
    lines: &[SalesOrderRevisionLine],
    goods_lines: &[SalesOrderGoodsServiceLineRevision],
    voucher_lines: &[SalesOrderVoucherLineRevision],
) -> HashMap<String, LineStateMap> {
    let goods = goods_lines
        .iter()
        .map(|line| (line.revision_line_id.to_string(), line))
        .collect::<HashMap<_, _>>();
    let vouchers = voucher_lines
        .iter()
        .map(|line| (line.revision_line_id.to_string(), line))
        .collect::<HashMap<_, _>>();
    let mut grouped: HashMap<String, LineStateMap> = HashMap::new();
    for line in lines {
        let goods_line = goods.get(&line.base.id);
        let voucher_line = vouchers.get(&line.base.id);
        let quantity = goods_line
            .map(|item| {
                format_quantity(
                    &item.quantity,
                    line.unit_snapshot
                        .as_deref()
                        .or(Some(item.base_unit_code.as_str())),
                )
            })
            .or_else(|| voucher_line.map(|item| format!("{} 张", item.card_count)));
        let unit_price = goods_line
            .map(|item| format_unit_price(item.unit_price_gross.to_decimal()))
            .or_else(|| voucher_line.map(|item| format_unit_price(item.unit_price_gross.to_decimal())));
        let due = goods_line.map(|item| format_instant_date(item.fulfillment_due_at));
        grouped
            .entry(line.sales_order_revision_id.to_string())
            .or_default()
            .insert(
                line.sales_order_line_id.to_string(),
                DiffLineState {
                    line_no: line.line_no,
                    title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
                    amount: format_yuan(&line.gross_amount),
                    quantity,
                    unit_price,
                    due,
                },
            );
    }
    grouped
}

/// 把销售变更目标提交行转成按稳定销售行分组的比较状态。
fn sales_target_line_states(lines: &[SalesChangeSubmissionLine]) -> HashMap<String, LineStateMap> {
    let mut grouped: HashMap<String, LineStateMap> = HashMap::new();
    for line in lines {
        let quantity = line
            .quantity
            .as_ref()
            .map(|value| {
                format_quantity(
                    value,
                    line.unit_snapshot.as_deref().or(line.base_unit_code.as_deref()),
                )
            })
            .or_else(|| line.card_count.map(|count| format!("{count} 张")));
        let unit_price = line
            .unit_price_gross
            .map(|value| format_unit_price(value.to_decimal()));
        grouped
            .entry(line.sales_change_submission_id.to_string())
            .or_default()
            .insert(
                line.sales_order_line_id.to_string(),
                DiffLineState {
                    line_no: line.line_no,
                    title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
                    amount: format_yuan(&line.gross_amount),
                    quantity,
                    unit_price,
                    due: line.fulfillment_due_at.map(format_instant_date),
                },
            );
    }
    grouped
}

/// 把采购基准版本行转成按稳定来源行分组的比较状态。
fn purchase_base_line_states(lines: &[PurchaseOrderRevisionLine]) -> HashMap<String, LineStateMap> {
    let mut grouped: HashMap<String, LineStateMap> = HashMap::new();
    for line in lines {
        grouped
            .entry(line.purchase_order_revision_id.to_string())
            .or_default()
            .insert(
                purchase_line_key(
                    line.procurement_confirmation_line_id.as_ref().map(AsRef::as_ref),
                    line.sales_order_line_id.as_ref().map(AsRef::as_ref),
                    line.sku_id.as_ref().map(AsRef::as_ref),
                    line.line_no,
                ),
                purchase_revision_line_state(line),
            );
    }
    grouped
}

/// 把采购变更目标提交行转成按稳定来源行分组的比较状态。
fn purchase_target_line_states(lines: &[PurchaseChangeSubmissionLine]) -> HashMap<String, LineStateMap> {
    let mut grouped: HashMap<String, LineStateMap> = HashMap::new();
    for line in lines {
        grouped
            .entry(line.purchase_change_submission_id.to_string())
            .or_default()
            .insert(
                purchase_line_key(
                    line.procurement_confirmation_line_id.as_ref().map(AsRef::as_ref),
                    line.sales_order_line_id.as_ref().map(AsRef::as_ref),
                    line.sku_id.as_ref().map(AsRef::as_ref),
                    line.line_no,
                ),
                purchase_submission_line_state(line),
            );
    }
    grouped
}

/// 把采购单不可变提交行转成按稳定来源行分组的比较状态。
pub(super) fn purchase_order_submission_line_states(
    lines: &[PurchaseOrderSubmissionLine],
) -> HashMap<String, LineStateMap> {
    let mut grouped: HashMap<String, LineStateMap> = HashMap::new();
    for line in lines {
        grouped
            .entry(line.purchase_order_submission_id.to_string())
            .or_default()
            .insert(
                purchase_line_key(
                    line.procurement_confirmation_line_id.as_ref().map(AsRef::as_ref),
                    line.sales_order_line_id.as_ref().map(AsRef::as_ref),
                    line.sku_id.as_ref().map(AsRef::as_ref),
                    line.line_no,
                ),
                DiffLineState {
                    line_no: line.line_no,
                    title: line_title(
                        line.product_name_snapshot.as_deref().unwrap_or("未命名采购明细"),
                        line.specification_snapshot.as_deref(),
                    ),
                    amount: format_yuan(&line.gross_amount),
                    quantity: line
                        .quantity
                        .as_ref()
                        .map(|value| format_quantity(value, line.base_unit_code.as_deref())),
                    unit_price: line
                        .unit_cost_gross
                        .map(|value| format_unit_price(value.to_decimal())),
                    due: line.expected_delivery_date.map(|value| value.to_string()),
                },
            );
    }
    grouped
}

/// 返回采购变更两侧可复用的稳定行键。
///
/// 正式商品行优先使用采购确认行或销售稳定行；只有旧数据缺少来源身份时才把
/// SKU 与行号组合，物流费等无商品身份的行最后回退行号。该键仅用于服务端匹配，
/// 不得上屏。
fn purchase_line_key(
    procurement_confirmation_line_id: Option<&str>,
    sales_order_line_id: Option<&str>,
    sku_id: Option<&str>,
    line_no: u32,
) -> String {
    if let Some(id) = procurement_confirmation_line_id.filter(|id| !id.trim().is_empty()) {
        return format!("procurement:{id}");
    }
    if let Some(id) = sales_order_line_id.filter(|id| !id.trim().is_empty()) {
        return format!("sales:{id}");
    }
    if let Some(id) = sku_id.filter(|id| !id.trim().is_empty()) {
        return format!("sku:{id}:line:{line_no}");
    }
    format!("line:{line_no}")
}

/// 组装采购基准版本行的比较状态。
fn purchase_revision_line_state(line: &PurchaseOrderRevisionLine) -> DiffLineState {
    DiffLineState {
        line_no: line.line_no,
        title: line_title(
            line.product_name_snapshot.as_deref().unwrap_or("未命名采购明细"),
            line.specification_snapshot.as_deref(),
        ),
        amount: format_yuan(&line.gross_amount),
        quantity: line
            .quantity
            .as_ref()
            .map(|value| format_quantity(value, line.base_unit_code.as_deref())),
        unit_price: line
            .unit_cost_gross
            .map(|value| format_unit_price(value.to_decimal())),
        due: line.expected_delivery_date.map(|value| value.to_string()),
    }
}

/// 组装采购目标提交行的比较状态。
fn purchase_submission_line_state(line: &PurchaseChangeSubmissionLine) -> DiffLineState {
    DiffLineState {
        line_no: line.line_no,
        title: line_title(
            line.product_name_snapshot.as_deref().unwrap_or("未命名采购明细"),
            line.specification_snapshot.as_deref(),
        ),
        amount: format_yuan(&line.gross_amount),
        quantity: line
            .quantity
            .as_ref()
            .map(|value| format_quantity(value, line.base_unit_code.as_deref())),
        unit_price: line
            .unit_cost_gross
            .map(|value| format_unit_price(value.to_decimal())),
        due: line.expected_delivery_date.map(|value| value.to_string()),
    }
}

/// 把单位价格格式化为人民币展示。
fn format_unit_price(value: rust_decimal::Decimal) -> String {
    format!("¥{}", value.normalize())
}

/// 计算两侧行状态中的实际变更行。
pub(super) fn change_diff_lines(
    before: Option<&LineStateMap>,
    after: Option<&LineStateMap>,
) -> Vec<BriefLine> {
    let before = before.cloned().unwrap_or_default();
    let after = after.cloned().unwrap_or_default();
    let mut keys = before.keys().chain(after.keys()).cloned().collect::<HashSet<_>>();
    let mut pairs = keys
        .drain()
        .map(|key| (before.get(&key).cloned(), after.get(&key).cloned()))
        .collect::<Vec<_>>();
    pairs.sort_by_key(|(before, after)| {
        after
            .as_ref()
            .or(before.as_ref())
            .map(|line| line.line_no)
            .unwrap_or(u32::MAX)
    });
    pairs
        .into_iter()
        .filter_map(|(before, after)| changed_line_brief(before.as_ref(), after.as_ref()))
        .collect()
}

/// 把单行前后状态转成变更简报行；完全一致时不上屏。
fn changed_line_brief(before: Option<&DiffLineState>, after: Option<&DiffLineState>) -> Option<BriefLine> {
    if before == after {
        return None;
    }
    let chosen = after.or(before)?;
    let mut title = match (before, after) {
        (None, Some(item)) => format!("{} · 新增 {}", item.title, item.amount),
        (Some(item), None) => format!("{} · 删除 {}", item.title, item.amount),
        (Some(before), Some(after)) => {
            let title = if before.title == after.title {
                after.title.clone()
            } else {
                format!("{} → {}", before.title, after.title)
            };
            if before.amount == after.amount {
                title
            } else {
                format!("{title} · {} → {}", before.amount, after.amount)
            }
        }
        (None, None) => return None,
    };
    if title.trim().is_empty() {
        title = chosen.title.clone();
    }
    let mut quantity_parts = Vec::new();
    if before.and_then(|item| item.quantity.as_deref()) != after.and_then(|item| item.quantity.as_deref()) {
        quantity_parts.push(format!(
            "数量 {}",
            text_comparison(
                before.and_then(|item| item.quantity.as_deref()),
                after.and_then(|item| item.quantity.as_deref()),
            )?
        ));
    }
    if before.and_then(|item| item.unit_price.as_deref()) != after.and_then(|item| item.unit_price.as_deref())
    {
        quantity_parts.push(format!(
            "单价 {}",
            text_comparison(
                before.and_then(|item| item.unit_price.as_deref()),
                after.and_then(|item| item.unit_price.as_deref()),
            )?
        ));
    }
    if let (Some(before), Some(after)) = (before, after) {
        if before.line_no != after.line_no {
            quantity_parts.push(format!("行号 {} → {}", before.line_no, after.line_no));
        }
    }
    let due_label =
        if before.and_then(|item| item.due.as_deref()) != after.and_then(|item| item.due.as_deref()) {
            text_comparison(
                before.and_then(|item| item.due.as_deref()),
                after.and_then(|item| item.due.as_deref()),
            )
            .map(|value| format!("交期 {value}"))
        } else {
            None
        };
    Some(BriefLine {
        title,
        quantity: (!quantity_parts.is_empty()).then(|| quantity_parts.join(" · ")),
        due_label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(
        title: &str,
        amount: &str,
        quantity: Option<&str>,
        price: Option<&str>,
        due: Option<&str>,
    ) -> DiffLineState {
        DiffLineState {
            line_no: 1,
            title: title.to_string(),
            amount: amount.to_string(),
            quantity: quantity.map(str::to_string),
            unit_price: price.map(str::to_string),
            due: due.map(str::to_string),
        }
    }

    #[test]
    fn change_list_summary_joins_source_and_reason() {
        let summary = join_list_summary([
            Some("销售单 SO-1".into()),
            Some("数量".into()),
            Some("客户减量".into()),
        ]);
        assert_eq!(summary, "销售单 SO-1 · 数量 · 客户减量");
    }

    #[test]
    fn changed_line_exposes_amount_quantity_price_and_due() {
        let before = state(
            "办公纸 A4",
            "¥1,000",
            Some("10 箱"),
            Some("¥100"),
            Some("2026-09-01"),
        );
        let after = state(
            "办公纸 A4",
            "¥1,080",
            Some("12 箱"),
            Some("¥90"),
            Some("2026-09-03"),
        );
        let line = changed_line_brief(Some(&before), Some(&after)).unwrap();
        assert!(line.title.contains("¥1,000 → ¥1,080"));
        assert!(line.quantity.as_deref().unwrap().contains("数量 10 箱 → 12 箱"));
        assert!(line.quantity.as_deref().unwrap().contains("单价 ¥100 → ¥90"));
        assert_eq!(line.due_label.as_deref(), Some("交期 2026-09-01 → 2026-09-03"));
    }

    #[test]
    fn unchanged_line_is_omitted() {
        let line = state("服务费", "¥500", None, None, None);
        assert!(changed_line_brief(Some(&line), Some(&line)).is_none());
    }

    #[test]
    fn purchase_line_key_prefers_stable_source_over_position() {
        assert_eq!(
            purchase_line_key(
                Some("confirmation-line-1"),
                Some("sales-line-1"),
                Some("sku-1"),
                7
            ),
            purchase_line_key(
                Some("confirmation-line-1"),
                Some("sales-line-1"),
                Some("sku-1"),
                2
            ),
        );
    }

    #[test]
    fn changed_line_exposes_position_change() {
        let before = state("服务费", "¥500", None, None, None);
        let mut after = before.clone();
        after.line_no = 3;
        let line = changed_line_brief(Some(&before), Some(&after)).unwrap();
        assert_eq!(line.quantity.as_deref(), Some("行号 1 → 3"));
    }
}
