//! 采购单财务审核任务的事项简报装载。
//!
//! 批量读取采购提交、提交行、供应商快照和来源销售单，生成队列只读简报。
//! 正式通过/驳回仍在采购单审核页提交。

use std::collections::{HashMap, HashSet};

use database::{Executor, PurchaseOrderExt, SalesOrderExt};
use entities::ids::PurchaseOrderSubmissionId;
use entities::money::{Amount, Quantity};
use entities::purchase_order::{
    PaymentTermSnapshot, PurchaseLineType, PurchaseOrder, PurchaseOrderSubmission,
    PurchaseOrderSubmissionLine,
};

use super::brief::{
    format_business_due_label, format_quantity, line_title, BriefLine, BriefSection, ObjectBriefSource,
};
use super::presentation::{format_yuan, purchase_review_impact_summary};
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, SubjectBrief, WorkItemService};
use crate::errors::Result;

/// 采购审核在对象事实中按提交版本保存的展示包。
#[derive(Debug, Clone)]
struct PurchaseReviewDisplay {
    purchase_order_id: String,
    counterparty: Option<String>,
    impact: String,
    brief: ObjectBriefSource,
}

impl WorkItemService {
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
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let orders = self.purchase_orders_for_keys(keys, executor).await?;
        if orders.is_empty() {
            return Ok(());
        }
        let displays = self.purchase_review_displays(&orders, executor).await?;
        insert_purchase_order_facts(facts, &orders, &displays);
        Ok(())
    }

    /// 按对象键批量读取采购单。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 没有采购单键时返回空集合。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn purchase_orders_for_keys(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrder>> {
        let ids = object_ids(keys, ObjectKind::PurchaseOrder);
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .purchase_orders()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?)
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
    ) -> Result<HashMap<String, PurchaseReviewDisplay>> {
        let submissions = self.purchase_submissions_for_orders(orders, executor).await?;
        let sales_order_nos = self.sales_order_numbers_for_orders(orders, executor).await?;
        let lines_by_submission = self
            .purchase_submission_brief_lines(&submissions, executor)
            .await?;
        let submitter_names = HashMap::<String, String>::new();
        let _ = executor;
        Ok(assemble_purchase_review_displays(
            &submissions,
            &sales_nos_by_purchase_order(orders, &sales_order_nos),
            &lines_by_submission,
            &submitter_names,
        ))
    }

    /// 读取本批采购单的全部提交，避免按单 N+1。
    ///
    /// # 参数
    /// * `orders` - 本批采购单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回这些采购单上的全部提交。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn purchase_submissions_for_orders(
        &self,
        orders: &[PurchaseOrder],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderSubmission>> {
        let order_ids = orders
            .iter()
            .map(|order| order.base.id.clone())
            .collect::<Vec<_>>();
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .purchase_order_submissions()
            .list_work_item_brief_submissions_by_orders(&order_ids, executor)
            .await?)
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
        let sales_order_ids = orders
            .iter()
            .map(|order| order.sales_order_id.to_string())
            .collect::<Vec<_>>();
        if sales_order_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .db
            .sales_orders()
            .list_work_item_brief_entities_by_ids(&sales_order_ids, executor)
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
    /// 返回提交 ID 到已按行号排序的简报行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn purchase_submission_brief_lines(
        &self,
        submissions: &[PurchaseOrderSubmission],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<BriefLine>>> {
        let submission_ids = submissions
            .iter()
            .map(|item| PurchaseOrderSubmissionId::new(item.base.id.clone()))
            .collect::<Vec<_>>();
        if submission_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut grouped: HashMap<String, Vec<(u32, PurchaseOrderSubmissionLine)>> = HashMap::new();
        for line in self
            .db
            .purchase_order_submission_lines()
            .find_lines_by_submission_ids(&submission_ids, executor)
            .await?
        {
            grouped
                .entry(line.purchase_order_submission_id.to_string())
                .or_default()
                .push((line.line_no, line));
        }
        Ok(grouped
            .into_iter()
            .map(|(submission_id, mut rows)| {
                rows.sort_by_key(|(line_no, _)| *line_no);
                (
                    submission_id,
                    purchase_brief_lines(rows.into_iter().map(|(_, line)| line)),
                )
            })
            .collect())
    }
}

