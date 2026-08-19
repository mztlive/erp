//! 统一待办面向用户的展示文案。
//!
//! 本模块不访问数据库。原因码、影响摘要和对象标题必须翻译成业务语言，
//! 禁止把内部事件名、下划线代码或「打开业务对象」这类机制词直接上屏。

use std::collections::HashMap;

use entities::money::Amount;
use entities::work_item::WorkItemType;

/// 处理人姓名尚未解析时的占位文案。
pub(crate) const UNRESOLVED_OWNER_DISPLAY_NAME: &str = "处理人待确认";

/// 按原因码和任务类型生成「为什么需要处理」。
///
/// # 参数
/// * `reason_code` - 任务写入时的稳定原因码；可为空
/// * `work_item_type` - 任务类型，用于未知原因码时的类型默认文案
///
/// # 返回
/// 返回中文处理理由。未知或内部形态的原因码不会原样上屏。
///
/// # 错误
/// 无。
pub(crate) fn reason_label(reason_code: Option<&str>, work_item_type: WorkItemType) -> String {
    let Some(normalized) = reason_code
        .map(normalize_reason_code)
        .filter(|code| !code.is_empty())
    else {
        return default_reason_label(work_item_type).to_string();
    };
    if let Some(label) = mapped_reason_label(&normalized) {
        return label.to_string();
    }
    if is_user_facing_copy(&normalized) {
        return normalized;
    }
    default_reason_label(work_item_type).to_string()
}

/// 选择可上屏的影响摘要；内部模板或机制词回退到类型默认后果。
///
/// # 参数
/// * `stored` - 任务上保存的影响摘要
/// * `work_item_type` - 任务类型
///
/// # 返回
/// 返回用户可理解的业务影响。
///
/// # 错误
/// 无。
pub(crate) fn usable_impact_summary(stored: Option<&str>, work_item_type: WorkItemType) -> String {
    if let Some(text) = stored.map(str::trim).filter(|text| !text.is_empty()) {
        if is_usable_impact(text, work_item_type) {
            return text.to_string();
        }
    }
    default_impact_summary(work_item_type).to_string()
}

/// 返回进入对应页面后用户要做的下一步。
///
/// # 参数
/// * `work_item_type` - 任务类型
///
/// # 返回
/// 返回一句可执行的下一步说明。
///
/// # 错误
/// 无。
pub(crate) fn next_action_hint(work_item_type: WorkItemType) -> String {
    match work_item_type {
        WorkItemType::ImportBusinessConfirmation => {
            "进入采购确认页后，逐行确认可供数量；确认通过后销售单才会生效。"
        }
        WorkItemType::ImportBusinessConfirmation => {
            "进入销售单后，确认是否按原条件承接；通过后仍需采购再次确认供货。"
        }
        WorkItemType::PurchaseOrderReview => {
            "进入后核对供应商、含税成本、进项税和付款条件，再提交通过或驳回。"
        }
        WorkItemType::SalesChangeImpactReview => "进入销售单后，核对本次变更对履约的影响并提交结论。",
        WorkItemType::SalesChangeFinanceReview => "进入销售单后，核对本次变更对金额的影响并提交结论。",
        WorkItemType::CardFundsReview => "进入票款复核页后，核对准期初回款与开票事实。",
        WorkItemType::CardFundsDeltaReview => "进入票款复核页后，核对差额并提交复核结论。",
        WorkItemType::OwnershipMigrationSalesConfirmation => "进入客户页后，确认本次归属迁移。",
        WorkItemType::OwnershipMigrationFinanceConfirmation => "进入对应页面后，确认归属迁移的财务影响。",
        WorkItemType::InventoryAdjustmentReview => "进入库存页后，核对本次调整并提交复核结论。",
        WorkItemType::FinanceCorrectionReview => "进入对应页面后，核对财务纠错并提交复核结论。",
        WorkItemType::SupplierSettlementReview => "进入结算页后，核对供应商结算并提交复核结论。",
        WorkItemType::ImportBusinessConfirmation => "进入导入页后，确认本次试算范围。",
        WorkItemType::IntegrationResultUnknown => "进入接口错误中心后，确认本次集成结果。",
        WorkItemType::BusinessException => "进入对应页面后，处理本次业务异常。",
        WorkItemType::DocumentApproval => "进入单据后，完成当前审批节点。",
    }
    .to_string()
}

