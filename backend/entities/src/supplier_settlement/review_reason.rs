//! 结算复核驳回原因值对象（FUL-E13）。
//!
//! 驳回原因的 trim、大写规范化、字符集、长度与固定 allowlist 是领域不变量，
//! 由本值对象独占；Service 只负责传输与事务编排，不得再持有第二份规则。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

/// 结算复核驳回原因固定代码（FUL-E13）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewRejectReason {
    /// 需要补充证据。
    NeedsMoreEvidence,
    /// 金额不一致。
    AmountMismatch,
    /// 其他原因。
    Other,
}

impl SettlementReviewRejectReason {
    /// 从线上传输字符串解析驳回原因。
    ///
    /// 依次执行 trim、前后空白去除后的大写规范化、非空/超长拒绝、受控字符集
    /// 校验与三元 allowlist 匹配；任意一步失败都返回领域错误。
    ///
    /// # 参数
    /// * `value` - 线上传输的原始原因代码（含大小写与空白变体）
    ///
    /// # 返回
    /// 返回规范化后的强类型驳回原因。
    ///
    /// # 错误
    /// 空白、超长、非法字符或未知代码时返回错误。
    ///
    /// # 约束
    /// 稳定代码与持久化/JSON 保持 `NEEDS_MORE_EVIDENCE`、`AMOUNT_MISMATCH`、
    /// `OTHER` 三元集合；不得接受其他字符串。
    pub fn parse(value: &str) -> Result<Self> {
        let normalized = value.trim().to_ascii_uppercase();
        if normalized.is_empty() {
            return Err(Error::from("驳回原因代码不能为空"));
        }
        if normalized.len() > 64 {
            return Err(Error::from("驳回原因代码长度不能超过64"));
        }
        if !normalized.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        }) {
            return Err(Error::from(
                "驳回原因代码只能包含大写字母、数字、下划线、连字符或点",
            ));
        }
        match normalized.as_str() {
            "NEEDS_MORE_EVIDENCE" => Ok(Self::NeedsMoreEvidence),
            "AMOUNT_MISMATCH" => Ok(Self::AmountMismatch),
            "OTHER" => Ok(Self::Other),
            _ => Err(Error::from("结算驳回原因代码不受支持")),
        }
    }

    /// 返回稳定持久化/传输代码。
    ///
    /// # 参数
    /// 无显式参数（方法接收者为已解析的原因值）。
    ///
    /// # 返回
    /// 返回与既有数据库及 JSON 兼容的稳定代码字符串。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 输出恒为三元集合成员；往返解析保持稳定。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NeedsMoreEvidence => "NEEDS_MORE_EVIDENCE",
            Self::AmountMismatch => "AMOUNT_MISMATCH",
            Self::Other => "OTHER",
        }
    }
}

impl std::fmt::Display for SettlementReviewRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::SettlementReviewRejectReason;

    #[test]
    fn parse_normalizes_trim_and_case() {
        assert_eq!(
            SettlementReviewRejectReason::parse("  needs_more_evidence ").unwrap(),
            SettlementReviewRejectReason::NeedsMoreEvidence
        );
        assert_eq!(
            SettlementReviewRejectReason::parse("amount_mismatch").unwrap(),
            SettlementReviewRejectReason::AmountMismatch
        );
        assert_eq!(
            SettlementReviewRejectReason::parse("other").unwrap(),
            SettlementReviewRejectReason::Other
        );
    }

    #[test]
    fn stable_codes_round_trip() {
        for code in ["NEEDS_MORE_EVIDENCE", "AMOUNT_MISMATCH", "OTHER"] {
            let reason = SettlementReviewRejectReason::parse(code).unwrap();
            assert_eq!(reason.as_str(), code);
            assert_eq!(reason.to_string(), code);
            assert_eq!(
                SettlementReviewRejectReason::parse(reason.as_str()).unwrap(),
                reason
            );
        }
    }

    #[test]
    fn rejects_blank_overlong_illegal_and_unknown_codes() {
        assert!(SettlementReviewRejectReason::parse("   ").is_err());
        assert!(SettlementReviewRejectReason::parse("").is_err());
        assert!(SettlementReviewRejectReason::parse(&"A".repeat(65)).is_err());
        assert!(SettlementReviewRejectReason::parse("NEEDS MORE EVIDENCE").is_err());
        assert!(SettlementReviewRejectReason::parse("驳回").is_err());
        assert!(SettlementReviewRejectReason::parse("AMOUNT_UNRESOLVED").is_err());
    }

    #[test]
    fn wire_codes_serialize_to_stable_strings() {
        let json = serde_json::to_string(&SettlementReviewRejectReason::AmountMismatch).unwrap();
        assert_eq!(json, "\"AMOUNT_MISMATCH\"");
        let back: SettlementReviewRejectReason = serde_json::from_str("\"OTHER\"").unwrap();
        assert_eq!(back, SettlementReviewRejectReason::Other);
    }
}
