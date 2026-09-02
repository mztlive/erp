//! 规格签名与身份排序位值对象（数据模型 §6.3 必需约束）。
//!
//! - 规格签名按「SPU 局部规格名、规格值排序后的规范化序列」计算，不受显示顺序
//!   或旧系统 JSON 字段顺序影响；无规格 SKU 使用固定空规格签名
//!   [`EMPTY_SPEC_SIGNATURE`]，确保同一 SPU 最多一个无规格 SKU；
//! - `sku_revision_attribute_value.identity_position` 是规范化排序位置，跨行组合必须
//!   构成 `0..n` 的完整排列（无重复、无越界），由 [`validate_identity_positions`] 判定。

use std::collections::HashSet;
use std::fmt;

use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

/// 无规格 SKU 的固定空规格签名（数据模型 §6.3）。
pub const EMPTY_SPEC_SIGNATURE: &str = "";

/// 规格签名最大长度。
const SIGNATURE_MAX_LEN: usize = 512;
/// 规格名/规格值最大长度。
const CODE_MAX_LEN: usize = 64;

/// 一条参与签名计算的 SPU 局部规格名-值对。
///
/// 字段名为兼容既有内部调用保留，不表示必须引用全局字典代码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecSignatureEntry {
    /// SPU 局部规格名。
    pub attribute_code: String,
    /// SPU 局部规格值。
    pub value_code: String,
}

/// 一次商品规格编辑中的唯一签名集合。
#[derive(Debug, Default)]
pub struct SpecificationSignatureSet {
    signatures: HashSet<String>,
}

impl SpecificationSignatureSet {
    /// 创建空的规格签名集合。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回不包含任何签名的集合。
    ///
    /// # 错误
    /// 无。
    pub fn new() -> Self {
        Self::default()
    }

    /// 规范化一组规格名值并登记其唯一签名。
    ///
    /// # 参数
    /// * `entries` - 一个 SKU 的全部 SPU 局部规格名和值
    ///
    /// # 返回
    /// 返回规范化签名，供 SKU 稳定身份和后续关系匹配使用。
    ///
    /// # 错误
    /// 规格名值非法或同一商品编辑中已登记相同签名时返回领域错误。
    pub fn register(&mut self, entries: &[SpecSignatureEntry]) -> Result<String> {
        let signature = compute_specification_signature(entries)?;
        self.register_signature(signature.clone())?;
        Ok(signature)
    }

    /// 登记一个已规范化的规格签名。
    ///
    /// # 参数
    /// * `signature` - 已由规格值对象计算出的规范化签名
    ///
    /// # 返回
    /// 首次登记时返回 `Ok(())`。
    ///
    /// # 错误
    /// 同一商品编辑中已存在相同签名时返回领域错误。
    pub fn register_signature(&mut self, signature: String) -> Result<()> {
        if !self.signatures.insert(signature) {
            return Err(Error::from("规格集合中存在重复签名"));
        }
        Ok(())
    }

    /// 判断集合是否已登记给定规范化签名。
    ///
    /// # 参数
    /// * `signature` - 待查询的规范化签名
    ///
    /// # 返回
    /// 已登记时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn contains(&self, signature: &str) -> bool {
        self.signatures.contains(signature)
    }
}

