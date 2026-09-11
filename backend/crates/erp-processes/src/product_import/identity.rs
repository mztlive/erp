//! 导入编号与金额规范化。

use erp_catalog::entity::catalog::product_import::{
    collapse_import_text, first_import_barcode, import_brand_name, is_import_blank, truncate_import_text,
    IMPORT_NAME_MAX_CHARS, IMPORT_SPEC_MAX_CHARS,
};
use std::collections::HashSet;

use erp_catalog::{SpecEntryInput, PRODUCT_IMPORT_HEADERS, PRODUCT_IMPORT_NAME_COLUMN};
use erp_core::money::Amount;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// SKU 规格维度名：同一产品编码下用规格值区分 SKU。
pub const SKU_SPEC_NAME: &str = "规格";
/// 规格值最大字符数，与规格签名分量上限一致。
const SKU_SPEC_VALUE_MAX_CHARS: usize = 64;

/// 一行导入所需的已规范化字段。
#[derive(Debug, Clone)]
pub struct NormalizedImportRow {
    /// Excel 行号。
    pub row_number: u32,
    /// 商品编号。
    pub product_no: String,
    /// SKU 编号。
    pub sku_no: String,
    /// 商品/SKU 名称。
    pub name: String,
    /// 规格。
    pub specification: Option<String>,
    /// 品牌名称。
    pub brand_name: String,
    /// 分类名称。
    pub category_name: String,
    /// 条码。
    pub barcode: Option<String>,
    /// 销售可见含税价。
    pub sales_price: Option<Amount>,
    /// 市场价。
    pub market_price: Option<Amount>,
    /// 是否来自模板「产品编码」列（相同编码归入同一 SPU）。
    pub coded_spu: bool,
    /// SKU 规格；有产品编码时用于区分同一 SPU 下的多个 SKU。
    pub spec_entries: Vec<SpecEntryInput>,
}

/// 把模板单元格映射为建品输入。
///
/// # 参数
/// * `row_number` - Excel 行号
/// * `cells` - 按表头顺序的单元格
///
/// # 返回
/// 返回规范化后的导入行。
///
/// # 错误
/// 名称为空、分类为空或价格无法解析时返回校验错误。
pub fn normalize_import_row(row_number: u32, cells: &[String]) -> Result<NormalizedImportRow> {
    let cell = |index: usize| cells.get(index).map(String::as_str).unwrap_or("");
    let name_raw = collapse_import_text(cell(PRODUCT_IMPORT_NAME_COLUMN - 1));
    if is_import_blank(&name_raw) {
        return Err(Error::ValidationError("产品名称不能为空".into()));
    }
    let category_name = collapse_import_text(cell(6));
    if is_import_blank(&category_name) {
        return Err(Error::ValidationError("产品类别不能为空".into()));
    }
    let spec_raw = cell(9);
    let specification = if is_import_blank(spec_raw) {
        None
    } else {
        Some(truncate_import_text(
            &collapse_import_text(spec_raw),
            IMPORT_SPEC_MAX_CHARS,
        ))
    };
    let barcode = first_import_barcode(cell(1));
    let name = truncate_import_text(&name_raw, IMPORT_NAME_MAX_CHARS);
    let coded = explicit_product_code(cell(0));
    let coded_spu = coded.is_some();
    let product_no = coded.unwrap_or_else(|| {
        stable_code(
            "PRD",
            &[
                &name,
                specification.as_deref().unwrap_or(""),
                barcode.as_deref().unwrap_or(""),
            ],
        )
    });
    let spec_entries = if coded_spu {
        vec![SpecEntryInput {
            attribute_code: SKU_SPEC_NAME.to_string(),
            attribute_value_code: sku_spec_value(specification.as_deref(), barcode.as_deref(), &name),
        }]
    } else {
        Vec::new()
    };
    Ok(NormalizedImportRow {
        row_number,
        sku_no: format!("{product_no}-01"),
        product_no,
        name,
        specification,
        brand_name: import_brand_name(cell(5)),
        category_name,
        barcode,
        sales_price: parse_optional_amount(cell(12), PRODUCT_IMPORT_HEADERS[12])?,
        market_price: parse_optional_amount(cell(16), PRODUCT_IMPORT_HEADERS[16])?,
        coded_spu,
        spec_entries,
    })
}

/// 读取模板「产品编码」；空白或占位视为未提供。
///
/// # 参数
/// * `raw` - 产品编码单元格
///
/// # 返回
/// 有编码时返回规范化编号。
///
/// # 错误
/// 无。
pub fn explicit_product_code(raw: &str) -> Option<String> {
    let trimmed = collapse_import_text(raw);
    if is_import_blank(&trimmed) {
        None
    } else {
        Some(truncate_import_text(&trimmed.replace(' ', "-"), 64))
    }
}

