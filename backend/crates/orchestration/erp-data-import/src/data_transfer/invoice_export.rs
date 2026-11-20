use application_core::AuditActor;
use chrono::{DateTime, FixedOffset};
use futures_util::future::try_join_all;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::task::spawn_blocking;

use super::workbook::{XLSX_MIME, xlsx};
use super::{BusinessTransfers, TransferKind, TransferResult};
use crate::{Error, Result};

const MAX_REQUESTS: u64 = 10_000;
const MAX_TASKS: u64 = 2_000;
const MAX_DETAILS: usize = 100_000;
const TASK_SHEET: &str = "开票任务";
const DETAIL_SHEET: &str = "开票申请明细";
const NO_DETAILS: &str = "无可导出的申请明细";

pub(super) struct InvoiceExportSource {
    rows: Vec<Value>,
    scope: Value,
    linked: Option<Vec<Option<Value>>>,
}

impl BusinessTransfers {
    pub(super) async fn invoice_workbook(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<TransferResult> {
        let source = self.invoice_export_source(kind, input, actor).await?;
        let authorization = source.authorization()?;
        let row_count = source.rows.len();
        let detail_row_count = source.detail_row_count()?;
        let name = if source.linked.is_some() { "工作台开票任务.xlsx" } else { "开票申请.xlsx" };
        let bytes = spawn_blocking(move || source.render())
            .await
            .map_err(|_| Error::Internal("Excel 生成中断".into()))??;
        let mut result = TransferResult::file(name.into(), XLSX_MIME, bytes, authorization)?;
        result.metadata = json!({"row_count": row_count, "detail_row_count": detail_row_count});
        Ok(result)
    }

    pub(super) async fn invoice_export_source(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<InvoiceExportSource> {
        let tasks = kind == TransferKind::WorkbenchInvoiceExecutionExcel;
        let (rows, scope) =
            self.collect_export(kind, input, actor, if tasks { MAX_TASKS } else { MAX_REQUESTS }).await?;
        let linked = if tasks {
            let mut requests = Vec::with_capacity(rows.len());
            for batch in rows.chunks(8) {
                let reads = batch.iter().map(|row| self.linked_invoice_for_task(row, actor));
                requests.extend(try_join_all(reads).await?);
            }
            let mut query = input.clone();
            query["page"] = json!(1);
            query["page_size"] = json!(100);
            for key in ["scope_version", "queue_context_id"] {
                query[key] = scope[key].clone();
            }
            self.check_final_scope(kind, &query, actor, Some(rows.len() as u64)).await?;
            Some(requests)
        } else {
            None
        };
        Ok(InvoiceExportSource { rows, scope, linked })
    }

    async fn linked_invoice_for_task(&self, row: &Value, actor: &AuditActor) -> Result<Option<Value>> {
        let id = invoice_task_id(row)?;
        let request =
            self.reader.linked_invoice_request(id, string(&row["receivable_account_id"]), actor).await?;
        ensure_link(id, request.as_ref())?;
        Ok(request)
    }
}

impl InvoiceExportSource {
    pub(super) fn authorization(&self) -> Result<Value> {
        let mut hash = Sha256::new();
        hash_json(&mut hash, &self.scope)?;
        for (index, row) in self.rows.iter().enumerate() {
            hash_json(&mut hash, row)?;
            if let Some(linked) = &self.linked {
                hash_json(&mut hash, &linked[index])?;
            }
        }
        Ok(json!({"invoice_export_v1": hex::encode(hash.finalize())}))
    }

    fn detail_row_count(&self) -> Result<usize> {
        let mut count = 0;
        for request in self.requests() {
            count += lines(request).len().max(1);
            if count > MAX_DETAILS {
                return Err(Error::ValidationError("开票申请明细超过十万行，请缩小导出范围".into()));
            }
        }
        Ok(count)
    }

    fn requests(&self) -> impl Iterator<Item = &Value> {
        let finance = self.rows.iter().filter(|_| self.linked.is_none());
        let workbench = self.linked.iter().flatten().flatten().map(|request| &request["data"]);
        finance.chain(workbench)
    }

    fn render(&self) -> Result<Vec<u8>> {
        let mut book = Workbook::new();
        if let Some(linked) = &self.linked {
            let sheet = worksheet(&mut book, TASK_SHEET, TASK_COLUMNS)?;
            for (index, (task, request)) in self.rows.iter().zip(linked).enumerate() {
                write_task(sheet, index as u32 + 1, task, request.as_ref().map(|row| &row["data"]))?;
            }
        }
        let sheet = worksheet(&mut book, DETAIL_SHEET, REQUEST_COLUMNS)?;
        let mut index = 1u32;
        for request in self.requests() {
            let rows = lines(request);
            if rows.is_empty() {
                write_request(sheet, index, request, None)?;
                index += 1;
            } else {
                for (line, data) in rows.iter().enumerate() {
                    write_request(sheet, index, request, Some((line + 1, data)))?;
                    index += 1;
                }
            }
        }
        book.save_to_buffer().map_err(xlsx)
    }
}

fn invoice_task_id(row: &Value) -> Result<&str> {
    if row["work_item_type"] != "SALES_INVOICE_EXECUTION" {
        return Err(Error::Internal("导出包含不支持的任务类型".into()));
    }
    row["id"].as_str().filter(|id| !id.is_empty()).ok_or_else(|| Error::Internal("导出任务缺少编号".into()))
}

fn ensure_link(id: &str, request: Option<&Value>) -> Result<()> {
    if request.is_some_and(|request| {
        request["data"]["work_item_id"].as_str() != Some(id) || !request["scope_version"].is_string()
    }) {
        return Err(Error::ConflictError("开票申请关联已变化，请重新导出".into()));
    }
    Ok(())
}

fn hash_json(hash: &mut Sha256, value: &impl serde::Serialize) -> Result<()> {
    hash.update(serde_json::to_vec(value).map_err(|_| Error::Internal("导出摘要生成失败".into()))?);
    hash.update(b"\n");
    Ok(())
}

fn lines(request: &Value) -> &[Value] {
    request["data"]["lines"].as_array().map_or(&[], Vec::as_slice)
}

fn worksheet<'a>(book: &'a mut Workbook, name: &str, columns: &[(&str, u16)]) -> Result<&'a mut Worksheet> {
    let sheet = book.add_worksheet();
    sheet.set_name(name).map_err(xlsx)?;
    sheet.set_freeze_panes(1, 0).map_err(xlsx)?;
    let header = Format::new().set_bold().set_background_color("DCE8FC");
    for (column, (title, width)) in columns.iter().enumerate() {
        sheet.set_column_width(column as u16, *width).map_err(xlsx)?;
        sheet.write_string_with_format(0, column as u16, *title, &header).map_err(xlsx)?;
    }
    Ok(sheet)
}