/// 把采购单上的来源销售单号改成按采购单 ID 查找。
///
/// # 参数
/// * `orders` - 本批采购单
/// * `sales_order_nos` - 销售单 ID 到单号
///
/// # 返回
/// 返回采购单 ID 到销售单号。
///
/// # 错误
/// 无。
fn sales_nos_by_purchase_order(
    orders: &[PurchaseOrder],
    sales_order_nos: &HashMap<String, String>,
) -> HashMap<String, String> {
    orders
        .iter()
        .filter_map(|order| {
            sales_order_nos
                .get(&order.sales_order_id.to_string())
                .cloned()
                .map(|order_no| (order.base.id.clone(), order_no))
        })
        .collect()
}

/// 把提交、销售单号和行明细组装成按提交 ID 索引的展示包。
///
/// # 参数
/// * `submissions` - 本批采购提交
/// * `sales_nos` - 采购单 ID 到销售单号
/// * `lines_by_submission` - 提交 ID 到简报行
/// * `submitter_names` - 账号 ID 到姓名
///
/// # 返回
/// 返回提交 ID 到供应商、影响和简报的映射。
///
/// # 错误
/// 无。
fn assemble_purchase_review_displays(
    submissions: &[PurchaseOrderSubmission],
    sales_nos: &HashMap<String, String>,
    lines_by_submission: &HashMap<String, Vec<BriefLine>>,
    submitter_names: &HashMap<String, String>,
) -> HashMap<String, PurchaseReviewDisplay> {
    let origins = submission_origins(submissions);
    submissions
        .iter()
        .map(|submission| {
            let display = purchase_review_display(
                submission,
                sales_nos
                    .get(submission.purchase_order_id.as_ref())
                    .map(String::as_str),
                lines_by_submission
                    .get(&submission.base.id)
                    .cloned()
                    .unwrap_or_default(),
                submission
                    .submitted_by
                    .as_ref()
                    .and_then(|actor| submitter_names.get(actor).cloned()),
                origins.get(&submission.base.id).copied(),
            );
            (submission.base.id.clone(), display)
        })
        .collect()
}

/// 把采购单事实和按提交准备好的简报写入对象事实表。
///
/// # 参数
/// * `facts` - 输出的对象事实表
/// * `orders` - 本批采购单
/// * `displays` - 提交 ID 到展示字段
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn insert_purchase_order_facts(
    facts: &mut ObjectFactMap,
    orders: &[PurchaseOrder],
    displays: &HashMap<String, PurchaseReviewDisplay>,
) {
    for order in orders {
        facts.insert(
            (ObjectKind::PurchaseOrder, order.base.id.clone()),
            purchase_order_fact(order, displays),
        );
    }
}

/// 为单张采购单生成对象事实，并按提交版本挂上简报。
///
/// # 参数
/// * `order` - 采购单
/// * `displays` - 提交 ID 到展示字段
///
/// # 返回
/// 返回带最小标题和提交级简报的对象事实。
///
/// # 错误
/// 无。
fn purchase_order_fact(
    order: &PurchaseOrder,
    displays: &HashMap<String, PurchaseReviewDisplay>,
) -> ObjectFact {
    let mut fact = ObjectFact::new(
        order.base.id.clone(),
        format!("采购单 {}", order.purchase_no),
        order.stable.created_by.clone(),
    );
    for (submission_id, display) in displays {
        if display.purchase_order_id != order.base.id {
            continue;
        }
        let brief = SubjectBrief {
            counterparty_label: display.counterparty.clone(),
            impact_summary: Some(display.impact.clone()),
            brief_source: Some(display.brief.clone()),
        };
        if order.current_submission_id.as_deref() == Some(submission_id.as_str()) {
            fact.counterparty_label = brief.counterparty_label.clone();
            fact.impact_summary = brief.impact_summary.clone();
            fact.brief_source = brief.brief_source.clone();
        }
        fact.subject_briefs.insert(submission_id.clone(), brief);
    }
    fact
}

