//! 数量核对列只作填写提示；导入丢弃缓存，仍按配送明细执行领域守恒校验。

use erp_sales::entity::sales_inquiry_import::InquiryImportIssue;
use erp_sales::entity::sales_inquiry_pool::{POOL_DELIVERY_HEADERS, PoolTemplateVersion};

use super::parse::{DELIVERY_SHEET, Row, issue};

/// 校验并移除不参与业务写入的数量核对列。
/// # 参数
/// `rows` 为配送页原始行，`version` 决定已发布辅助列，`errors` 接收格式错误。
/// # 返回
/// 原地保留既有十二列配送明细。
/// # 错误
/// 非约定表头或额外字段登记逐行错误，阻止后续确认；辅助公式不参与校验。
pub(super) fn strip_checks(
    rows: &mut [Row],
    version: PoolTemplateVersion,
    errors: &mut Vec<InquiryImportIssue>,
) {
    let headers = version.check_headers();
    if headers.is_empty() {
        return;
    }
    let business_columns = POOL_DELIVERY_HEADERS.len();
    for row in rows {
        if row.cells.len() > business_columns + headers.len() {
            issue(errors, DELIVERY_SHEET, row.number, "附加列", "配送页不得添加字段");
        }
        if row.number == 1
            && !headers.iter().enumerate().all(|(index, name)| {
                row.cells
                    .get(index + business_columns)
                    .is_some_and(|cell| cell.value == *name && !cell.formula)
            })
        {
            issue(errors, DELIVERY_SHEET, 1, "表头", "请保留数量核对列及原导出的隐藏核对列");
        }
        // 辅助列不进入业务数据；共享公式、软件改写和缓存均直接丢弃，不执行表达式。
        row.cells.truncate(business_columns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sales_inquiry_import::parse::Cell;

    fn row(number: u32) -> Row {
        let mut cells = vec![Cell::default(); 12];
        cells.extend((0..4).map(|_| Cell {
            formula: true,
            formula_text: Some("IFERROR(1,0)".into()),
            value: "untrusted cache".into(),
            ..Default::default()
        }));
        Row { number, cells }
    }

    #[test]
    fn checks_do_not_supply_business_values_or_retain_stale_caches() {
        let mut rows = vec![row(2), row(99)];
        rows[0].cells[9].value = "60".into();
        rows[1].cells[9].value = "40".into();
        let mut errors = vec![];
        strip_checks(&mut rows, PoolTemplateVersion::V3, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(rows[0].cells.len(), 12);
        assert_eq!(rows[0].cells[9].value, "60");
        assert_eq!(rows[1].cells[9].value, "40");
    }

    #[test]
    fn changed_advisory_formula_is_ignored_but_extra_columns_are_rejected() {
        let mut rows = vec![row(2)];
        rows[0].cells[13].formula_text = Some("1+1".into());
        rows[0].cells.push(Cell::default());
        let mut errors = vec![];
        strip_checks(&mut rows, PoolTemplateVersion::V3, &mut errors);
        assert_eq!(errors.len(), 1);
        assert!(errors.iter().any(|error| error.column == "附加列"));
    }

    #[test]
    fn scalar_helpers_discard_shared_formulas_rewritten_formulas_and_cached_values() {
        let mut rows = vec![row(3)];
        rows[0].cells.extend([Cell::default(), Cell::default()]);
        for cell in rows[0].cells.iter_mut().skip(12) {
            cell.formula = true;
            cell.formula_text = None;
            cell.value = "数量一致".into();
        }
        rows[0].cells[16].formula_text = Some("1+1".into());
        let mut errors = vec![];
        strip_checks(&mut rows, PoolTemplateVersion::V4, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(rows[0].cells.len(), 12);
        assert!(rows[0].cells.iter().all(|cell| !cell.formula && cell.value.is_empty()));
    }

    #[test]
    fn missing_check_headers_are_rejected() {
        let mut rows = vec![Row { number: 1, cells: vec![Cell::default(); 12] }];
        let mut errors = vec![];
        strip_checks(&mut rows, PoolTemplateVersion::V3, &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].column, "表头");
    }
}
