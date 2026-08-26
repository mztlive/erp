//! 供应商经营类目与付款条件快照的拆分规则。
//!
//! 早期前端把经营类目编码进商务版本 `payment_term_snapshot`（`现结｜经营类目：礼盒`）。
//! 经营类目现为独立字段；读写都必须把付款条件与类目拆开，禁止再混入付款条件代码。

use crate::errors::Result;
use crate::validation::normalize_optional_text;

/// 经营类目最大长度（字符）。
const BUSINESS_CATEGORY_MAX_LEN: usize = 64;
/// 历史编码使用的类目标记。
const CATEGORY_LABEL: &str = "经营类目：";

/// 从付款条件快照拆出的付款条件代码与经营类目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentTermSnapshotParts {
    /// 不含经营类目编码的付款条件代码或结算文案。
    pub payment_term_code: String,
    /// 快照内编码的经营类目；无标记时为空。
    pub business_category: Option<String>,
}

/// 把历史编码的付款条件快照拆成付款条件与经营类目。
///
/// 同时识别全角 `｜` 与半角 `|`，并容忍标记前后空白。无标记时整串视为付款条件。
///
/// # 参数
/// * `raw` - 商务版本付款条件快照或采购单付款条件代码
///
/// # 返回
/// 付款条件代码（可能为空串）与可选经营类目。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 拆分必须确定：同一输入始终得到同一对字段，供采购单落单与审核展示共用。
pub fn split_encoded_payment_term_snapshot(raw: &str) -> PaymentTermSnapshotParts {
    let trimmed = raw.trim();
    let Some(label_at) = trimmed.find(CATEGORY_LABEL) else {
        return PaymentTermSnapshotParts {
            payment_term_code: trimmed.to_string(),
            business_category: None,
        };
    };
    let prefix = trimmed[..label_at].trim_end();
    let payment_term_code = strip_trailing_pipe(prefix).to_string();
    let category = trimmed[label_at + CATEGORY_LABEL.len()..].trim();
    PaymentTermSnapshotParts {
        payment_term_code,
        business_category: (!category.is_empty()).then(|| category.to_string()),
    }
}

/// 规范化可选经营类目（去空白、空值视为未登记、长度上限）。
///
/// # 参数
/// * `value` - 原始经营类目
///
/// # 返回
/// 空白返回 `None`，否则返回去空白后的类目。
///
/// # 错误
/// 超过 64 个字符时返回错误。
pub fn normalize_business_category(value: Option<String>) -> Result<Option<String>> {
    normalize_optional_text(value, "经营类目", BUSINESS_CATEGORY_MAX_LEN)
}

/// 去掉付款条件与类目标记之间的分隔竖线。
///
/// # 参数
/// * `prefix` - 类目标记之前、已去掉右侧空白的前缀
///
/// # 返回
/// 去掉末尾 `|` / `｜` 后再 trim 的付款条件。
///
/// # 错误
/// 无。
fn strip_trailing_pipe(prefix: &str) -> &str {
    prefix
        .strip_suffix('|')
        .or_else(|| prefix.strip_suffix('｜'))
        .map(str::trim_end)
        .unwrap_or(prefix)
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_business_category, split_encoded_payment_term_snapshot, PaymentTermSnapshotParts,
        BUSINESS_CATEGORY_MAX_LEN,
    };

    fn parts(code: &str, category: Option<&str>) -> PaymentTermSnapshotParts {
        PaymentTermSnapshotParts {
            payment_term_code: code.to_string(),
            business_category: category.map(ToOwned::to_owned),
        }
    }

    /// 全角/半角竖线、标记前后空白都能拆出结算文案与类目。
    #[test]
    fn split_recognizes_fullwidth_and_ascii_marks() {
        assert_eq!(
            split_encoded_payment_term_snapshot("现结｜经营类目：礼盒"),
            parts("现结", Some("礼盒"))
        );
        assert_eq!(
            split_encoded_payment_term_snapshot("先用后付 | 经营类目：礼盒"),
            parts("先用后付", Some("礼盒"))
        );
        assert_eq!(
            split_encoded_payment_term_snapshot(" 预付款｜ 经营类目： 鲜花 "),
            parts("预付款", Some("鲜花"))
        );
    }

    /// 无标记时整串就是付款条件，空白输入得到空代码。
    #[test]
    fn split_without_mark_keeps_payment_term_only() {
        assert_eq!(
            split_encoded_payment_term_snapshot("PREPAY_30"),
            parts("PREPAY_30", None)
        );
        assert_eq!(
            split_encoded_payment_term_snapshot("  NET-30  "),
            parts("NET-30", None)
        );
        assert_eq!(split_encoded_payment_term_snapshot("   "), parts("", None));
    }

    /// 标记存在但类目为空时只保留付款条件。
    #[test]
    fn split_drops_blank_category() {
        assert_eq!(
            split_encoded_payment_term_snapshot("现结｜经营类目：  "),
            parts("现结", None)
        );
    }

    /// 经营类目去空白、空值清除，超长拒绝。
    #[test]
    fn normalize_trims_clears_and_rejects_overlong() {
        assert_eq!(
            normalize_business_category(Some(" 礼盒 ".to_string()))
                .unwrap()
                .as_deref(),
            Some("礼盒")
        );
        assert_eq!(normalize_business_category(Some("  ".to_string())).unwrap(), None);
        assert_eq!(normalize_business_category(None).unwrap(), None);
        assert!(normalize_business_category(Some("x".repeat(BUSINESS_CATEGORY_MAX_LEN + 1))).is_err());
    }
}