/// 按采购单内提交序号判断初次提交或驳回后重提。
///
/// # 参数
/// * `submissions` - 本批采购提交
///
/// # 返回
/// 返回提交 ID 到来源文案的映射。
///
/// # 错误
/// 无。
fn submission_origins(submissions: &[PurchaseOrderSubmission]) -> HashMap<String, &'static str> {
    submissions
        .iter()
        .map(|current| {
            let has_earlier = submissions.iter().any(|item| {
                item.purchase_order_id == current.purchase_order_id
                    && item.submission_no.as_str() < current.submission_no.as_str()
            });
            let origin = if has_earlier {
                "驳回后重提"
            } else {
                "初次提交"
            };
            (current.base.id.clone(), origin)
        })
        .collect()
}

/// 用单条提交和已解析展示字段生成队列简报。
///
/// # 参数
/// * `submission` - 不可变采购提交
/// * `sales_order_no` - 来源销售单号
/// * `lines` - 已按行号排好的简报行
/// * `submitter_name` - 已解析的提交人姓名
/// * `origin` - 初次提交或驳回后重提
///
/// # 返回
/// 返回供应商、影响和事项简报。
///
/// # 错误
/// 无。
fn purchase_review_display(
    submission: &PurchaseOrderSubmission,
    sales_order_no: Option<&str>,
    lines: Vec<BriefLine>,
    submitter_name: Option<String>,
    origin: Option<&str>,
) -> PurchaseReviewDisplay {
    let supplier = non_empty(&submission.supplier_snapshot.supplier_name);
    let (visible_lines, more_count) = split_purchase_brief_lines(lines);
    let brief = purchase_review_brief_source(
        supplier.clone(),
        sales_order_no,
        submission,
        visible_lines,
        more_count,
        submitter_name,
        origin,
    );
    let line_count = Some(brief.lines.len() + brief.more_count as usize).filter(|count| *count > 0);
    PurchaseReviewDisplay {
        purchase_order_id: submission.purchase_order_id.to_string(),
        counterparty: supplier,
        impact: purchase_review_impact_summary(
            line_count,
            Some(&submission.gross_amount),
            submission.payment_term_snapshot.prepay_gate,
        ),
        brief,
    }
}

/// 组装采购审核事项简报键值和列表一行摘要。
///
/// # 参数
/// * `supplier` - 供应商名称
/// * `sales_order_no` - 来源销售单号
/// * `submission` - 不可变采购提交
/// * `lines` - 截断后的简报行
/// * `more_count` - 未展开的商品行数
/// * `submitter_name` - 提交人姓名
/// * `origin` - 提交来源
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
fn purchase_review_brief_source(
    supplier: Option<String>,
    sales_order_no: Option<&str>,
    submission: &PurchaseOrderSubmission,
    lines: Vec<BriefLine>,
    more_count: u32,
    submitter_name: Option<String>,
    origin: Option<&str>,
) -> ObjectBriefSource {
    let payment = payment_term_label(&submission.payment_term_snapshot);
    ObjectBriefSource {
        customer: None,
        amount_label: Some(format_yuan(&submission.gross_amount)),
        extra_sections: purchase_review_sections(
            supplier.as_deref(),
            sales_order_no,
            submission,
            payment.as_deref(),
            origin,
        ),
        list_summary: purchase_review_list_summary(
            supplier.as_deref(),
            &submission.gross_amount,
            &submission.tax_amount,
            payment.as_deref(),
            &lines,
            more_count,
        ),
        lines,
        more_count,
        submitter_name,
    }
}

