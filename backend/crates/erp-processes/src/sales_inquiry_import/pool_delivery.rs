//! 重算系统配送联动公式；不执行任意公式，也不信任文件中的公式缓存。

use std::collections::BTreeMap;
use std::sync::LazyLock;

use erp_sales::entity::sales_inquiry_import::InquiryImportIssue;
use erp_sales::entity::sales_inquiry_pool::{
    POOL_DELIVERY_HEADERS as HEADERS, POOL_DELIVERY_SOURCE_HEADERS as SOURCE_FIELDS, POOL_SHEET,
    POOL_TABLE_NAME, automatic_delivery_formula,
};
use regex::{Regex, escape};

use super::parse::{Cell, DELIVERY_SHEET, Row, data_rows, issue};
use crate::{Error, Result};

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"MATCH\("(line-[1-9][0-9]*)",{}\[配送关联\],0\)"#, escape(POOL_TABLE_NAME)))
        .expect("static link regex")
});

struct SourceProduct {
    name: String,
    sku: String,
    spu: String,
    quantity: String,
}

/// 按选品中的稳定关联重算白名单自动列，再映射为已有配送校验输入。
/// # 参数
/// `products` 保留完整选品页，`rows` 为配送原始行，`errors` 收集逐行错误。
/// # 返回
/// 返回不含名称、SPU 等展示字段的配送行，保留原始行号。
/// # 错误
/// 必要选品列缺失或关联重复时拒绝；任意其他公式交给统一公式校验拒绝。
pub(super) fn delivery_rows(
    products: &[Row],
    mut rows: Vec<Row>,
    errors: &mut Vec<InquiryImportIssue>,
) -> Result<Vec<Row>> {
    let sources = source_products(products)?;
    for row in rows.iter_mut().filter(|row| row.number > 1) {
        resolve_row(row, &sources, errors);
    }
    Ok(data_rows(rows, DELIVERY_SHEET, &HEADERS, errors)
        .into_iter()
        .map(|row| Row {
            number: row.number,
            cells: [1, 3, 4, 5, 6, 7, 8, 9, 10, 11].map(|index| row.cells[index].clone()).to_vec(),
        })
        .collect())
}

fn source_products(rows: &[Row]) -> Result<BTreeMap<String, SourceProduct>> {
    let header = rows.iter().find(|row| row.number == 2).ok_or_else(missing_source)?;
    let columns = SOURCE_FIELDS
        .iter()
        .map(|name| header.cells.iter().position(|cell| cell.value == *name).ok_or_else(missing_source))
        .collect::<Result<Vec<_>>>()?;
    let mut sources = BTreeMap::new();
    for row in rows.iter().filter(|row| row.number > 2) {
        if row.cells.iter().all(|cell| cell.value.is_empty() && !cell.formula) {
            continue;
        }
        let values = columns
            .iter()
            .map(|index| row.cells.get(*index).map(|cell| cell.value.clone()).unwrap_or_default())
            .collect::<Vec<_>>();
        let product = SourceProduct {
            name: values[1].clone(),
            sku: values[2].clone(),
            spu: values[3].clone(),
            quantity: values[4].clone(),
        };
        if values[0].is_empty() || sources.insert(values[0].clone(), product).is_some() {
            return Err(Error::ValidationError(format!(
                "{POOL_SHEET}第 {} 行配送关联缺失或重复，请保留原导出的隐藏关联列",
                row.number
            )));
        }
    }
    Ok(sources)
}

fn missing_source() -> Error {
    Error::ValidationError("请保留选品页的商品名称、SKU 编号、SPU 编号、需求数量及隐藏的配送关联列".into())
}

fn resolve_row(
    row: &mut Row,
    sources: &BTreeMap<String, SourceProduct>,
    errors: &mut Vec<InquiryImportIssue>,
) {
    let mut keys = Vec::new();
    for (column, field) in [(0, "商品名称"), (1, "SKU 编号"), (2, "SPU 编号"), (9, "需求数量")] {
        let Some(cell) = row.cells.get_mut(column).filter(|cell| cell.formula) else { continue };
        let Some(key) = formula_key(cell, field) else { continue };
        let Some(source) = sources.get(&key) else {
            issue(
                errors,
                DELIVERY_SHEET,
                row.number,
                HEADERS[column],
                "关联商品已从选品页移除，请核对该行配送资料",
            );
            continue;
        };
        let value = if source.quantity.is_empty() {
            ""
        } else {
            match column {
                0 => &source.name,
                1 => &source.sku,
                2 => &source.spu,
                _ => &source.quantity,
            }
        };
        *cell = Cell { value: value.into(), text: true, ..Default::default() };
        keys.push(key);
    }
    if let Some(key) = keys.first() {
        let source = &sources[key];
        let sku = row.cells.get(1).map_or("", |cell| cell.value.as_str());
        if keys.iter().any(|other| other != key) || (!sku.is_empty() && sku != source.sku) {
            issue(
                errors,
                DELIVERY_SHEET,
                row.number,
                "SKU 编号",
                "自动商品信息与该行 SKU 不一致，请核对配送行，避免地址错配",
            );
        }
    }
}