/// 同一产品编码下用于区分 SKU 的规格值。
///
/// # 参数
/// * `spec` - 产品规格
/// * `barcode` - 条码
/// * `name` - 商品名称
///
/// # 返回
/// 优先规格，其次条码，最后名称。
///
/// # 错误
/// 无。
pub fn sku_spec_value(spec: Option<&str>, barcode: Option<&str>, name: &str) -> String {
    if let Some(spec) = spec.filter(|value| !is_import_blank(value)) {
        return truncate_import_text(spec, SKU_SPEC_VALUE_MAX_CHARS);
    }
    if let Some(barcode) = barcode.filter(|value| !is_import_blank(value)) {
        return truncate_import_text(barcode, SKU_SPEC_VALUE_MAX_CHARS);
    }
    truncate_import_text(name, SKU_SPEC_VALUE_MAX_CHARS)
}

/// 在已占用编号中分配下一个 SKU 编号。
///
/// # 参数
/// * `product_no` - 商品编号
/// * `taken` - 已占用的 SKU 编号
///
/// # 返回
/// 返回 `{product_no}-NN`。
///
/// # 错误
/// 无。
pub fn next_sku_no(product_no: &str, taken: &HashSet<String>) -> String {
    let mut index = 1u32;
    loop {
        let candidate = format!("{product_no}-{index:02}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        index = index.saturating_add(1);
        if index == 0 {
            return format!("{product_no}-SKU");
        }
    }
}

/// 生成稳定导入代码。
///
/// # 参数
/// * `prefix` - 代码前缀
/// * `parts` - 参与哈希的文本
///
/// # 返回
/// 返回 `PREFIX-` 加 12 位十六进制摘要。
///
/// # 错误
/// 无。
pub fn stable_code(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    format!("{prefix}-{}", hex::encode(&digest[..6])).to_uppercase()
}

fn parse_optional_amount(value: &str, column: &str) -> Result<Option<Amount>> {
    if is_import_blank(value) {
        return Ok(None);
    }
    let text = collapse_import_text(value);
    text.parse::<Amount>()
        .map(Some)
        .map_err(|_| Error::ValidationError(format!("「{column}」不是有效金额")))
}

#[cfg(test)]
mod tests {
    use super::{next_sku_no, normalize_import_row, stable_code};
    use erp_catalog::PRODUCT_IMPORT_HEADERS;
    use std::collections::HashSet;

    #[test]
    fn stable_code_is_deterministic() {
        assert_eq!(stable_code("BRD", &["莲香楼"]), stable_code("BRD", &["莲香楼"]));
        assert_ne!(stable_code("BRD", &["莲香楼"]), stable_code("BRD", &["法丽兹"]));
    }

    #[test]
    fn name_and_category_are_required() {
        let mut cells = vec![String::new(); PRODUCT_IMPORT_HEADERS.len()];
        assert!(normalize_import_row(2, &cells).is_err());
        cells[8] = "莲香楼凤梨酥".into();
        assert!(normalize_import_row(2, &cells).is_err());
        cells[6] = "酥点".into();
        let row = normalize_import_row(2, &cells).unwrap();
        assert_eq!(row.category_name, "酥点");
        assert!(row.product_no.starts_with("PRD-"));
        assert!(!row.coded_spu);
        assert!(row.spec_entries.is_empty());
    }

    #[test]
    fn same_product_code_shares_spu_and_splits_skus() {
        let mut first = vec![String::new(); PRODUCT_IMPORT_HEADERS.len()];
        first[0] = "FSY-1".into();
        first[6] = "米".into();
        first[8] = "东北大米".into();
        first[9] = "5kg".into();
        let mut second = first.clone();
        second[9] = "10kg".into();
        let left = normalize_import_row(2, &first).unwrap();
        let right = normalize_import_row(3, &second).unwrap();
        assert_eq!(left.product_no, "FSY-1");
        assert_eq!(right.product_no, "FSY-1");
        assert_eq!(left.spec_entries[0].attribute_value_code, "5kg");
        assert_eq!(right.spec_entries[0].attribute_value_code, "10kg");
    }

    #[test]
    fn next_sku_no_skips_taken_codes() {
        let taken = HashSet::from(["FSY-1-01".into()]);
        assert_eq!(next_sku_no("FSY-1", &taken), "FSY-1-02");
    }
}