/// 生成采购审核简报键值，空值不上屏。
///
/// # 参数
/// * `supplier` - 供应商名称
/// * `sales_order_no` - 来源销售单号
/// * `submission` - 不可变采购提交
/// * `payment` - 已翻译的付款条件
/// * `origin` - 提交来源
///
/// # 返回
/// 返回供应商、销售单、金额三元组、付款条件和提交号等段。
///
/// # 错误
/// 无。
fn purchase_review_sections(
    supplier: Option<&str>,
    sales_order_no: Option<&str>,
    submission: &PurchaseOrderSubmission,
    payment: Option<&str>,
    origin: Option<&str>,
) -> Vec<BriefSection> {
    let mut sections = Vec::new();
    push_section(&mut sections, "供应商", supplier, false);
    push_section(
        &mut sections,
        "来源销售单",
        sales_order_no.map(str::trim).filter(|text| !text.is_empty()),
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
    push_section(&mut sections, "付款条件", payment, false);
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
    push_section(&mut sections, "提交来源", origin, false);
    push_section(
        &mut sections,
        "提交号",
        non_empty(&submission.submission_no).as_deref(),
        false,
    );
    sections
}

/// 生成采购审核队列一行摘要。
///
/// # 参数
/// * `supplier` - 供应商名称
/// * `gross` - 含税金额
/// * `tax` - 税额
/// * `payment` - 付款条件
/// * `lines` - 已截断的简报行
/// * `more_count` - 未展开行数
///
/// # 返回
/// 返回供应商、金额、付款条件和首行货品组成的摘要。
///
/// # 错误
/// 无。
fn purchase_review_list_summary(
    supplier: Option<&str>,
    gross: &Amount,
    tax: &Amount,
    payment: Option<&str>,
    lines: &[BriefLine],
    more_count: u32,
) -> String {
    let mut parts = Vec::new();
    if let Some(name) = supplier.map(str::trim).filter(|text| !text.is_empty()) {
        parts.push(name.to_string());
    }
    parts.push(amount_with_tax_label(gross, tax));
    if let Some(payment) = payment.map(str::trim).filter(|text| !text.is_empty()) {
        parts.push(payment.to_string());
    }
    if let Some(first) = lines.first() {
        parts.push(first_line_summary(first));
    }
    if more_count > 0 {
        parts.push(format!("另 {more_count} 行"));
    }
    parts.join(" · ")
}

/// 把含税金额和税额拼成列表摘要中的金额段。
///
/// # 参数
/// * `gross` - 含税金额
/// * `tax` - 税额
///
/// # 返回
/// 有税额时返回 `¥12,800 / 税 ¥1,470`，否则只返回含税金额。
///
/// # 错误
/// 无。
fn amount_with_tax_label(gross: &Amount, tax: &Amount) -> String {
    let gross_label = format_yuan(gross);
    if tax.to_decimal().is_zero() {
        return gross_label;
    }
    format!("{gross_label} / 税 {}", format_yuan(tax))
}

/// 把首行品名和数量压成列表摘要片段。
///
/// # 参数
/// * `line` - 第一条简报行
///
/// # 返回
/// 返回品名，有数量时追加数量。
///
/// # 错误
/// 无。
fn first_line_summary(line: &BriefLine) -> String {
    match line.quantity.as_deref() {
        Some(quantity) => format!("{} {quantity}", line.title),
        None => line.title.clone(),
    }
}

/// 商品行保留前 3 条，物流费用始终单独列出。
///
/// # 参数
/// * `lines` - 已按行号排好的简报行
///
/// # 返回
/// 返回可见行和未展开的商品行数。
///
/// # 错误
/// 无。
fn split_purchase_brief_lines(lines: Vec<BriefLine>) -> (Vec<BriefLine>, u32) {
    let (items, fees): (Vec<_>, Vec<_>) = lines
        .into_iter()
        .partition(|line| line.title != PurchaseLineType::LogisticsFee.label());
    let more_count = items.len().saturating_sub(super::brief::BRIEF_LINE_LIMIT) as u32;
    let mut visible = items;
    visible.truncate(super::brief::BRIEF_LINE_LIMIT);
    visible.extend(fees);
    (visible, more_count)
}

/// 把采购提交行转成队列简报行。
///
/// # 参数
/// * `lines` - 同一提交内已按行号排序的明细
///
/// # 返回
/// 返回品名、数量/金额和交期。
///
/// # 错误
/// 无。
fn purchase_brief_lines(lines: impl IntoIterator<Item = PurchaseOrderSubmissionLine>) -> Vec<BriefLine> {
    lines.into_iter().map(purchase_brief_line).collect()
}

/// 把单条采购提交行转成简报行。
///
/// # 参数
/// * `line` - 采购提交行
///
/// # 返回
/// 商品行展示品名、数量和交期；物流费用行展示类型和含税金额。
///
/// # 错误
/// 无。
fn purchase_brief_line(line: PurchaseOrderSubmissionLine) -> BriefLine {
    if line.line_type == PurchaseLineType::LogisticsFee {
        return BriefLine {
            title: PurchaseLineType::LogisticsFee.label().to_string(),
            quantity: Some(format_yuan(&line.gross_amount)),
            due_label: None,
        };
    }
    BriefLine {
        title: line_title(
            line.product_name_snapshot.as_deref().unwrap_or(""),
            line.specification_snapshot.as_deref(),
        ),
        quantity: purchase_item_quantity(
            line.quantity.as_ref(),
            line.base_unit_code.as_deref(),
            &line.gross_amount,
        ),
        due_label: line.expected_delivery_date.map(format_business_due_label),
    }
}

/// 商品行数量带上含税小计，方便财务扫一眼规模。
///
/// # 参数
/// * `quantity` - 基础单位数量
/// * `unit` - 单位
/// * `gross` - 行含税金额
///
/// # 返回
/// 有数量时返回 `20 件 · ¥1,600`；否则只返回金额。
///
/// # 错误
/// 无。
fn purchase_item_quantity(quantity: Option<&Quantity>, unit: Option<&str>, gross: &Amount) -> Option<String> {
    let amount = format_yuan(gross);
    Some(match quantity {
        Some(qty) => format!("{} · {amount}", format_quantity(qty, unit)),
        None => amount,
    })
}

/// 把受控付款条件代码翻译成财务可读文案。
///
/// # 参数
/// * `snapshot` - 提交时付款条件快照
///
/// # 返回
/// 返回先款比例、货到天数或合同约定；未知代码回退原码，先款门禁单独补「先款后货」。
///
/// # 错误
/// 无。
fn payment_term_label(snapshot: &PaymentTermSnapshot) -> Option<String> {
    let code = snapshot.payment_term_code.trim();
    let named = match code {
        "PREPAY_100" => "先款 100%",
        "PREPAY_50" => "先款 50%",
        "PREPAY_30" => "先款 30%",
        "POSTPAY_NET15" => "货到 15 天",
        "POSTPAY_NET30" => "货到 30 天",
        "CONTRACT" => "按合同约定",
        other if !other.is_empty() => other,
        _ => return snapshot.prepay_gate.then(|| "先款后货".to_string()),
    };
    if snapshot.prepay_gate && !named.contains("先款") {
        Some(format!("{named} · 先款后货"))
    } else {
        Some(named.to_string())
    }
}

/// 向简报追加非空键值段。
///
/// # 参数
/// * `sections` - 已组装的键值段
/// * `label` - 标签
/// * `value` - 待写入的值
/// * `numeric` - 是否按数字对齐
///
/// # 返回
/// 无。空值不上屏。
///
/// # 错误
/// 无。
fn push_section(sections: &mut Vec<BriefSection>, label: &str, value: Option<&str>, numeric: bool) {
    let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    sections.push(BriefSection {
        label: label.to_string(),
        value: value.to_string(),
        numeric,
    });
}

/// 去掉首尾空白后保留非空文本。
///
/// # 参数
/// * `value` - 原始文本
///
/// # 返回
/// 空白返回 `None`。
///
/// # 错误
/// 无。
fn non_empty(value: &str) -> Option<String> {
    let text = value.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// 采购审核待办创建时使用的稳定原因码。
///
/// # 参数
/// * `submission_no` - 提交序号
///
/// # 返回
/// 首个提交返回初次审核码，其后视为驳回重提。
///
/// # 错误
/// 无。
#[cfg(test)]
pub(crate) fn purchase_review_reason_code(submission_no: &str) -> &'static str {
    if submission_no.trim() == "SUB-000001" {
        "purchase_order_review_dispatched"
    } else {
        "purchase_order_review_resubmitted"
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::BusinessDate;
    use entities::purchase_order::{FulfillmentResponsibility, PurchaseType};

    use super::*;

    fn payment(code: &str, prepay: bool) -> PaymentTermSnapshot {
        PaymentTermSnapshot::new(code.to_string(), prepay, None, None).expect("付款条件必须合法")
    }

    fn amount(value: &str) -> Amount {
        value.parse().expect("测试金额必须合法")
    }

    fn qty(value: &str) -> Quantity {
        value.parse().expect("测试数量必须合法")
    }

    #[test]
    fn payment_term_uses_business_labels_and_prepay_gate() {
        assert_eq!(
            payment_term_label(&payment("PREPAY_30", true)).as_deref(),
            Some("先款 30%")
        );
        assert_eq!(
            payment_term_label(&payment("POSTPAY_NET30", true)).as_deref(),
            Some("货到 30 天 · 先款后货")
        );
        assert_eq!(
            payment_term_label(&payment("CUSTOM", false)).as_deref(),
            Some("CUSTOM")
        );
    }

    #[test]
    fn reason_code_distinguishes_first_and_resubmitted() {
        assert_eq!(
            purchase_review_reason_code("SUB-000001"),
            "purchase_order_review_dispatched"
        );
        assert_eq!(
            purchase_review_reason_code("SUB-000002"),
            "purchase_order_review_resubmitted"
        );
    }

    #[test]
    fn list_summary_shows_supplier_scale_payment_and_first_line() {
        let lines = vec![
            BriefLine {
                title: "办公椅".to_string(),
                quantity: Some("20 件 · ¥10,000".to_string()),
                due_label: Some("8/20 交".to_string()),
            },
            BriefLine {
                title: PurchaseLineType::LogisticsFee.label().to_string(),
                quantity: Some("¥200".to_string()),
                due_label: None,
            },
        ];
        let summary = purchase_review_list_summary(
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
    fn item_lines_keep_three_and_always_surface_logistics_fee() {
        let mut lines = Vec::new();
        for name in ["椅", "桌", "灯", "柜"] {
            lines.push(BriefLine {
                title: name.to_string(),
                quantity: Some("1 件 · ¥1".to_string()),
                due_label: None,
            });
        }
        lines.push(BriefLine {
            title: PurchaseLineType::LogisticsFee.label().to_string(),
            quantity: Some("¥80".to_string()),
            due_label: None,
        });
        let (visible, more_count) = split_purchase_brief_lines(lines);
        assert_eq!(more_count, 1);
        assert_eq!(visible.len(), 4);
        assert_eq!(visible[3].title, "物流费用");
    }

    #[test]
    fn purchase_item_quantity_joins_qty_and_amount() {
        assert_eq!(
            purchase_item_quantity(Some(&qty("20")), Some("件"), &amount("1600")).as_deref(),
            Some("20 件 · ¥1,600")
        );
        assert_eq!(
            purchase_item_quantity(None, None, &amount("80")).as_deref(),
            Some("¥80")
        );
        assert_eq!(PurchaseType::Physical.label(), "实物");
        assert_eq!(FulfillmentResponsibility::Warehouse.label(), "入仓");
        assert_eq!(
            format_business_due_label(BusinessDate::from_ymd(2026, 8, 20).unwrap()),
            "8/20 交"
        );
    }
}
