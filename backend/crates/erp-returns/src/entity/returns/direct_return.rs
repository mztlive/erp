//! 已生效销售减量后的客户直退供应商凭据；不改写原交付，也不单独冲减款项。
use std::collections::HashSet;

use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 本次实际已被供应商接收的明细。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectReturnLine {
    pub purchase_line_id: String,
    pub quantity: Quantity,
}

/// 同键重试必须逐字段相同；原采购版本在首次登记时校验。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectReturnRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
    pub reason: String,
    /// 供应商收货回单、签收单号或核实说明。
    pub receipt_reference: String,
    pub lines: Vec<DirectReturnLine>,
}
impl DirectReturnRequest {
    /// 校验当前采购可直退量，允许分批实际退回。
    /// # 参数
    /// * `limits` - 事务内核实的当前采购行及可退上限。
    /// # 返回
    /// 所有数量均在范围内时成功。
    /// # 错误
    /// 非当前行、非正数、重复或累计超退时拒绝。
    pub fn check_limits(&self, limits: &[(&str, Quantity)]) -> Result<()> {
        self.validate()?;
        for line in &self.lines {
            let max = limits
                .iter()
                .find(|(id, _)| *id == line.purchase_line_id)
                .map(|(_, quantity)| *quantity)
                .ok_or_else(|| Error::from("退货行已变化或不属于当前采购，请刷新后重试"))?;
            if line.quantity > max {
                return Err(Error::from("实际退回数量超过本采购当前可直退数量，请核对已登记记录"));
            }
        }
        Ok(())
    }

    /// 校验回单、原因及去重后的正数数量。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回可持久化请求。
    /// # 错误
    /// 空白、超长、重复行及非正数拒绝。
    pub fn validate(&self) -> Result<()> {
        let key = &self.idempotency_key;
        if key.len() != 36 || !key.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(Error::from("退货提交标识无效，请重新打开登记窗口"));
        }
        for (value, label) in [(&self.reason, "退货原因"), (&self.receipt_reference, "供应商收货凭据")]
        {
            if value.trim().is_empty() || value.len() > 1500 {
                return Err(Error::from(format!("请填写{label}，长度不得超过 1500 字节")));
            }
        }
        if self.lines.is_empty() || self.lines.len() > 100 {
            return Err(Error::from("每次登记 1 至 100 条实际退货明细"));
        }
        let mut seen = HashSet::new();
        for line in &self.lines {
            if line.purchase_line_id.is_empty()
                || !seen.insert(&line.purchase_line_id)
                || line.quantity.to_decimal() <= Decimal::ZERO
            {
                return Err(Error::from("退货明细不得重复，实际退回数量必须大于零"));
            }
        }
        Ok(())
    }
}

/// 经交易验证后冻结的实际退回凭据。旧退货草稿不具有此字段，不能抵扣数量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectReturnEvidence {
    pub request: DirectReturnRequest,
    pub sales_revision_id: String,
    /// 稳定销售行及实际退回量，保持跨采购版本可追溯。
    pub returned_lines: Vec<(String, Quantity)>,
}

/// 从累计履约中扣除已证明退回的数量，越界时拒绝隐式归零。
/// # 参数
/// * `fulfilled` - 原累计履约量。
/// * `returned` - 供应商实际接收的累计退回量。
/// # 返回
/// 非负净履约量。
/// # 错误
/// 负数、退回超量或溢出时拒绝。
pub fn net_fulfilled(fulfilled: Quantity, returned: Quantity) -> Result<Quantity> {
    let net = fulfilled
        .to_decimal()
        .checked_sub(returned.to_decimal())
        .ok_or_else(|| Error::from("退回数量超出范围"))?;
    if returned.to_decimal().is_sign_negative() || net.is_sign_negative() {
        return Err(Error::from("累计退回数量超过原履约数量，请核对退货记录"));
    }
    Quantity::try_from(net)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    fn q(s: &str) -> Quantity {
        Quantity::from_str(s).unwrap()
    }
    #[test]
    fn returned_goods_reduce_floor_without_rewriting_shipment() {
        assert_eq!(net_fulfilled(q("47"), q("27")).unwrap(), q("20"));
        assert_eq!(net_fulfilled(q("47"), q("47")).unwrap(), q("0"));
        assert!(net_fulfilled(q("47"), q("48")).is_err());
        assert!(net_fulfilled(q("47"), q("-1")).is_err());
    }
    #[test]
    fn actual_receipt_and_unique_positive_lines_are_required() {
        let mut req = DirectReturnRequest {
            expected_version: 1,
            idempotency_key: "12345678-1234-1234-1234-123456789abc".into(),
            reason: "销售减量".into(),
            receipt_reference: "供应商签收单 100".into(),
            lines: vec![DirectReturnLine { purchase_line_id: "line".into(), quantity: q("27") }],
        };
        req.validate().unwrap();
        req.lines.push(req.lines[0].clone());
        assert!(req.validate().is_err());
        req.lines.pop();
        req.lines[0].quantity = q("0");
        assert!(req.validate().is_err());
        req.lines[0].quantity = q("27");
        req.receipt_reference.clear();
        assert!(req.validate().is_err());
    }
    #[test]
    fn partial_multi_line_returns_reject_foreign_and_excess_quantities() {
        let mut req = DirectReturnRequest {
            expected_version: 1,
            idempotency_key: "12345678-1234-1234-1234-123456789abc".into(),
            reason: "减量退回".into(),
            receipt_reference: "回单 1".into(),
            lines: vec![
                DirectReturnLine { purchase_line_id: "a".into(), quantity: q("10") },
                DirectReturnLine { purchase_line_id: "b".into(), quantity: q("2.5") },
            ],
        };
        req.check_limits(&[("a", q("27")), ("b", q("2.5"))]).unwrap();
        assert!(req.check_limits(&[("a", q("9")), ("b", q("2.5"))]).is_err());
        assert!(req.check_limits(&[("a", q("27"))]).is_err());
        req.lines.clear();
        assert!(req.check_limits(&[]).is_err());
        let first = net_fulfilled(q("47"), q("10")).unwrap();
        assert_eq!(net_fulfilled(first, q("17")).unwrap(), q("20"));
        assert!(net_fulfilled(first, q("38")).is_err());
    }
}