fn write_task(sheet: &mut Worksheet, index: u32, task: &Value, request: Option<&Value>) -> Result<()> {
    let timestamps =
        ["created_at", "due_at", "completed_at", "closed_at"].map(|field| timestamp(&task[field]));
    let fields = [
        string(&task["id"]),
        string(&task["type_label"]),
        string(&task["status_label"]),
        string(&task["priority_label"]),
        string(&task["owner_user"]["display_name"]),
        string(&task["owner_organization"]["display_name"]),
        &timestamps[0],
        &timestamps[1],
        &timestamps[2],
        &timestamps[3],
        request.map_or(NO_DETAILS, |_| "见开票申请明细"),
    ];
    for (column, text) in fields.into_iter().enumerate() {
        write_text(sheet, index, column as u16, text)?;
    }
    if let Some(request) = request {
        write_text(sheet, index, 11, string(&request["request_no"]))?;
        write_text(sheet, index, 12, string(&request["sales_order_no"]))?;
        write_decimal(sheet, index, 13, &request["remaining_amount"], "0.00")?;
    }
    Ok(())
}

fn write_request(
    sheet: &mut Worksheet,
    index: u32,
    request: &Value,
    line: Option<(usize, &Value)>,
) -> Result<()> {
    let data = &request["data"];
    let content = line.map_or(&data["invoice_content"], |(_, line)| &line["content"]);
    let fields = [
        &request["work_item_id"],
        &request["request_no"],
        &request["sales_order_no"],
        &data["seller_name"],
        &data["seller_tax_number"],
        &data["invoice_title"],
        &data["tax_number"],
        content,
    ];
    for (column, value) in fields.into_iter().enumerate() {
        write_text(sheet, index, column as u16, string(value))?;
    }
    if let Some((line_no, line)) = line {
        sheet.write_number(index, 8, line_no as f64).map_err(xlsx)?;
        write_decimal(sheet, index, 9, &line["quantity"], "0.######")?;
        write_decimal(sheet, index, 10, &line["unit_price"], "0.0000")?;
        write_decimal(sheet, index, 11, &line["line_amount"], "0.00")?;
    }
    if line.is_none_or(|(line_no, _)| line_no == 1) {
        write_decimal(sheet, index, 12, &request["amount"], "0.00")?;
        write_decimal(sheet, index, 13, &request["invoiced_amount"], "0.00")?;
    }
    for (column, text) in [
        string(&request["status_label"]),
        string(&request["applicant_user_id"]),
        string(&request["handler_user_id"]),
        &timestamp(&request["created_at"]),
        string(&data["reason"]),
    ]
    .into_iter()
    .enumerate()
    {
        write_text(sheet, index, 14 + column as u16, text)?;
    }
    Ok(())
}

fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn timestamp(value: &Value) -> String {
    value.as_i64().and_then(|seconds| DateTime::from_timestamp(seconds, 0)).map_or_else(String::new, |at| {
        let beijing = FixedOffset::east_opt(8 * 3600).expect("UTC+8");
        at.with_timezone(&beijing).format("%Y-%m-%d %H:%M:%S").to_string()
    })
}

fn write_text(sheet: &mut Worksheet, row: u32, col: u16, value: &str) -> Result<()> {
    if !value.is_empty() {
        sheet.write_string(row, col, value).map_err(xlsx)?;
    }
    Ok(())
}

fn write_decimal(sheet: &mut Worksheet, row: u32, col: u16, value: &Value, format: &str) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    let text = string(value);
    let decimal = text.parse::<Decimal>().map_err(|_| Error::Internal("开票申请金额格式无效".into()))?;
    // Excel 只保留 15 位有效数字；超限时保留完整的十进制文本。
    if decimal.normalize().mantissa().unsigned_abs() < 1_000_000_000_000_000
        && let Some(number) = decimal.to_f64()
    {
        sheet
            .write_number_with_format(row, col, number, &Format::new().set_num_format(format))
            .map_err(xlsx)?;
    } else {
        write_text(sheet, row, col, text)?;
    }
    Ok(())
}

const TASK_COLUMNS: &[(&str, u16)] = &[
    ("工作项 ID", 30),
    ("任务类型", 16),
    ("任务状态", 14),
    ("优先级", 12),
    ("处理人", 22),
    ("责任组织", 22),
    ("创建时间（北京时间）", 24),
    ("截止时间（北京时间）", 24),
    ("完成时间（北京时间）", 24),
    ("关闭时间（北京时间）", 24),
    ("开票申请明细", 24),
    ("关联申请单号", 22),
    ("来源销售单号", 22),
    ("申请待开票金额", 18),
];