/// 计算规范化规格签名。
///
/// 规则（数据模型 §6.3）：空列表返回 [`EMPTY_SPEC_SIGNATURE`]；否则把每条
/// `规格名=规格值` 按 `(规格名, 规格值)` 字典序排序后以 `|`
/// 连接。同一属性出现两次视为规格数据不一致，直接拒绝。
///
/// # 参数
/// * `entries` - 该 SKU 的全部规格属性-值对（无序即可）
///
/// # 返回
/// 返回规范化后的签名（空规格为 [`EMPTY_SPEC_SIGNATURE`]）。
///
/// # 错误
/// 规格名/值为空或超长，或同一规格名出现多次时返回错误。
pub fn compute_specification_signature(entries: &[SpecSignatureEntry]) -> Result<String> {
    if entries.is_empty() {
        return Ok(EMPTY_SPEC_SIGNATURE.to_string());
    }

    let mut normalized: Vec<(String, String)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let attribute_code = normalize_required_text(
            entry.attribute_code.clone(),
            "规格名不能为空",
            CODE_MAX_LEN,
            "规格名过长",
        )?;
        let value_code = normalize_required_text(
            entry.value_code.clone(),
            "规格值不能为空",
            CODE_MAX_LEN,
            "规格值过长",
        )?;
        normalized.push((attribute_code, value_code));
    }
    normalized.sort();

    let mut signature = String::with_capacity(normalized.len() * 24);
    let mut previous_attribute: Option<&str> = None;
    for (attribute_code, value_code) in &normalized {
        if previous_attribute == Some(attribute_code.as_str()) {
            return Err(Error::from("同一规格名在 SKU 规格中出现多次"));
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

/// 已持久化规格签名的读取形态。
///
/// 严格解析见 [`parse_specification_signature`]。公司商品池列表在签名审计清零前
/// 对历史非法签名使用 [`SpecificationSignatureRead::LegacyNonCanonical`]，不失败整页。
/// 审计入口是 catalog 仓储 `noncanonical_specification_signature_sku_ids`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecificationSignatureRead {
    /// 与写入合同一致的规范签名。
    Canonical(Vec<SpecSignatureEntry>),
    /// 历史非法签名：列表读取兼容为空属性，待审计清零后可改回失败关闭。
    LegacyNonCanonical,
}

/// 读取已持久化规格签名；非法历史行标为兼容形态而不是抛出领域错误。
///
/// # 参数
/// * `signature` - 已持久化的规格签名
///
/// # 返回
/// 规范签名返回条目；非法签名返回 [`SpecificationSignatureRead::LegacyNonCanonical`]。
///
/// # 错误
/// 无。严格失败关闭请调用 [`parse_specification_signature`]。
pub fn read_specification_signature(signature: &str) -> SpecificationSignatureRead {
    match parse_specification_signature(signature) {
        Ok(entries) => SpecificationSignatureRead::Canonical(entries),
        Err(_) => SpecificationSignatureRead::LegacyNonCanonical,
    }
}

/// 解析已持久化的规范化规格签名。
///
/// 与 [`compute_specification_signature`] 共用 `规格名=规格值` 并以 `|` 连接的
/// 格式合同：空签名返回空列表；非空签名必须等于对其条目再编码的规范结果。
/// 写入路径已保证规范形态。历史非法签名由 [`read_specification_signature`]
/// 标记为兼容读取，审计清零前不得在列表接口失败关闭整页。
///
/// # 参数
/// * `signature` - 已持久化的规格签名
///
/// # 返回
/// 返回按签名既有顺序排列的规格名-值对；空签名返回空集合。
///
/// # 错误
/// 缺失 `=`、空名称、空值、重复属性、超长或非规范形态时返回领域错误。
pub fn parse_specification_signature(signature: &str) -> Result<Vec<SpecSignatureEntry>> {
    if signature == EMPTY_SPEC_SIGNATURE {
        return Ok(Vec::new());
    }
    validate_specification_signature(signature)?;
    let mut entries = Vec::new();
    for fragment in signature.split('|') {
        let Some((name, value)) = fragment.split_once('=') else {
            return Err(Error::from("规格签名格式非法"));
        };
        entries.push(SpecSignatureEntry {
            attribute_code: name.to_string(),
            value_code: value.to_string(),
        });
    }
    let canonical = compute_specification_signature(&entries)?;
    if canonical != signature {
        return Err(Error::from("规格签名不是规范形态"));
    }
    Ok(entries)
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
    /// 以 `规格名=规格值` 形式展示。
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

    /// 签名按规格名、规格值排序，且与输入顺序无关；空规格返回固定空签名。
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

    /// 中文自由规格不依赖预建字典即可形成稳定签名。
    #[test]
    fn compute_signature_accepts_spu_local_names_and_values() {
        let entries = vec![entry(" 尺码 ", " L "), entry("颜色", " 红色 ")];

        assert_eq!(
            compute_specification_signature(&entries).unwrap(),
            "尺码=L|颜色=红色"
        );
    }

    /// 编辑签名集合登记规范化结果并拒绝重复规格组合。
    #[test]
    fn signature_set_rejects_duplicate_combinations() {
        let mut signatures = SpecificationSignatureSet::new();
        let first = signatures
            .register(&[entry("颜色", "红色"), entry("尺码", "L")])
            .unwrap();

        assert!(signatures.contains(&first));
        assert!(signatures
            .register(&[entry("尺码", "L"), entry("颜色", "红色")])
            .is_err());
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

    /// 签名中的规格名和值去首尾空白并拒绝超长。
    #[test]
    fn compute_signature_trims_codes_and_rejects_overlong() {
        let padded = vec![entry("  size  ", "  L  ")];
        assert_eq!(compute_specification_signature(&padded).unwrap(), "size=L");

        let overlong_code = vec![entry(&"a".repeat(65), "L")];
        assert!(compute_specification_signature(&overlong_code).is_err());
    }

    /// 空签名解析为空集合；合法多项保持规范顺序。
    #[test]
    fn parse_signature_accepts_empty_and_canonical_entries() {
        assert!(parse_specification_signature(EMPTY_SPEC_SIGNATURE)
            .unwrap()
            .is_empty());
        let entries = parse_specification_signature("尺码=L|颜色=红色").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].attribute_code, "尺码");
        assert_eq!(entries[0].value_code, "L");
        assert_eq!(entries[1].attribute_code, "颜色");
        assert_eq!(entries[1].value_code, "红色");
    }

    /// 编码后再解析必须得到规范内容，且与输入顺序无关的编码结果稳定。
    #[test]
    fn parse_signature_round_trips_canonical_encoding() {
        let encoded = compute_specification_signature(&[entry("颜色", "红色"), entry("尺码", "L")]).unwrap();
        let parsed = parse_specification_signature(&encoded).unwrap();

        assert_eq!(encoded, "尺码=L|颜色=红色");
        assert_eq!(parsed, vec![entry("尺码", "L"), entry("颜色", "红色")]);
    }

    /// 缺失 `=`、空名称、空值、重复属性和超长签名均失败关闭。
    #[test]
    fn parse_signature_rejects_illegal_persisted_shapes() {
        assert!(parse_specification_signature("尺码L").is_err());
        assert!(parse_specification_signature("=L").is_err());
        assert!(parse_specification_signature("尺码=").is_err());
        assert!(parse_specification_signature("尺码=L|尺码=S").is_err());
        assert!(parse_specification_signature(&"a=b|".repeat(200)).is_err());
        assert!(parse_specification_signature("颜色=红色|尺码=L").is_err());
    }

    /// 列表读取把非法历史签名标为兼容形态，不把整页变成解析错误。
    #[test]
    fn read_signature_versions_illegal_history_as_legacy() {
        assert!(matches!(
            read_specification_signature(EMPTY_SPEC_SIGNATURE),
            SpecificationSignatureRead::Canonical(entries) if entries.is_empty()
        ));
        assert!(matches!(
            read_specification_signature("尺码=L|颜色=红色"),
            SpecificationSignatureRead::Canonical(entries) if entries.len() == 2
        ));
        assert!(matches!(
            read_specification_signature("尺码L"),
            SpecificationSignatureRead::LegacyNonCanonical
        ));
        assert!(matches!(
            read_specification_signature("颜色=红色|尺码=L"),
            SpecificationSignatureRead::LegacyNonCanonical
        ));
    }
}
