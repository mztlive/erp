//! 地址提示只刷新实时查询入口；不缓存字典、不改已发送订单、不凭来源编码推断层级。

use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use erp_integration::entity::supplier_callback::SupplierRefreshHint;
use erp_supply::ports::connector::address::{
    AddressCandidateQuery, SupplierAddressDepth, SupplierAddressSource,
};

use crate::{Error, Result};

/// 接收连接级地址更新提示后，核实实时地址查询可用才允许完成消息处理。
/// # 参数
/// `source` 为原连接实时地址能力，`hint` 为已保存的来源提示，`received_at` 为收件时间。
/// # 返回
/// 已重新读取有效省级入口；原目标编码仅保留为变更线索，不作为省级查询参数。
/// # 错误
/// 提示归属类型不符、来源不可读、缺项或观察早于收件时返回错误，保持消息待处理。
pub(super) async fn refresh_address_notice(
    source: &dyn SupplierAddressSource,
    hint: &SupplierRefreshHint,
    received_at: Instant,
) -> Result<()> {
    if hint.topic != "address"
        || hint.target_kind != "supplier_address"
        || hint.object_id.trim().is_empty()
        || hint.product_id.is_some()
        || hint.funds_transaction_id.is_some()
    {
        return Err(Error::ValidationError("供应商地址变更提示身份无效".into()));
    }
    let observed = source
        .candidates(&AddressCandidateQuery { depth: SupplierAddressDepth::Province, parent_code: None })
        .await
        .map_err(|_| Error::BusinessLogicError("供应商地址已更新，请稍后重试读取最新地址".into()))?;
    let mut seen = BTreeSet::new();
    if observed.stamp.observed_at < received_at
        || observed.value.is_empty()
        || observed.value.iter().any(|candidate| {
            candidate.depth != SupplierAddressDepth::Province
                || candidate.parent_code.is_some()
                || candidate.code.trim().is_empty()
                || candidate.name.trim().is_empty()
                || !seen.insert(candidate.code.as_str())
        })
    {
        return Err(Error::BusinessLogicError("供应商最新地址尚未完整核实，保留原更新提示".into()));
    }
    // 无本地地址库或跨请求候选缓存。设置地址和新创建步骤仍逐级重读5680并调用5710；
    // 原票据不会绕过即时核验。本提示与处理时间由消息完成回执持久化后才远端ACK。
    Ok(())
}

#[cfg(test)]
mod tests;
