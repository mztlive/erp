//! 公司商品池原导出格式的需求列映射；展示资料不进入询价。

use erp_sales::entity::sales_inquiry_import::{INQUIRY_IMPORT_MAX_ROWS, InquiryImportIssue};
use erp_sales::entity::sales_inquiry_pool::{INVALID_POOL_HEADERS, POOL_SHEET, PoolSelectionLayout};

use super::parse::{Row, issue};
use crate::{Error, Result};

/// 解析已版本校验的选品页，只映射客户实际填写的需求。
/// # 参数
/// `rows` 保留 Excel 原始行号，`errors` 收集逐单元格错误。
/// # 返回
/// 返回固定白名单顺序的需求行，不包含原导出的内部展示列。
/// # 错误
/// 表头非法或商品数量超限时拒绝。
pub(super) fn pool_rows(rows: &[Row], errors: &mut Vec<InquiryImportIssue>) -> Result<Vec<Row>> {
    if rows.iter().filter(|row| row.number > 2).count() > INQUIRY_IMPORT_MAX_ROWS {
        return Err(Error::ValidationError("选品页每次不得超过 2000 个商品，请减少后重新导入".into()));
    }
    let Some(header) = rows.iter().find(|row| row.number == 2) else {
        return Err(Error::ValidationError("选品页第 2 行表头缺失，请使用公司商品池导出文件".into()));
    };
    if header.cells.iter().any(|cell| cell.formula) {
        return Err(Error::ValidationError(INVALID_POOL_HEADERS.into()));
    }
    let names = header.cells.iter().map(|cell| cell.value.as_str()).collect::<Vec<_>>();
    let layout = PoolSelectionLayout::new(&names)?;
    let mut products = Vec::new();
    for row in rows.iter().filter(|row| row.number > 2) {
        validate_cells(row, layout.headers(), errors);
        let cells = layout
            .mapping()
            .iter()
            .map(|index| index.and_then(|index| row.cells.get(index)).cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        if cells[4].value.is_empty() && !cells[4].formula {
            continue;
        }
        products.push(Row { number: row.number, cells });
    }
    Ok(products)
}

fn validate_cells(row: &Row, headers: &[&str], errors: &mut Vec<InquiryImportIssue>) {
    for (index, cell) in row.cells.iter().enumerate() {
        let column = headers.get(index).copied().unwrap_or("附加列");
        if index >= headers.len() && (!cell.value.is_empty() || cell.formula) {
            issue(errors, POOL_SHEET, row.number, column, "不得在表头外增加字段");
        }
        if cell.formula {
            issue(errors, POOL_SHEET, row.number, column, "公式单元格须先转换为值");
        }
        if cell.value.chars().count() > 2000 {
            issue(errors, POOL_SHEET, row.number, column, "单元格内容过长");
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_sales::entity::sales_inquiry_pool::{POOL_HEADERS as HEADERS, POOL_REQUIRED_HEADERS as REQUIRED};

    use super::*;
    use crate::sales_inquiry_import::parse::Cell;

    fn cells(values: &[&str]) -> Vec<Cell> {
        values.iter().map(|value| Cell { value: (*value).into(), text: true, ..Default::default() }).collect()
    }

    #[test]
    fn selected_quantity_controls_import_without_copying_supplier_or_display_prices() {
        let mut selected = vec![""; HEADERS.len()];
        selected[3] = "00123";
        selected[5] = "88.00";
        selected[9] = "supplier-private";
        selected[14] = "2";
        selected[18] = "PCS";
        let mut unselected = selected.clone();
        unselected[3] = "voucher";
        unselected[14] = "";
        let rows = vec![
            Row { number: 2, cells: cells(&HEADERS) },
            Row { number: 3, cells: cells(&selected) },
            Row { number: 4, cells: cells(&unselected) },
        ];
        let mut errors = vec![];
        let products = pool_rows(&rows, &mut errors).unwrap();
        assert!(errors.is_empty());
        assert_eq!(products.len(), 1);
        assert_eq!(products[0].number, 3);
        assert_eq!(products[0].cells[0].value, "00123");
        assert!(products[0].cells[0].text);
        assert_eq!(products[0].cells[3].value, "PCS");
        assert_eq!(products[0].cells[4].value, "2");
        assert!(products[0].cells[5].value.is_empty());
        assert!(!products[0].cells.iter().any(|cell| cell.value == "supplier-private"));
    }

    #[test]
    fn sales_can_remove_display_columns_and_reorder_remaining_columns() {
        let rows = vec![
            Row {
                number: 2, cells: cells(&["单位编码", "SKU 编号", "需求备注", "需求数量", "销售价"])
            },
            Row { number: 8, cells: cells(&["PCS", "0001", "客户备注", "3", "12.50"]) },
        ];
        let products = pool_rows(&rows, &mut vec![]).unwrap();
        assert_eq!(products[0].number, 8);
        assert_eq!(products[0].cells[0].value, "0001");
        assert_eq!(products[0].cells[5].value, "12.50");
        assert_eq!(products[0].cells[6].value, "客户备注");
    }

    #[test]
    fn current_and_legacy_sales_price_headers_preserve_exact_value() {
        for price_header in ["销售价", "销售参考价"] {
            let rows = vec![
                Row {
                    number: 2,
                    cells: cells(&["SKU 编号", "单位编码", "需求数量", price_header, "需求备注"]),
                },
                Row { number: 3, cells: cells(&["0001", "PCS", "3", "12.5001", ""]) },
            ];
            let mut errors = Vec::new();
            let products = pool_rows(&rows, &mut errors).unwrap();
            assert!(errors.is_empty());
            assert_eq!(products[0].cells[5].value, "12.5001");
        }
    }

    #[test]
    fn missing_duplicate_and_unknown_headers_are_rejected() {
        for headers in [
            vec!["SKU 编号"],
            [REQUIRED.as_slice(), &["供应商原始报价"]].concat(),
            [REQUIRED.as_slice(), &["需求数量"]].concat(),
            [REQUIRED.as_slice(), &["销售参考价"]].concat(),
        ] {
            assert!(pool_rows(&[Row { number: 2, cells: cells(&headers) }], &mut vec![]).is_err());
        }
    }

    #[test]
    fn formulas_are_reported_even_when_quantity_is_blank() {
        let mut formula = cells(&["001", "PCS", "", "", ""]);
        formula[2].formula = true;
        let mut errors = vec![];
        let products = pool_rows(
            &[Row { number: 2, cells: cells(&REQUIRED) }, Row { number: 9, cells: formula }],
            &mut errors,
        )
        .unwrap();
        assert_eq!(products.len(), 1);
        assert_eq!(errors[0].row_number, 9);
        assert_eq!(errors[0].sheet, POOL_SHEET);
        assert_eq!(errors[0].column, "需求数量");
    }
}