/// 用销售单号生成采购确认任务的对象标题。
///
/// # 参数
/// * `order_no` - 销售单号；空则只返回「销售单」
///
/// # 返回
/// 返回 `销售单 {单号}`，不重复任务类型名。
///
/// # 错误
/// 无。
pub(crate) fn sales_order_object_label(order_no: &str) -> String {
    let order_no = order_no.trim();
    if order_no.is_empty() {
        return "销售单".to_string();
    }
    format!("销售单 {order_no}")
}

/// 用行数和含税金额生成采购确认的业务影响。
///
/// # 参数
/// * `line_count` - 销售提交行数；未知时为 `None`
/// * `gross_amount` - 提交含税金额；未知时为 `None`
///
/// # 返回
/// 返回「不确认则销售单不能生效」，有规模时追加行数和金额。
///
/// # 错误
/// 无。
pub(crate) fn procurement_impact_summary(line_count: Option<usize>, gross_amount: Option<&Amount>) -> String {
    let mut summary = default_impact_summary(WorkItemType::ImportBusinessConfirmation).to_string();
    let mut scale = Vec::new();
    if let Some(count) = line_count.filter(|count| *count > 0) {
        scale.push(format!("{count} 行"));
    }
    if let Some(amount) = gross_amount {
        scale.push(format_yuan(amount));
    }
    if !scale.is_empty() {
        summary.push_str(" · ");
        summary.push_str(&scale.join(" / "));
    }
    summary
}

/// 用行数、含税金额和先款门禁生成采购财务审核的业务影响。
///
/// # 参数
/// * `line_count` - 采购提交行数；未知时为 `None`
/// * `gross_amount` - 提交含税金额；未知时为 `None`
/// * `prepay_gate` - 是否先款后货
///
/// # 返回
/// 返回「不审核则不能形成应付、不能付款」，有规模或先款时追加说明。
///
/// # 错误
/// 无。
pub(crate) fn purchase_review_impact_summary(
    line_count: Option<usize>,
    gross_amount: Option<&Amount>,
    prepay_gate: bool,
) -> String {
    let mut summary = default_impact_summary(WorkItemType::PurchaseOrderReview).to_string();
    let mut scale = Vec::new();
    if let Some(count) = line_count.filter(|count| *count > 0) {
        scale.push(format!("{count} 行"));
    }
    if let Some(amount) = gross_amount {
        scale.push(format_yuan(amount));
    }
    if !scale.is_empty() {
        summary.push_str(" · ");
        summary.push_str(&scale.join(" / "));
    }
    if prepay_gate {
        summary.push_str(" · 先款后货");
    }
    summary
}

/// 按账号姓名表解析处理人展示名。
///
/// # 参数
/// * `owner_id` - 处理人账号 ID
/// * `names` - 账号 ID 到姓名的映射
///
/// # 返回
/// 返回去空白后的姓名；缺失或空名时返回「处理人待确认」。
///
/// # 错误
/// 无。
pub(crate) fn resolve_owner_display_name(owner_id: &str, names: &HashMap<String, String>) -> String {
    names
        .get(owner_id)
        .map(String::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "当前处理人")
        .unwrap_or(UNRESOLVED_OWNER_DISPLAY_NAME)
        .to_string()
}

fn normalize_reason_code(value: &str) -> String {
    value.trim().replace('-', "_").to_ascii_lowercase()
}

fn mapped_reason_label(code: &str) -> Option<&'static str> {
    Some(match code {
        "procurement_confirmation_dispatched" => "销售已提交，需要采购确认能否供货",
        "low_margin_approved_procurement_confirmation" => "低毛利已获上级通过，需要采购重新确认供货",
        "procurement_confirmation_resubmitted" => "销售已按驳回意见重提，需要采购重新确认",
        "procurement_rejection_low_margin_requested" => "采购驳回后，需要上级确认是否按原条件承接",
        "change_impact_dispatched" => "销售变更已提交，需要核对履约影响",
        "change_finance_dispatched" => "销售变更已提交，需要核对财务影响",
        "card_funds_delta_review" => "票款出现差额，需要财务复核",
        "card_funds_opening_review" => "卡券销售已生效，需要核对准期初回款与开票",
        "supplier_settlement_review_dispatched" => "供应商结算单待复核",
        "import_trial_confirmation" => "导入试算已完成，需要业务确认范围",
        "supplier_stopped" => "供应已停止，商城在售发布已暂停",
        "purchase_order_review_dispatched" => "采购已提交，需要核对成本、进项税和付款条件",
        "purchase_order_review_resubmitted" => "采购已按驳回意见重提，需要重新核对成本与付款条件",
        other if other.ends_with("_active") => "当前审批步骤等待处理",
        _ => return None,
    })
}

