//! 来源单位缺失时的确定性建议；建议不覆盖供应商原文，也不执行数量换算。

use std::collections::BTreeSet;

use super::supplier_catalog::SupplierCatalogItem;

const PACKAGES: &[&str] = &[
    "箱", "盒", "袋", "包", "瓶", "罐", "桶", "套", "件", "个", "只", "支", "枝", "束", "枚", "片", "条",
    "块", "本", "册", "张", "卷", "双", "对", "份", "组", "提", "杯", "听", "板", "粒", "颗", "台", "把",
    "顶", "床",
];
const MEASURES: &[(&str, &str)] = &[
    ("千克", "千克"),
    ("公斤", "千克"),
    ("kg", "千克"),
    ("毫克", "毫克"),
    ("mg", "毫克"),
    ("克", "克"),
    ("g", "克"),
    ("毫升", "毫升"),
    ("ml", "毫升"),
    ("升", "升"),
    ("l", "升"),
    ("英寸", "英寸"),
    ("寸", "寸"),
    ("磅", "磅"),
    ("lbs", "磅"),
    ("lb", "磅"),
    ("斤", "斤"),
    ("两", "两"),
    ("厘米", "厘米"),
    ("cm", "厘米"),
    ("毫米", "毫米"),
    ("mm", "毫米"),
    ("米", "米"),
    ("m", "米"),
];

impl SupplierCatalogItem {
    /// 提供来源单位或从规格、名称提取的常用单位建议。
    /// # 参数
    /// 无，读取当前商品的来源字段。
    /// # 返回
    /// 明确来源优先，其次规格、名称；冲突或无依据返回空。
    /// # 错误
    /// 无；不将建议写回来源字段。
    pub fn suggested_unit(&self) -> Option<String> {
        suggest(self.unit.as_deref(), &self.specification, &self.name)
    }
}

fn suggest(source: Option<&str>, specification: &str, name: &str) -> Option<String> {
    if let Some(unit) = source.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(unit.into());
    }
    let (unit, found) = extract(specification);
    if found {
        return unit;
    }
    extract(name).0
}

fn extract(text: &str) -> (Option<String>, bool) {
    let text = text.to_lowercase();
    // 外包装明确时优先：250ml/瓶、12盒/箱、礼盒、袋装。
    let packaging: BTreeSet<_> = PACKAGES
        .iter()
        .copied()
        .filter(|unit| {
            text.ends_with(&format!("/{unit}"))
                || text.ends_with(&format!("／{unit}"))
                || text.contains(&format!("{unit}装"))
                || (*unit == "盒" && text.contains("礼盒"))
        })
        .collect();
    if !packaging.is_empty() {
        return unique(packaging);
    }
    let packages: BTreeSet<_> =
        PACKAGES.iter().copied().filter(|unit| text.trim() == *unit || numbered(&text, unit)).collect();
    if !packages.is_empty() {
        return unique(packages);
    }
    let measures: BTreeSet<_> = MEASURES
        .iter()
        .filter_map(|(token, unit)| (text.trim() == *token || numbered(&text, token)).then_some(*unit))
        .collect();
    unique(measures)
}

fn unique(units: BTreeSet<&str>) -> (Option<String>, bool) {
    (if units.len() == 1 { units.first().map(|u| (*u).into()) } else { None }, !units.is_empty())
}

fn numbered(text: &str, token: &str) -> bool {
    text.match_indices(token).any(|(start, _)| {
        let preceding = text[..start].trim_end().chars().next_back();
        let following = text[start + token.len()..].chars().next();
        preceding
            .is_some_and(|c| c.is_ascii_digit() || "０１２３４５６７８９一二三四五六七八九十".contains(c))
            && (!token.is_ascii() || following.is_none_or(|c| !c.is_ascii_alphabetic()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_specifications_and_packaging_are_inferred() {
        for (spec, name, expected) in [
            ("3磅", "蛋糕", "磅"),
            ("1.0磅", "蛋糕", "磅"),
            ("6寸", "蛋糕", "寸"),
            ("500g", "茶叶", "克"),
            ("1.5 KG", "米", "千克"),
            ("500ml", "水", "毫升"),
            ("250ml×12盒", "牛奶", "盒"),
            ("250ml×12盒/箱", "牛奶", "箱"),
            ("红色", "永生花玫瑰礼盒/红色", "盒"),
            ("", "99枝玫瑰", "枝"),
            ("2袋装", "零食", "袋"),
            ("6英寸", "蛋糕", "英寸"),
            ("盒", "礼品", "盒"),
            ("500g/袋", "食品", "袋"),
            ("３磅", "蛋糕", "磅"),
        ] {
            assert_eq!(suggest(None, spec, name).as_deref(), Some(expected), "{spec} {name}");
        }
    }

    #[test]
    fn explicit_unit_wins_and_ambiguous_or_missing_evidence_stays_unknown() {
        assert_eq!(suggest(Some("  个  "), "3磅", "蛋糕").as_deref(), Some("个"));
        for spec in ["", "红色", "2019款", "3磅或6寸", "2袋+3盒", "500gold"] {
            assert_eq!(suggest(Some(" "), spec, "普通商品"), None, "{spec}");
        }
        assert_eq!(suggest(None, "2袋+3盒", "礼盒"), None);
    }
}
