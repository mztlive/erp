//! 数量核对列只作填写提示；导入丢弃缓存，仍按配送明细执行领域守恒校验。

use erp_sales::entity::sales_inquiry_import::InquiryImportIssue;

use super::parse::{DELIVERY_SHEET, Row, issue};

pub(super) const CHECKED_POOL_VERSION: &str = "sales-inquiry-pool-v3";
pub(super) const SCALAR_POOL_VERSION: &str = "sales-inquiry-pool-v4";
const HEADERS: [&str; 6] = ["需求数量", "已分配数量", "剩余数量", "分配状态", "核对数量", "核对商品"];

/// 校验并移除不参与业务写入的数量核对列。
/// # 参数
/// `rows` 为配送页原始行，`scalar` 表示 v4 逐行核对，`errors` 接收格式错误。
/// # 返回
/// 原地保留既有十二列配送明细。
/// # 错误
/// 非约定表头、额外字段或非系统公式登记逐行错误，阻止后续确认。
pub(super) fn strip_checks(rows: &mut [Row], scalar: bool, errors: &mut Vec<InquiryImportIssue>) {
    let headers = if scalar { &HEADERS[..] } else { &HEADERS[..4] };
    for row in rows {
        if row.cells.len() > 12 + headers.len() {
            issue(errors, DELIVERY_SHEET, row.number, "附加列", "配送页不得添加字段");
        }
        if row.number == 1 {
            if !headers.iter().enumerate().all(|(index, name)| {
                row.cells.get(index + 12).is_some_and(|cell| cell.value == *name && !cell.formula)
            }) {
                issue(errors, DELIVERY_SHEET, 1, "表头", "请保留数量核对列及原导出的隐藏核对列");
            }
        } else {
            for (index, expected) in formulas(row.number, scalar).iter().enumerate() {
                if row.cells.get(index + 12).is_some_and(|cell| {
                    cell.formula && cell.formula_text.as_deref() != Some(expected.as_str())
                }) {
                    issue(
                        errors,
                        DELIVERY_SHEET,
                        row.number,
                        HEADERS[index],
                        "数量核对公式已修改，请重新导出或先转换为值",
                    );
                }
            }
        }
        row.cells.truncate(12);
    }
}

fn formulas(row: u32, scalar: bool) -> Vec<String> {
    let mut values = check_formulas(row).to_vec();
    if scalar {
        let sku = format!("$B{row}");
        let quantities = "$Q$2:$Q$10000";
        let keys = "$R$2:$R$10000";
        let criterion = format!(
            "SUBSTITUTE(SUBSTITUTE(SUBSTITUTE(\"sku:\"&{sku},\"~\",\"~~\"),\"*\",\"~*\"),\"?\",\"~?\")"
        );
        values[1] = format!(
            "IF({sku}=\"\",\"\",IF(COUNTIFS({keys},{criterion},{quantities},\"数量格式错误\")>0,\"数量格式错误\",ROUND(SUMIF({keys},{criterion},{quantities}),6)))"
        );
        values.push(format!("IF({sku}=\"\",\"\",IF(LEN(J{row}&\"\")=0,\"\",IFERROR(IF(AND(1*J{row}>0,ROUND(1*J{row},6)=1*J{row}),1*J{row},\"数量格式错误\"),\"数量格式错误\")))"));
        values.push(format!("IF({sku}=\"\",\"\",\"sku:\"&{sku})"));
    }
    values
}

fn check_formulas(row: u32) -> [String; 4] {
    let sku = format!("$B{row}");
    let quantities = "$J$2:$J$10000";
    let same_sku = format!("$B$2:$B$10000={sku}");
    let positive = format!("IF({quantities}=\"\",0,IF(1*{quantities}>0,1*{quantities},NA()))");
    [
        format!(
            "IF({sku}=\"\",\"\",INDEX(InquirySelection[需求数量],MATCH({sku},InquirySelection[SKU 编号],0)))"
        ),
        format!(
            "IF({sku}=\"\",\"\",IFERROR(ROUND(SUMPRODUCT(IF({same_sku},{positive},0)),6),\"数量格式错误\"))"
        ),
        format!("IF({sku}=\"\",\"\",IFERROR(ROUND(M{row}-N{row},6),\"数量格式错误\"))"),
        format!(
            "IF({sku}=\"\",\"\",IFERROR(IF(1*M{row}<=0,\"需求数量错误\",IF(NOT(ISNUMBER(O{row})),\"数量格式错误\",IF(O{row}<0,\"超出 \"&TEXT(-O{row},\"General\"),IF(O{row}>0,\"尚差 \"&TEXT(O{row},\"General\"),\"数量一致\")))),\"数量格式错误\"))"
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sales_inquiry_import::parse::Cell;

    fn row(number: u32) -> Row {
        let mut cells = vec![Cell::default(); 12];
        cells.extend(check_formulas(number).map(|formula| Cell {
            formula: true,
            formula_text: Some(formula),
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
        strip_checks(&mut rows, false, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(rows[0].cells.len(), 12);
        assert_eq!(rows[0].cells[9].value, "60");
        assert_eq!(rows[1].cells[9].value, "40");
    }

    #[test]
    fn changed_formula_and_extra_columns_fail_closed() {
        let mut rows = vec![row(2)];
        rows[0].cells[13].formula_text = Some("1+1".into());
        rows[0].cells.push(Cell::default());
        let mut errors = vec![];
        strip_checks(&mut rows, false, &mut errors);
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|error| error.column == "已分配数量"));
        assert!(errors.iter().any(|error| error.column == "附加列"));
    }

    #[test]
    fn scalar_helpers_are_checked_and_discarded_without_hiding_changed_formulas() {
        let mut cells = vec![Cell::default(); 12];
        cells.extend(formulas(3, true).into_iter().map(|formula| Cell {
            formula: true,
            formula_text: Some(formula),
            value: "cached".into(),
            ..Default::default()
        }));
        let mut rows = vec![Row { number: 3, cells }];
        let mut valid = rows.clone();
        let mut errors = vec![];
        strip_checks(&mut valid, true, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(valid[0].cells.len(), 12);
        rows[0].cells[16].formula_text = Some("1+1".into());
        strip_checks(&mut rows, true, &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].column, "核对数量");
    }

    #[test]
    fn missing_check_headers_are_rejected() {
        let mut rows = vec![Row { number: 1, cells: vec![Cell::default(); 12] }];
        let mut errors = vec![];
        strip_checks(&mut rows, false, &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].column, "表头");
    }
}
