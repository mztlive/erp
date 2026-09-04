//! 外部调用“结果未知”判定（MASTER-E09 领域归属）。
//!
//! 显式 `ResultUnknown` 分类，或稳定错误码包含大写 `TIMEOUT`、
//! `OUTCOME_UNKNOWN` 时视为结果未知；匹配规则区分大小写。
//! Service 侧 `ClassifiedError` 只做薄委托，保留连接器调用、失败结算、
//! 重试调度和升级编排。

use super::integration_error_task::ErrorClass;

/// 判断外部调用是否没有可确认的最终结果。
pub fn is_result_unknown(class: ErrorClass, code: &str) -> bool {
    class == ErrorClass::ResultUnknown || code.contains("TIMEOUT") || code.contains("OUTCOME_UNKNOWN")
}

/// 把符合结果未知信号的分类归一化为正式结果未知分类。
pub fn normalized_result_unknown_class(class: ErrorClass, code: &str) -> ErrorClass {
    if is_result_unknown(class, code) {
        ErrorClass::ResultUnknown
    } else {
        class
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_result_unknown_is_unknown() {
        assert!(is_result_unknown(ErrorClass::ResultUnknown, "MALL_PENDING"));
    }

    #[test]
    fn uppercase_timeout_and_outcome_unknown_are_unknown() {
        assert!(is_result_unknown(ErrorClass::TransientFailure, "MALL_TIMEOUT"));
        assert!(is_result_unknown(
            ErrorClass::TransientFailure,
            "MALL_OUTCOME_UNKNOWN"
        ));
        assert!(is_result_unknown(
            ErrorClass::BusinessRejected,
            "REMOTE_PRE_TIMEOUT_POST"
        ));
    }

    #[test]
    fn ordinary_and_lowercase_codes_stay_definitive() {
        for (class, code) in [
            (ErrorClass::TransientFailure, "NETWORK_FAILURE"),
            (ErrorClass::BusinessRejected, "MALL_REJECTED"),
            (ErrorClass::TransientFailure, "mall_timeout"),
            (ErrorClass::TransientFailure, "mall_outcome_unknown"),
        ] {
            assert!(!is_result_unknown(class, code), "code={code}");
        }
    }

    #[test]
    fn normalization_only_replaces_class() {
        assert_eq!(
            normalized_result_unknown_class(ErrorClass::TransientFailure, "MALL_TIMEOUT"),
            ErrorClass::ResultUnknown
        );
        assert_eq!(
            normalized_result_unknown_class(ErrorClass::BusinessRejected, "MALL_REJECTED"),
            ErrorClass::BusinessRejected
        );
    }
}
