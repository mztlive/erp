//! 原隔离 fixture 使用的供应商付款条件事实适配；不连接外部服务。
/// 复用提供方解析以保持 fixture 的旧规范化值和门禁。
///
/// # 参数
/// * `raw` - 供应商付款条件原文。
///
/// # 返回
/// 成功时返回规范化后的 `PaymentTermFact`。
///
/// # 错误
/// `SupplierPaymentTerm::parse` 失败时返回对应错误。
pub(super) fn payment_term_fact(
    raw: &str,
) -> erp_core::Result<erp_procurement::entity::facts::PaymentTermFact> {
    let term = erp_supplier::SupplierPaymentTerm::parse(raw)?;
    Ok(erp_procurement::entity::facts::PaymentTermFact {
        canonical_code: term.code().to_string(),
        prepay_gate: term.prepay_gate(),
        prepay_minimum_ratio: term.prepay_minimum_ratio(),
        days_after_delivery: term.days_after_delivery(),
        calendar_due: term.calendar_due(),
    })
}
