//! 自动选品只消除格式差异，保留品牌、规格、包装与单位身份。

use std::sync::LazyLock;

use regex::Regex;

use super::parse_specification_signature;
use crate::{Error, Result};

/// 规范化全角 ASCII 和连续空白；不删除型号、口味、数字或标点。
/// # 参数
/// `value` 为完整名称或规格属性。
/// # 返回
/// 保留大小写和词间分隔的规范文本。
/// # 错误
/// 不返回错误。
pub fn match_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(u32::from(ch) - 0xfee0).expect("全角 ASCII 映射有效"),
            '\u{3000}' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 供应商完整规格与公司结构化规格的精确对应。
/// # 参数
/// `source` 为来源完整规格；`signature` 为公司规格签名。
/// # 返回
/// 单一「规格」属性或完整结构化签名一致时为 true；空规格不作为匹配证据。
/// # 错误
/// 公司规格签名损坏时拒绝。
pub fn same_specification(source: &str, signature: &str) -> Result<bool> {
    if source.trim().is_empty() {
        return Ok(false);
    }
    let entries = parse_specification_signature(signature)?;
    if entries.len() == 1 && entries[0].attribute_code == "规格" {
        return Ok(spec_text(source) == spec_text(&entries[0].value_code));
    }
    let Ok(source_entries) = parse_specification_signature(source) else { return Ok(false) };
    Ok(source_entries.len() == entries.len()
        && source_entries.iter().zip(entries).all(|(a, b)| {
            match_text(&a.attribute_code) == match_text(&b.attribute_code)
                && spec_text(&a.value_code) == spec_text(&b.value_code)
        }))
}

fn spec_text(value: &str) -> String {
    // 只统一明确的容量单位大小写与乘号两侧空格；不折算总容量。
    static MILLILITRES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)([0-9])\s*ml([^a-zA-Z]|$)").expect("固定毫升单位模式合法"));
    let value = match_text(value).replace('*', "×");
    let value = MILLILITRES.replace_all(&value, "${1}ml${2}");
    value.split('×').map(str::trim).collect::<Vec<_>>().join("×")
}

/// 从完整候选中取唯一身份；不按排序选第一条。
/// # 参数
/// `ids` 为已经去重的候选稳定身份。
/// # 返回
/// 无候选返回空；唯一候选返回其身份。
/// # 错误
/// 多个候选时返回明确的主数据重复错误。
pub fn unique_match(ids: &[String]) -> Result<Option<String>> {
    match ids {
        [] => Ok(None),
        [id] => Ok(Some(id.clone())),
        _ => Err(Error::ConflictError("公司商品存在多个相同身份的 SKU，请先整理重复商品后重新预览".into())),
    }
}

/// 自动关联所需的完整身份；来源规格为原文，公司规格为正式签名。
pub struct MatchIdentity<'a> {
    pub kind: super::ProductKind,
    pub brand: Option<&'a str>,
    pub specification: &'a str,
    pub unit: &'a str,
    pub barcode: Option<&'a str>,
}

impl MatchIdentity<'_> {
    /// 校验自动关联不能改变实际销售项。
    /// # 参数
    /// `company` 为公司当前身份；`active` 为商品及 SKU 的联合启用状态。
    /// # 返回
    /// 类型、已知品牌、规格、单位及已知条码全部一致时成功。
    /// # 错误
    /// 停用或任一身份冲突时拒绝，不能回落到新建。
    pub fn ensure_matches(&self, company: &MatchIdentity<'_>, active: bool) -> Result<()> {
        if !active
            || self.kind != company.kind
            || self.unit != company.unit
            || !same_specification(self.specification, company.specification)?
            || self
                .brand
                .filter(|b| !b.trim().is_empty())
                .is_some_and(|b| company.brand.is_none_or(|c| match_text(b) != match_text(c)))
        {
            return Err(Error::ConflictError(
                "匹配商品已停用，或品牌、规格、单位、商品类型不一致，请先核对商品档案".into(),
            ));
        }
        if self.barcode.zip(company.barcode).is_some_and(|(a, b)| a != b) {
            return Err(Error::ConflictError("来源条码与公司 SKU 当前条码不一致，请先核对条码归属".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::ProductKind;

    #[test]
    fn barcode_does_not_override_packaging_unit_brand_kind_or_disabled_state() {
        let source = MatchIdentity {
            kind: ProductKind::Physical,
            brand: Some("品牌"),
            specification: "250ml×12盒",
            unit: "箱",
            barcode: Some("00123"),
        };
        let mut company = MatchIdentity { specification: "规格=250ml×12盒", ..source };
        assert!(source.ensure_matches(&company, true).is_ok());
        assert!(source.ensure_matches(&company, false).is_err());
        company.specification = "规格=500ml×6盒";
        assert!(source.ensure_matches(&company, true).is_err());
        company.specification = "规格=250ml×12盒";
        company.unit = "盒";
        assert!(source.ensure_matches(&company, true).is_err());
        company.unit = "箱";
        company.brand = Some("另一品牌");
        assert!(source.ensure_matches(&company, true).is_err());
        company.brand = Some("品牌");
        company.barcode = Some("123");
        assert!(source.ensure_matches(&company, true).is_err());
        company.barcode = None;
        assert!(source.ensure_matches(&company, true).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_normalization_preserves_identity() {
        assert_eq!(match_text("  ＡＢＣ　低糖  250ml "), "ABC 低糖 250ml");
        assert!(same_specification("250ML * 12盒", "规格=250ml×12盒").unwrap());
        assert!(!same_specification("500ml×6盒", "规格=250ml×12盒").unwrap());
        assert!(!same_specification("250ml×24盒", "规格=250ml×12盒").unwrap());
        assert!(!same_specification("", "").unwrap());
        assert!(!same_specification("红色", "尺码=L|颜色=红色").unwrap());
        assert!(same_specification("尺码=L|颜色=红色", "尺码=L|颜色=红色").unwrap());
    }

    #[test]
    fn ambiguous_matches_never_choose_or_create() {
        assert_eq!(unique_match(&[]).unwrap(), None);
        assert_eq!(unique_match(&["sku".into()]).unwrap().as_deref(), Some("sku"));
        assert!(unique_match(&["sku-1".into(), "sku-2".into()]).is_err());
    }
}