const REQUEST_COLUMNS: &[(&str, u16)] = &[
    ("工作项 ID", 30),
    ("申请单号", 22),
    ("销售单号", 22),
    ("销方名称", 30),
    ("销方纳税人识别号", 24),
    ("购方名称（发票抬头）", 30),
    ("购方纳税人识别号", 24),
    ("开票内容", 38),
    ("明细序号", 12),
    ("数量", 18),
    ("含税单价", 18),
    ("明细含税金额", 18),
    ("申请金额", 18),
    ("已开票金额", 18),
    ("申请状态", 16),
    ("申请人 ID", 26),
    ("开票处理人 ID", 26),
    ("申请时间（北京时间）", 24),
    ("申请事由", 38),
];

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use zip::ZipArchive;

    use super::*;

    fn workbook_file(bytes: &[u8], path: &str) -> String {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut text = String::new();
        zip.by_name(path).unwrap().read_to_string(&mut text).unwrap();
        text
    }

    fn request() -> Value {
        json!({"id":"invoice", "work_item_id":"task-1", "request_no":"KP-001",
        "sales_order_no":"SO-1", "amount":"50.00", "invoiced_amount":"0.00",
        "data":{"invoice_title":"=DO_NOT_RUN()", "tax_number":"001234567890123456",
            "seller_name":"销方甲", "seller_tax_number":"0099", "invoice_content":"不重复旧内容",
            "lines":[
                {"content":"项目甲","quantity":"2","unit_price":"12.40","line_amount":"24.80"},
                {"content":"项目乙","quantity":"1","unit_price":"25.20","line_amount":"25.20"}
            ]}})
    }

    fn workbench() -> InvoiceExportSource {
        InvoiceExportSource {
            rows: vec![task("task-1"), task("task-without-details")],
            scope: json!({"scope_version":"queue-version", "queue_context_id":"queue-context"}),
            linked: Some(vec![Some(json!({"data":request(),"scope_version":"request-version"})), None]),
        }
    }

    fn task(id: &str) -> Value {
        json!({"id":id, "work_item_type":"SALES_INVOICE_EXECUTION", "status":"OPEN",
            "business_object_id":"do-not-export-without-request-read", "created_at":0})
    }

    #[test]
    fn workbench_keeps_every_task_and_only_emits_visible_request_lines() {
        let source = workbench();
        assert_eq!(source.detail_row_count().unwrap(), 2);
        let bytes = source.render().unwrap();
        let workbook = workbook_file(&bytes, "xl/workbook.xml");
        assert!(workbook.contains(TASK_SHEET) && workbook.contains(DETAIL_SHEET));
        let tasks = workbook_file(&bytes, "xl/worksheets/sheet1.xml");
        let details = workbook_file(&bytes, "xl/worksheets/sheet2.xml");
        assert_eq!(row_count(&tasks), 3);
        assert_eq!(row_count(&details), 3);
        assert!(details.contains(r#"r="L2" s=""#) && details.contains(">24.8<"));
        assert!(!details.contains(r#"r="M3""#));
        for sheet in [&tasks, &details] {
            assert!(!sheet.contains("<f>") && !sheet.contains("<f "));
        }
        let strings = workbook_file(&bytes, "xl/sharedStrings.xml");
        for expected in ["task-without-details", NO_DETAILS, "=DO_NOT_RUN()", "001234567890123456"] {
            assert!(strings.contains(expected));
        }
        assert!(!strings.contains("不重复旧内容"));
        assert!(!strings.contains("do-not-export-without-request-read"));
    }

    #[test]
    fn finance_uses_shared_detail_columns_and_does_not_invent_legacy_values() {
        let source = InvoiceExportSource {
            rows: vec![json!({"id":"legacy", "amount":"1234567890123456.00",
                "data":{"invoice_title":"历史购方", "tax_number":"0011", "invoice_content":"历史内容"}})],
            scope: json!({"scope_version":"finance-scope"}),
            linked: None,
        };
        assert_eq!(source.detail_row_count().unwrap(), 1);
        let bytes = source.render().unwrap();
        let workbook = workbook_file(&bytes, "xl/workbook.xml");
        assert!(workbook.contains(DETAIL_SHEET) && !workbook.contains(TASK_SHEET));
        let sheet = workbook_file(&bytes, "xl/worksheets/sheet1.xml");
        assert_eq!(row_count(&sheet), 2);
        for blank in ["D2", "E2", "I2", "J2", "K2", "L2"] {
            assert!(!sheet.contains(&format!(r#"r="{blank}""#)));
        }
        let strings = workbook_file(&bytes, "xl/sharedStrings.xml");
        assert!(strings.contains("历史内容") && strings.contains("1234567890123456.00"));
    }

    fn row_count(sheet: &str) -> usize {
        sheet.matches("<row ").count()
    }

    #[test]
    fn linked_request_must_match_exact_work_item_and_result_detects_revocation() {
        let mut source = workbench();
        let baseline = source.authorization().unwrap();
        let link = &mut source.linked.as_mut().unwrap()[0];
        assert!(ensure_link("task-1", link.as_ref()).is_ok());
        assert!(ensure_link("another-task", link.as_ref()).is_err());
        assert!(ensure_link("task-1", None).is_ok());
        assert!(invoice_task_id(&json!({"id":"task", "work_item_type":"DOCUMENT_APPROVAL"})).is_err());
        link.as_mut().unwrap()["scope_version"] = json!("changed-authorization");
        assert_ne!(baseline, source.authorization().unwrap());
        source.linked.as_mut().unwrap()[0] = None;
        assert_ne!(baseline, source.authorization().unwrap());
        assert_eq!(source.rows.len(), 2);
    }
}