fn formula_key(cell: &Cell, field: &str) -> Option<String> {
    let formula = cell.formula_text.as_deref()?;
    let key = LINK_RE.captures(formula)?.get(1)?.as_str();
    (formula == automatic_delivery_formula(key, field)).then(|| key.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Cell {
        Cell { value: value.into(), text: true, ..Default::default() }
    }

    fn source(key: &str, sku: &str, quantity: &str, number: u32) -> Row {
        Row { number, cells: [key, "商品名称", sku, "000-SPU", quantity].map(text).to_vec() }
    }

    fn sources(quantity: &str) -> Vec<Row> {
        vec![
            Row { number: 2, cells: SOURCE_FIELDS.map(text).to_vec() },
            source("line-1", "00123", quantity, 3),
        ]
    }

    fn automatic_row(number: u32) -> Row {
        let mut row = Row { number, cells: vec![Cell::default(); 12] };
        for (index, field) in [(0, "商品名称"), (1, "SKU 编号"), (2, "SPU 编号"), (9, "需求数量")]
        {
            row.cells[index] = Cell {
                value: "incorrect-cache".into(),
                formula: true,
                formula_text: Some(automatic_delivery_formula("line-1", field)),
                ..Default::default()
            };
        }
        row
    }

    fn parse_row(products: &[Row], row: Row, errors: &mut Vec<InquiryImportIssue>) -> Vec<Row> {
        delivery_rows(products, vec![Row { number: 1, cells: HEADERS.map(text).to_vec() }, row], errors)
            .unwrap()
    }

    #[test]
    fn automatic_columns_recompute_from_selection_and_ignore_cached_values() {
        let mut errors = vec![];
        let rows = parse_row(&sources("2.5"), automatic_row(8), &mut errors);
        assert!(errors.is_empty());
        assert_eq!(rows[0].number, 8);
        assert_eq!(rows[0].cells[0].value, "00123");
        assert!(rows[0].cells[0].text);
        assert_eq!(rows[0].cells[7].value, "2.5");
        assert!(!rows[0].cells.iter().any(|cell| cell.formula));
    }

    #[test]
    fn deselection_clears_automatic_values_but_preserves_entered_address_for_validation() {
        let mut errors = vec![];
        assert!(parse_row(&sources(""), automatic_row(8), &mut errors).is_empty());
        let mut row = automatic_row(8);
        row.cells[3] = text("收件人");
        row.cells[4] = text("0012345");
        let rows = parse_row(&sources(""), row, &mut errors);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].cells[0].value.is_empty());
        assert_eq!(rows[0].cells[1].value, "收件人");
        assert_eq!(rows[0].cells[2].value, "0012345");
    }

    #[test]
    fn selection_sorting_keeps_sku_and_address_binding_and_manual_split_quantity() {
        let mut products = sources("6");
        products[1].number = 4;
        products.insert(1, source("line-2", "002", "9", 3));
        let mut row = automatic_row(12);
        row.cells[3] = text("固定收件人");
        row.cells[9] = text("2");
        let mut errors = vec![];
        let rows = parse_row(&products, row, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(rows[0].cells[0].value, "00123");
        assert_eq!(rows[0].cells[1].value, "固定收件人");
        assert_eq!(rows[0].cells[7].value, "2");
    }

    #[test]
    fn changed_formulas_mixed_keys_and_deleted_sources_are_rejected() {
        let mut row = automatic_row(8);
        row.cells[9].formula_text = Some("1+1".into());
        let mut errors = vec![];
        parse_row(&sources("2"), row, &mut errors);
        assert!(errors.iter().any(|error| error.column == "配送数量"));
        let mut row = automatic_row(8);
        row.cells[1] = text("other-sku");
        let mut errors = vec![];
        parse_row(&sources("2"), row, &mut errors);
        assert!(errors.iter().any(|error| error.message.contains("地址错配")));
        let mut errors = vec![];
        parse_row(&sources("2")[..1], automatic_row(8), &mut errors);
        assert!(errors.iter().any(|error| error.message.contains("已从选品页移除")));
    }

    #[test]
    fn duplicate_source_links_and_missing_identity_columns_are_rejected() {
        let mut products = sources("2");
        products.push(source("line-1", "002", "3", 4));
        assert!(delivery_rows(&products, vec![], &mut vec![]).is_err());
        assert!(delivery_rows(&[], vec![], &mut vec![]).is_err());
    }
}
