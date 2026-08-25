//! 销售单审批任务的事项简报装载。
//!
//! 批量读取销售提交、提交行和结构化快照，生成工作台只读简报。采购确认销售单
//! 与卡券销售单共用本装载；正式通过/驳回仍走单据审批命令。

use std::collections::{HashMap, HashSet};

use database::{Executor, SalesOrderExt};
use entities::ids::SalesOrderSubmissionId;
use entities::money::Amount;
#[cfg(test)]
use entities::money::Quantity;
use entities::sales_order::{SalesOrder, SalesOrderSubmission, SalesOrderSubmissionLine, SubmissionStatus};

use super::brief::{
    format_instant_due_label, format_quantity, join_list_summary, line_title, non_empty, push_section,
    BriefLine, BriefSection, ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::presentation::format_yuan;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

impl WorkItemService {
    /// 读取销售单身份，并按当前审核中提交写入客户、金额、付款条件和明细简报。
    ///
    /// 没有提交时仍保留最小标题，避免任务因对象事实缺失被授权过滤丢弃。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入销售单号和简报。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
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
        let orders = self
            .db
            .sales_orders()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        if orders.is_empty() {
            return Ok(());
        }
        let submissions = self.sales_submissions_for_orders(&orders, executor).await?;
        let lines_by_submission = self.sales_submission_brief_lines(&submissions, executor).await?;
        insert_sales_order_facts(facts, &orders, &submissions, &lines_by_submission);
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

    /// 读取本批销售提交行并转成按提交分组的简报行。
    ///
    /// # 参数
    /// * `submissions` - 本批销售提交
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回提交 ID 到已按行号排序的简报行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn sales_submission_brief_lines(
        &self,
        submissions: &[SalesOrderSubmission],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<BriefLine>>> {
        let submission_ids = submissions
            .iter()
            .map(|item| SalesOrderSubmissionId::new(item.base.id.clone()))
            .collect::<Vec<_>>();
        if submission_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut grouped: HashMap<String, Vec<(u32, SalesOrderSubmissionLine)>> = HashMap::new();
        for line in self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&submission_ids, executor)
            .await?
        {
            grouped
                .entry(line.submission_id.to_string())
                .or_default()
                .push((line.line_no, line));
        }
        Ok(grouped
            .into_iter()
            .map(|(submission_id, mut rows)| {
                rows.sort_by_key(|(line_no, _)| *line_no);
                (
                    submission_id,
                    rows.into_iter()
                        .map(|(_, line)| sales_brief_line(&line))
                        .collect(),
                )
            })
            .collect())
    }
}

/// 把销售单事实和按提交准备好的简报写入对象事实表。
///
/// # 参数
/// * `facts` - 输出的对象事实表
/// * `orders` - 本批销售单
/// * `submissions` - 本批销售提交
/// * `lines_by_submission` - 提交 ID 到简报行
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn insert_sales_order_facts(
    facts: &mut ObjectFactMap,
    orders: &[SalesOrder],
    submissions: &[SalesOrderSubmission],
    lines_by_submission: &HashMap<String, Vec<BriefLine>>,
) {
    for order in orders {
        facts.insert(
            (ObjectKind::SalesOrder, order.base.id.clone()),
            sales_order_fact(order, submissions, lines_by_submission),
        );
    }
}

/// 为单张销售单生成对象事实，优先挂上当前审核中提交的简报。
///
/// # 参数
/// * `order` - 销售单
/// * `submissions` - 本批销售提交
/// * `lines_by_submission` - 提交 ID 到简报行
///
/// # 返回
/// 返回带标题和提交级简报的对象事实。
///
/// # 错误
/// 无。
fn sales_order_fact(
    order: &SalesOrder,
    submissions: &[SalesOrderSubmission],
    lines_by_submission: &HashMap<String, Vec<BriefLine>>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        order.base.id.clone(),
        format!("销售单 {}", order.order_no),
        order.stable.created_by.clone(),
    );
    let Some(submission) = preferred_submission(&order.base.id, submissions) else {
        return fact;
    };
    let lines = lines_by_submission
        .get(&submission.base.id)
        .cloned()
        .unwrap_or_default();
    let brief = sales_order_brief_source(submission, lines);
    fact.counterparty_label = non_empty(&submission.customer_snapshot.customer_name);
    fact.impact_summary = Some(sales_order_impact(submission.business_type.label()).to_string());
    fact.brief_source = Some(brief);
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

/// 组装销售单事项简报键值和列表一行摘要。
///
/// # 参数
/// * `submission` - 当前展示的销售提交
/// * `lines` - 已按行号排好的简报行
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
fn sales_order_brief_source(submission: &SalesOrderSubmission, lines: Vec<BriefLine>) -> ObjectBriefSource {
    let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    let mut visible = lines;
    visible.truncate(BRIEF_LINE_LIMIT);
    let customer = non_empty(&submission.customer_snapshot.customer_name);
    ObjectBriefSource {
        customer: customer.clone(),
        amount_label: Some(format_yuan(&submission.gross_amount)),
        extra_sections: sales_order_sections(submission),
        list_summary: sales_order_list_summary(
            customer.as_deref(),
            &submission.gross_amount,
            &submission.tax_amount,
            non_empty(&submission.payment_term_snapshot.payment_term_name).as_deref(),
            &visible,
            more_count,
        ),
        lines: visible,
        more_count,
        submitter_name: non_empty(&submission.submitted_by),
    }
}

/// 生成销售单简报键值，空值不上屏。
///
/// # 参数
/// * `submission` - 销售提交
///
/// # 返回
/// 返回业务性质、结算主体、合同、金额和付款条件等段。
///
/// # 错误
/// 无。
fn sales_order_sections(submission: &SalesOrderSubmission) -> Vec<BriefSection> {
    let mut sections = Vec::new();
    push_section(
        &mut sections,
        "业务性质",
        Some(submission.business_type.label()),
        false,
    );
    push_section(
        &mut sections,
        "结算主体",
        submission
            .settlement_party_snapshot
            .as_ref()
            .map(|item| item.settlement_party_name.as_str()),
        false,
    );
    push_section(
        &mut sections,
        "合同",
        submission
            .contract_snapshot
            .as_ref()
            .map(|item| item.contract_no.as_str()),
        false,
    );
    push_section(
        &mut sections,
        "含税金额",
        Some(format_yuan(&submission.gross_amount)).as_deref(),
        true,
    );
    push_section(
        &mut sections,
        "不含税金额",
        Some(format_yuan(&submission.net_amount)).as_deref(),
        true,
    );
    if !submission.tax_amount.to_decimal().is_zero() {
        push_section(
            &mut sections,
            "税额",
            Some(format_yuan(&submission.tax_amount)).as_deref(),
            true,
        );
    }
    push_section(
        &mut sections,
        "付款条件",
        non_empty(&submission.payment_term_snapshot.payment_term_name).as_deref(),
        false,
    );
    push_section(&mut sections, "项目", submission.project_name.as_deref(), false);
    sections
}

/// 生成销售单队列一行摘要。
///
/// # 参数
/// * `customer` - 客户名称
/// * `gross` - 含税金额
/// * `tax` - 税额
/// * `payment` - 付款条件
/// * `lines` - 已截断的简报行
/// * `more_count` - 未展开行数
///
/// # 返回
/// 返回客户、金额、付款条件和首行货品组成的摘要。
///
/// # 错误
/// 无。
fn sales_order_list_summary(
    customer: Option<&str>,
    gross: &Amount,
    tax: &Amount,
    payment: Option<&str>,
    lines: &[BriefLine],
    more_count: u32,
) -> String {
    let amount = if tax.to_decimal().is_zero() {
        format_yuan(gross)
    } else {
        format!("{} / 税 {}", format_yuan(gross), format_yuan(tax))
    };
    let first_line = lines.first().map(|line| match line.quantity.as_deref() {
        Some(quantity) => format!("{} {quantity}", line.title),
        None => line.title.clone(),
    });
    let more = (more_count > 0).then(|| format!("另 {more_count} 行"));
    join_list_summary([
        customer
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        Some(amount),
        payment
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        first_line,
        more,
    ])
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
fn sales_order_impact(business_type_label: &str) -> &'static str {
    if business_type_label == "卡券" {
        "不审批则卡券销售不能生效"
    } else {
        "不审批则销售单不能生效、不能履约"
    }
}

/// 把单条销售提交行转成简报行。
///
/// # 参数
/// * `line` - 销售提交行
///
/// # 返回
/// 商品行展示品名、数量和交期；卡券行展示张数和含税金额。
///
/// # 错误
/// 无。
fn sales_brief_line(line: &SalesOrderSubmissionLine) -> BriefLine {
    BriefLine {
        title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
        quantity: sales_line_quantity(line),
        due_label: line.fulfillment_due_at.map(format_instant_due_label),
    }
}

/// 销售行数量带上含税小计。
///
/// # 参数
/// * `line` - 销售提交行
///
/// # 返回
/// 卡券返回 `10 张 · ¥1,000`；实物返回 `20 件 · ¥1,600`；否则只返回金额。
///
/// # 错误
/// 无。
fn sales_line_quantity(line: &SalesOrderSubmissionLine) -> Option<String> {
    let amount = format_yuan(&line.gross_amount);
    if let Some(count) = line.card_count.filter(|count| *count > 0) {
        return Some(format!("{count} 张 · {amount}"));
    }
    match line.quantity.as_ref() {
        Some(qty) => Some(format!(
            "{} · {amount}",
            format_quantity(
                qty,
                line.unit_snapshot.as_deref().or(line.base_unit_code.as_deref()),
            )
        )),
        None => Some(amount),
    }
}

/// 仅测试使用：数量格式化入口，避免测试构造完整提交行。
///
/// # 参数
/// * `quantity` - 基础单位数量
/// * `unit` - 单位
/// * `gross` - 行含税金额
/// * `card_count` - 卡张数
///
/// # 返回
/// 与 `sales_line_quantity` 相同规则的展示串。
///
/// # 错误
/// 无。
#[cfg(test)]
fn sales_line_quantity_parts(
    quantity: Option<&Quantity>,
    unit: Option<&str>,
    gross: &Amount,
    card_count: Option<u32>,
) -> Option<String> {
    let amount = format_yuan(gross);
    if let Some(count) = card_count.filter(|count| *count > 0) {
        return Some(format!("{count} 张 · {amount}"));
    }
    match quantity {
        Some(qty) => Some(format!("{} · {amount}", format_quantity(qty, unit))),
        None => Some(amount),
    }
}

#[cfg(test)]
mod tests {
    use entities::money::Quantity;

    use super::*;

    fn amount(value: &str) -> Amount {
        value.parse().expect("测试金额必须合法")
    }

    fn qty(value: &str) -> Quantity {
        value.parse().expect("测试数量必须合法")
    }

    #[test]
    fn list_summary_shows_customer_scale_payment_and_first_line() {
        let lines = vec![BriefLine {
            title: "办公椅".to_string(),
            quantity: Some("20 件 · ¥10,000".to_string()),
            due_label: Some("8/20 交".to_string()),
        }];
        let summary = sales_order_list_summary(
            Some("华东纸业"),
            &amount("12800"),
            &amount("1470"),
            Some("先款 30%"),
            &lines,
            2,
        );
        assert!(summary.contains("华东纸业"));
        assert!(summary.contains("¥12,800 / 税 ¥1,470"));
        assert!(summary.contains("先款 30%"));
        assert!(summary.contains("办公椅"));
        assert!(summary.contains("另 2 行"));
    }

    #[test]
    fn impact_distinguishes_voucher_and_physical() {
        assert_eq!(sales_order_impact("卡券"), "不审批则卡券销售不能生效");
        assert_eq!(
            sales_order_impact("实物及服务"),
            "不审批则销售单不能生效、不能履约"
        );
    }

    #[test]
    fn line_quantity_joins_qty_cards_and_amount() {
        assert_eq!(
            sales_line_quantity_parts(Some(&qty("20")), Some("件"), &amount("1600"), None).as_deref(),
            Some("20 件 · ¥1,600")
        );
        assert_eq!(
            sales_line_quantity_parts(None, None, &amount("1000"), Some(10)).as_deref(),
            Some("10 张 · ¥1,000")
        );
        assert_eq!(
            sales_line_quantity_parts(None, None, &amount("80"), None).as_deref(),
            Some("¥80")
        );
    }
}
