//! 规格签名与身份排序位值对象（数据模型 §6.3 必需约束）。
//!
//! - 规格签名按「属性代码、属性值代码排序后的规范化序列」计算，不受显示顺序、名称
//!   或旧系统 JSON 字段顺序影响；无规格 SKU 使用固定空规格签名
//!   [`EMPTY_SPEC_SIGNATURE`]，确保同一 SPU 最多一个无规格 SKU；
//! - `sku_revision_attribute_value.identity_position` 是规范化排序位置，跨行组合必须
//!   构成 `0..n` 的完整排列（无重复、无越界），由 [`validate_identity_positions`] 判定。

use std::fmt;

use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

/// 无规格 SKU 的固定空规格签名（数据模型 §6.3）。
pub const EMPTY_SPEC_SIGNATURE: &str = "";

/// 规格签名最大长度。
const SIGNATURE_MAX_LEN: usize = 512;
/// 属性代码/属性值代码最大长度。
const CODE_MAX_LEN: usize = 64;

/// 一条参与签名计算的规格属性-值对（P3 由字典代码回填后调用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecSignatureEntry {
    /// 属性代码。
    pub attribute_code: String,
    /// 属性值代码。
    pub value_code: String,
}

/// 计算规范化规格签名。
///
/// 规则（数据模型 §6.3）：空列表返回 [`EMPTY_SPEC_SIGNATURE`]；否则把每条
/// `attribute_code=value_code` 按 `(attribute_code, value_code)` 字典序排序后以 `|`
/// 连接。同一属性出现两次视为规格数据不一致，直接拒绝。
///
/// # 参数
/// * `entries` - 该 SKU 的全部规格属性-值对（无序即可）
///
/// # 返回
/// 返回规范化后的签名（空规格为 [`EMPTY_SPEC_SIGNATURE`]）。
///
/// # 错误
/// 代码为空/超长，或同一属性代码出现多次时返回错误。
pub fn compute_specification_signature(entries: &[SpecSignatureEntry]) -> Result<String> {
    if entries.is_empty() {
        return Ok(EMPTY_SPEC_SIGNATURE.to_string());
    }

    let mut normalized: Vec<(String, String)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let attribute_code = normalize_required_text(
            entry.attribute_code.clone(),
            "属性代码不能为空",
            CODE_MAX_LEN,
            "属性代码过长",
        )?;
        let value_code = normalize_required_text(
            entry.value_code.clone(),
            "属性值代码不能为空",
            CODE_MAX_LEN,
            "属性值代码过长",
        )?;
        normalized.push((attribute_code, value_code));
    }
    normalized.sort();

    let mut signature = String::with_capacity(normalized.len() * 24);
    let mut previous_attribute: Option<&str> = None;
    for (attribute_code, value_code) in &normalized {
        if previous_attribute == Some(attribute_code.as_str()) {
            return Err(Error::from("同一属性在规格中出现多次"));
        }
        if !signature.is_empty() {
            signature.push('|');
        }
        signature.push_str(attribute_code);
        signature.push('=');
        signature.push_str(value_code);
        previous_attribute = Some(attribute_code);
    }

    if signature.chars().count() > SIGNATURE_MAX_LEN {
        return Err(Error::from("规格签名过长"));
    }
    Ok(signature)
}

/// 校验规格签名是否可写入 `sku`。
///
/// 规则（数据模型 §6.3）：空签名只允许使用固定空规格签名
/// [`EMPTY_SPEC_SIGNATURE`]；非空签名不得全空白且不得超过长度上限。
///
/// # 参数
/// * `signature` - 待写入的规范化签名
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// 签名超长，或空签名不是规范空值形态时返回错误。
pub fn validate_specification_signature(signature: &str) -> Result<()> {
    let trimmed = signature.trim();
    if trimmed.is_empty() {
        if signature != EMPTY_SPEC_SIGNATURE {
            return Err(Error::from("空规格签名必须为固定空规格签名"));
        }
        return Ok(());
    }
    if trimmed.chars().count() > SIGNATURE_MAX_LEN {
        return Err(Error::from("规格签名过长"));
    }
    Ok(())
}