fn default_reason_label(work_item_type: WorkItemType) -> &'static str {
    match work_item_type {
        WorkItemType::PurchaseOrderReview => "采购已提交，需要核对成本、进项税和付款条件",
        WorkItemType::SalesChangeImpactReview => "销售变更待核对履约影响",
        WorkItemType::SalesChangeFinanceReview => "销售变更待核对财务影响",
        WorkItemType::CardFundsReview => "卡券票款待复核",
        WorkItemType::CardFundsDeltaReview => "票款差额待复核",
        WorkItemType::OwnershipMigrationSalesConfirmation => "客户归属迁移待销售确认",
        WorkItemType::OwnershipMigrationFinanceConfirmation => "客户归属迁移待财务确认",
        WorkItemType::InventoryAdjustmentReview => "库存调整待复核",
        WorkItemType::FinanceCorrectionReview => "财务纠错待复核",
        WorkItemType::SupplierSettlementReview => "供应商结算待复核",
        WorkItemType::ImportBusinessConfirmation => "导入试算待业务确认",
        WorkItemType::IntegrationResultUnknown => "集成结果待确认",
        WorkItemType::BusinessException => "业务异常待处理",
        WorkItemType::DocumentApproval => "单据待审批",
    }
}

fn default_impact_summary(work_item_type: WorkItemType) -> &'static str {
    match work_item_type {
        WorkItemType::PurchaseOrderReview => "不审核则不能形成应付、不能付款",
        WorkItemType::SalesChangeImpactReview => "不复核则销售变更不能继续履约",
        WorkItemType::SalesChangeFinanceReview => "不复核则销售变更金额不能入账",
        WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview => {
            "不复核则票款与开票事实不能确认"
        }
        WorkItemType::DocumentApproval | WorkItemType::DocumentApproval => "不审批则卡券销售不能生效",
        WorkItemType::OwnershipMigrationSalesConfirmation
        | WorkItemType::OwnershipMigrationFinanceConfirmation => "不确认则客户归属不能完成迁移",
        WorkItemType::InventoryAdjustmentReview => "不复核则库存调整不能入账",
        WorkItemType::FinanceCorrectionReview => "不复核则财务纠错不能入账",
        WorkItemType::SupplierSettlementReview => "不复核则供应商结算不能确认",
        WorkItemType::ImportBusinessConfirmation => "不确认则导入范围不能落地",
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException => {
            "不处理则异常会继续挡住后续业务"
        }
        WorkItemType::DocumentApproval => "不审批则单据不能形成正式事实",
    }
}

fn is_usable_impact(text: &str, work_item_type: WorkItemType) -> bool {
    is_user_facing_copy(text)
        && !text.contains("打开业务对象")
        && !text.starts_with(&format!("{}：", work_item_type.label()))
        && !is_identity_echo_impact(text, work_item_type)
}

/// 识别只复读任务类型或单号、没有说明后果的影响摘要。
///
/// # 参数
/// * `text` - 任务上保存的影响摘要
/// * `work_item_type` - 任务类型
///
/// # 返回
/// 复读身份时应丢弃并回退类型默认后果。
///
/// # 错误
/// 无。
fn is_identity_echo_impact(text: &str, work_item_type: WorkItemType) -> bool {
    match work_item_type {
        WorkItemType::PurchaseOrderReview => text.contains("待财务审核") && !text.contains("不审核"),
        _ => false,
    }
}

fn is_user_facing_copy(text: &str) -> bool {
    !text.is_ascii()
}

/// 把含税金额格式化为带千分位的人民币文案。
///
/// # 参数
/// * `amount` - 定点金额
///
/// # 返回
/// 返回如 `¥12,800` 的展示串。
///
/// # 错误
/// 无。
pub(crate) fn format_yuan(amount: &Amount) -> String {
    let raw = amount.to_decimal().normalize().to_string();
    let (int_part, frac) = raw.split_once('.').unwrap_or((raw.as_str(), ""));
    let grouped = group_int(int_part);
    if frac.is_empty() || frac.chars().all(|ch| ch == '0') {
        format!("¥{grouped}")
    } else {
        format!("¥{grouped}.{frac}")
    }
}