/// 校验一组身份排序位是否构成 `0..len` 的完整排列。
///
/// 数据模型 §6.3 要求 `identity_position` 是规范化排序位置：n 条规格关系的位置
/// 必须恰好是 `0..n` 各一次（无重复、无越界），否则规格集合不一致。
///
/// # 参数
/// * `positions` - 同一 SKU 修订的全部身份排序位
///
/// # 返回
/// 构成完整排列时返回 `Ok(())`。
///
/// # 错误
/// 位置重复或越界时返回错误。
pub fn validate_identity_positions(positions: &[u32]) -> Result<()> {
    let mut seen = vec![false; positions.len()];
    for &position in positions {
        let index = usize::try_from(position).map_err(|_| Error::from("身份排序位超出可用范围"))?;
        let slot = seen.get_mut(index).ok_or_else(|| Error::from("身份排序位越界"))?;
        if *slot {
            return Err(Error::from("身份排序位重复"));
        }
        *slot = true;
    }
    Ok(())
}

/// 签名与排序位置的内部展示（调试用，不参与持久化）。
impl fmt::Display for SpecSignatureEntry {
    /// 以 `attribute_code=value_code` 形式展示。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}={}", self.attribute_code, self.value_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(attribute_code: &str, value_code: &str) -> SpecSignatureEntry {
        SpecSignatureEntry {
            attribute_code: attribute_code.to_string(),
            value_code: value_code.to_string(),
        }
    }

    /// 签名按属性代码、属性值代码排序，且与输入顺序无关；空规格返回固定空签名。
    #[test]
    fn compute_signature_is_canonical_and_order_independent() {
        let unsorted = vec![
            entry(" color ", " 白色 "),
            entry("size", "S"),
            entry("material", "棉"),
        ];
        let sorted = vec![
            entry("size", "S"),
            entry("material", "棉"),
            entry("color", "白色"),
        ];

        let a = compute_specification_signature(&unsorted).unwrap();
        let b = compute_specification_signature(&sorted).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, "color=白色|material=棉|size=S");

        assert_eq!(
            compute_specification_signature(&[]).unwrap(),
            EMPTY_SPEC_SIGNATURE
        );
    }

    /// 同一属性出现两次（值不同）属于规格数据不一致，拒绝计算。
    #[test]
    fn compute_signature_rejects_duplicate_attribute() {
        let duplicated = vec![entry("size", "S"), entry("size", "L")];
        assert!(compute_specification_signature(&duplicated).is_err());
    }

    /// 签名校验：固定空签名合法，其余空白/超长拒绝。
    #[test]
    fn validate_signature_accepts_only_canonical_shapes() {
        assert!(validate_specification_signature(EMPTY_SPEC_SIGNATURE).is_ok());
        assert!(validate_specification_signature("size=L").is_ok());
        assert!(validate_specification_signature(" ").is_err());
        assert!(validate_specification_signature(&"a=b|".repeat(200)).is_err());
    }

    /// 身份排序位必须构成 0..n 完整排列：重复与越界被拒绝，完整排列通过。
    #[test]
    fn identity_positions_must_form_complete_permutation() {
        assert!(validate_identity_positions(&[]).is_ok());
        assert!(validate_identity_positions(&[0, 1, 2]).is_ok());

        assert!(validate_identity_positions(&[0, 0]).is_err());
        assert!(validate_identity_positions(&[0, 1, 1]).is_err());
        assert!(validate_identity_positions(&[0, 2]).is_err());
        assert!(validate_identity_positions(&[0, 1, 3]).is_err());
    }

    /// 签名中的代码去首尾空白并拒绝超长。
    #[test]
    fn compute_signature_trims_codes_and_rejects_overlong() {
        let padded = vec![entry("  size  ", "  L  ")];
        assert_eq!(compute_specification_signature(&padded).unwrap(), "size=L");

        let overlong_code = vec![entry(&"a".repeat(65), "L")];
        assert!(compute_specification_signature(&overlong_code).is_err());
    }
}