fn group_int(int_part: &str) -> String {
    let (sign, digits) = int_part
        .strip_prefix('-')
        .map(|digits| ("-", digits))
        .unwrap_or(("", int_part));
    let mut grouped = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}{}", grouped.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_label_maps_known_codes_and_hides_internal_events() {
        assert_eq!(
            reason_label(
                Some("procurement_confirmation_dispatched"),
                WorkItemType::ImportBusinessConfirmation
            ),
            "销售已提交，需要采购确认能否供货"
        );
        assert_eq!(
            reason_label(
                Some("LOW_MARGIN_APPROVED_PROCUREMENT_CONFIRMATION"),
                WorkItemType::ImportBusinessConfirmation
            ),
            "低毛利已获上级通过，需要采购重新确认供货"
        );
        assert_eq!(
            reason_label(Some("unknown_internal_event"), WorkItemType::PurchaseOrderReview),
            "采购已提交，需要核对成本、进项税和付款条件"
        );
        assert_eq!(
            reason_label(
                Some("purchase_order_review_resubmitted"),
                WorkItemType::PurchaseOrderReview
            ),
            "采购已按驳回意见重提，需要重新核对成本与付款条件"
        );
        assert_eq!(
            reason_label(Some("客户资料缺失"), WorkItemType::ImportBusinessConfirmation),
            "客户资料缺失"
        );
    }

    #[test]
    fn impact_summary_rejects_templates_and_mechanism_words() {
        assert_eq!(
            usable_impact_summary(
                Some("采购二次确认：销售提交 1"),
                WorkItemType::ImportBusinessConfirmation
            ),
            "不确认则销售单不能生效"
        );
        assert_eq!(
            usable_impact_summary(Some("请打开业务对象核对影响。"), WorkItemType::CardFundsReview),
            "不复核则票款与开票事实不能确认"
        );
        assert_eq!(
            usable_impact_summary(Some("同步差额待复核"), WorkItemType::CardFundsDeltaReview),
            "同步差额待复核"
        );
        assert_eq!(
            usable_impact_summary(
                Some("采购单 PO-20260817-9a550b 待财务审核"),
                WorkItemType::PurchaseOrderReview
            ),
            "不审核则不能形成应付、不能付款"
        );
    }

    #[test]
    fn procurement_display_uses_order_identity_and_scale() {
        assert_eq!(sales_order_object_label(" SO-12 "), "销售单 SO-12");
        assert_eq!(sales_order_object_label("  "), "销售单");
        let amount = "12800".parse::<Amount>().expect("测试金额必须合法");
        assert_eq!(
            procurement_impact_summary(Some(3), Some(&amount)),
            "不确认则销售单不能生效 · 3 行 / ¥12,800"
        );
        assert_eq!(procurement_impact_summary(None, None), "不确认则销售单不能生效");
        assert_eq!(
            purchase_review_impact_summary(Some(3), Some(&amount), true),
            "不审核则不能形成应付、不能付款 · 3 行 / ¥12,800 · 先款后货"
        );
    }

    #[test]
    fn owner_display_name_never_uses_placeholder_token() {
        let mut names = HashMap::new();
        names.insert("u1".to_string(), " 周航 ".to_string());
        names.insert("u2".to_string(), "当前处理人".to_string());
        assert_eq!(resolve_owner_display_name("u1", &names), "周航");
        assert_eq!(
            resolve_owner_display_name("u2", &names),
            UNRESOLVED_OWNER_DISPLAY_NAME
        );
        assert_eq!(
            resolve_owner_display_name("missing", &names),
            UNRESOLVED_OWNER_DISPLAY_NAME
        );
    }

    #[test]
    fn next_action_hint_tells_user_what_to_do() {
        assert!(next_action_hint(WorkItemType::ImportBusinessConfirmation).contains("逐行确认可供数量"));
        assert!(next_action_hint(WorkItemType::PurchaseOrderReview).contains("核对供应商、含税成本、进项税"));
        assert!(!next_action_hint(WorkItemType::PurchaseOrderReview).contains("打开业务对象"));
    }
}
