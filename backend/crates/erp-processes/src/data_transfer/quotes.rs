use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{Cursor, Write};

use application_core::AuditActor;
use chrono::{DateTime, FixedOffset};
use erp_core::common::time::BUSINESS_TZ_OFFSET_SECS;
use rust_decimal::Decimal;
use rust_xlsxwriter::{Format, Image, Workbook, Worksheet};
use serde_json::{Value, json};
use tokio::task::spawn_blocking;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::imports::{required, version};
use super::workbook::{text, write_row, xlsx};
use super::{BusinessTransfers, TransferKind, TransferResult};
use crate::sales_inquiry::read::InquiryReadService;
use crate::{Error, Result};

pub(super) const HEADERS: [&str; 15] = [
    "主图",
    "商品名称",
    "规格",
    "SKU 编号",
    "供应商货号",
    "单位",
    "需求数量",
    "要求交付日",
    "交付要求",
    "配送要求",
    "含税单价（元）",
    "税率（%）",
    "可供数量",
    "运费（元）",
    "报价备注",
];
impl BusinessTransfers {
    pub(super) async fn quote_context(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let detail = InquiryReadService::new(self.db.clone(), self.rbac.clone())
            .procurement_detail(actor, required(input, "inquiry_id")?, required(input, "task_id")?)
            .await?;
        let detail = serde_json::to_value(detail).map_err(json_error)?;
        let task = detail["work_items"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["id"] == input["task_id"]))
            .ok_or_else(|| Error::Forbidden("当前采购任务已失效".into()))?;
        if detail["version"].as_u64() != Some(version(input)?)
            || task["version"] != input["task_version"]
            || !["awaiting_procurement", "procurement_processing"]
                .contains(&detail["status"].as_str().unwrap_or_default())
        {
            return Err(Error::ConflictError("询价或采购任务已变化，请刷新后重试".into()));
        }
        Ok(detail)
    }
    pub(super) async fn quote_source(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let detail = self.quote_context(input, actor).await?;
        let task = detail["work_items"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["id"] == input["task_id"]))
            .ok_or_else(|| Error::Forbidden("采购任务失效".into()))?;
        let ids = task["line_ids"].as_array().ok_or_else(|| Error::Internal("任务商品缺失".into()))?;
        let lines: Vec<_> = detail["draft"]["lines"]
            .as_array()
            .ok_or_else(|| Error::Internal("询价商品缺失".into()))?
            .iter()
            .filter(|l| ids.contains(&l["line_id"]))
            .cloned()
            .collect();
        if lines.is_empty() || lines.len() > 200 {
            return Err(Error::ValidationError("每次生成 1 至 200 条商品".into()));
        }
        let mut products = Vec::new();
        let mut offerings = Vec::new();
        let mut seen = HashSet::new();
        for line in &lines {
            if !seen.insert(line["sku_id"].clone().to_string()) {
                continue;
            }
            let product = self
                .reader
                .page(
                    TransferKind::SupplierQuoteArchive,
                    &json!({"source":"products","sku_ids":line["sku_id"],"page":1,"page_size":100}),
                    actor,
                )
                .await?;
            products.extend(product["items"].as_array().cloned().unwrap_or_default());
            offerings.extend(
                self.collect(TransferKind::SupplierQuoteArchive, &json!({"sku_id":line["sku_id"]}), actor)
                    .await?,
            );
        }
        Ok(json!({"inquiry":detail,"lines":lines,"products":products,"offerings":offerings}))
    }
    pub(super) async fn quote_archive(&self, input: &Value, actor: &AuditActor) -> Result<TransferResult> {
        let source = self.quote_source(input, actor).await?;
        let products = source["products"].as_array().ok_or_else(|| Error::Internal("商品资料缺失".into()))?;
        let images = self.images(products).await?;
        let authorization = self.workbook_evidence("source", &source, products).await?;
        let input = input.clone();
        let bytes = spawn_blocking(move || archive(&source, &input, &images))
            .await
            .map_err(|_| Error::Internal("报价表生成中断".into()))??;
        TransferResult::file("供应商报价表.zip".into(), "application/zip", bytes, authorization)
    }
}
fn archive(source: &Value, input: &Value, images: &HashMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    let selected_lines =
        input["line_ids"].as_array().ok_or_else(|| Error::ValidationError("请选择商品".into()))?;
    let selected_suppliers =
        input["supplier_ids"].as_array().ok_or_else(|| Error::ValidationError("请选择供应商".into()))?;
    if selected_suppliers.is_empty() || selected_suppliers.len() > 200 {
        return Err(Error::ValidationError("每次选择 1 至 200 家供应商".into()));
    }
    let groups = groups(source, selected_lines, selected_suppliers)?;
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut size = 0;
    for (index, (supplier, (name, rows))) in groups.into_iter().enumerate() {
        let bytes = quote_book(&source["inquiry"], &supplier, &name, &rows, images)?;
        size += bytes.len();
        if size > 50 * 1024 * 1024 {
            return Err(Error::ValidationError("报价表文件包超过 50 MiB，请缩小范围".into()));
        }
        let name: String = name
            .chars()
            .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c })
            .take(60)
            .collect();
        zip.start_file(format!("{}-{}报价表.xlsx", index + 1, name), SimpleFileOptions::default())
            .map_err(|_| Error::Internal("文件包生成失败".into()))?;
        zip.write_all(&bytes).map_err(|_| Error::Internal("文件包写入失败".into()))?;
    }
    Ok(zip.finish().map_err(|_| Error::Internal("文件包生成失败".into()))?.into_inner())
}
pub(super) fn date(value: &Value) -> String {
    value
        .as_i64()
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .and_then(|t| {
            FixedOffset::east_opt(BUSINESS_TZ_OFFSET_SECS)
                .map(|offset| t.with_timezone(&offset).format("%Y-%m-%d").to_string())
        })
        .unwrap_or_default()
}
fn quote_book(
    inquiry: &Value,
    supplier: &str,
    name: &str,
    rows: &[Value],
    images: &HashMap<String, Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name("报价表").map_err(xlsx)?;
    sheet
        .set_freeze_panes(4, 2)
        .map_err(xlsx)?
        .set_column_range_width(0, 15, 22)
        .map_err(xlsx)?
        .set_column_hidden(15)
        .map_err(xlsx)?;
    for (i, notice) in [
        format!("{name}报价表"),
        format!("询价：{}；请按原需求填写", text(inquiry, "inquiry_no")),
        "填写含税单价、税率、可供数量、运费（已含填 0）及备注；不能供货填数量 0 并说明原因。".into(),
    ]
    .iter()
    .enumerate()
    {
        sheet.merge_range(i as u32, 0, i as u32, 14, notice, &Format::new().set_text_wrap()).map_err(xlsx)?;
    }
    write_row(sheet, 3, &HEADERS.map(str::to_owned)).map_err(xlsx)?;
    quote_rows(sheet, rows, images)?;
    let meta = book.add_worksheet();
    meta.set_name("回导信息").map_err(xlsx)?.set_very_hidden(true);
    for (i, value) in [
        "supplier-inquiry-quote-v4".into(),
        text(inquiry, "id"),
        inquiry["requirement_version"].to_string(),
        (inquiry["feedback_round"].as_u64().unwrap_or_default() + 1).to_string(),
        supplier.into(),
        name.into(),
    ]
    .iter()
    .enumerate()
    {
        meta.write_string(i as u32, 0, value).map_err(xlsx)?;
    }
    delivery(&mut book, rows)?;
    book.save_to_buffer().map_err(xlsx)
}
fn delivery(book: &mut Workbook, rows: &[Value]) -> Result<()> {
    let sheet = book.add_worksheet();
    sheet.set_name("配送需求").map_err(xlsx)?;
    write_row(
        sheet,
        0,
        &[
            "询价项次",
            "SKU 编号",
            "商品名称",
            "规格",
            "配送地区",
            "地址数量",
            "配送数量",
            "单位",
            "要求交付日",
            "交付要求",
        ]
        .map(str::to_owned),
    )
    .map_err(xlsx)?;
    let mut row_index = 1;
    for (i, row) in rows.iter().enumerate() {
        let l = &row["line"];
        let regions = regions(l)?;
        for (_, (region, count, quantity)) in regions {
            write_row(
                sheet,
                row_index,
                &[
                    (i + 1).to_string(),
                    text(l, "sku_code"),
                    text(l, "product_name"),
                    text(l, "specification"),
                    region,
                    count.to_string(),
                    quantity.normalize().to_string(),
                    text(&row["product"], "base_unit_name"),
                    date(&l["delivery_due_at"]),
                    text(l, "delivery_terms"),
                ],
            )
            .map_err(xlsx)?;
            row_index += 1;
        }
    }
    Ok(())
}

fn json_error(_: serde_json::Error) -> Error {
    Error::Internal("任务结果无法编码".into())
}

fn quote_rows(sheet: &mut Worksheet, rows: &[Value], images: &HashMap<String, Vec<u8>>) -> Result<()> {
    for (i, row) in rows.iter().enumerate() {
        let l = &row["line"];
        let p = &row["product"];
        let mut values = vec![
            "暂无图片".into(),
            text(l, "product_name"),
            text(l, "specification"),
            text(l, "sku_code"),
            text(row, "codes"),
            text(p, "base_unit_name"),
            text(l, "quantity"),
            date(&l["delivery_due_at"]),
            text(l, "delivery_terms"),
            format!("第 {} 项，详见配送需求", i + 1),
        ];
        values.extend(std::iter::repeat_n(String::new(), HEADERS.len() - 10));
        values.push(json!({"lineId":l["line_id"],"skuId":l["sku_id"]}).to_string());
        let n = u32::try_from(i + 4).map_err(|_| Error::ValidationError("商品数量超限".into()))?;
        write_row(sheet, n, &values).map_err(xlsx)?;
        sheet.set_row_height(n, 66).map_err(xlsx)?;
        if let Some(bytes) = p["main_image_asset_id"].as_str().and_then(|id| images.get(id)) {
            sheet
                .write_string(n, 0, "")
                .map_err(xlsx)?
                .insert_image(
                    n,
                    0,
                    &Image::new_from_buffer(bytes).map_err(xlsx)?.set_scale_to_size(76, 76, false),
                )
                .map_err(xlsx)?;
        }
    }
    Ok(())
}

fn regions(l: &Value) -> Result<BTreeMap<String, (String, u32, Decimal)>> {
    let mut regions: BTreeMap<String, (String, u32, Decimal)> = BTreeMap::new();
    for allocation in l["allocations"].as_array().into_iter().flatten() {
        let name = text(allocation, "region_name");
        let key = format!("{}:{name}", text(allocation, "region_code"));
        let group = regions.entry(key).or_insert((name, 0, Decimal::ZERO));
        group.1 += 1;
        let quantity: Decimal = required(allocation, "quantity")?
            .parse()
            .map_err(|_| Error::ValidationError("配送数量无效".into()))?;
        group.2 =
            group.2.checked_add(quantity).ok_or_else(|| Error::ValidationError("配送数量溢出".into()))?;
    }
    Ok(regions)
}

fn groups(
    source: &Value,
    selected_lines: &[Value],
    selected_suppliers: &[Value],
) -> Result<BTreeMap<String, (String, Vec<Value>)>> {
    let mut groups: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
    for line in
        source["lines"].as_array().into_iter().flatten().filter(|l| selected_lines.contains(&l["line_id"]))
    {
        let product = source["products"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["sku_id"] == line["sku_id"])
            .ok_or_else(|| Error::ConflictError("商品已不可售，请重新查询".into()))?;
        for offering in source["offerings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["sku_id"] == line["sku_id"] && selected_suppliers.contains(&o["supplier_id"]))
        {
            let id = required(offering, "supplier_id")?;
            let name = required(offering, "supplier_name")?;
            let group = groups.entry(id.into()).or_insert_with(|| (name.into(), Vec::new()));
            if group.1.iter().any(|r| r["line"]["line_id"] == line["line_id"]) {
                continue;
            }
            let codes = source["offerings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|o| o["supplier_id"] == offering["supplier_id"] && o["sku_id"] == line["sku_id"])
                .filter_map(|o| o["supplier_sku_code"].as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join("、");
            if product["base_unit_code"] != line["unit_code"] {
                return Err(Error::ConflictError("商品单位已变化".into()));
            }
            group.1.push(json!({"line":line,"product":product,"codes":codes}));
        }
    }
    check_coverage(&groups, selected_lines, selected_suppliers)?;
    Ok(groups)
}

fn check_coverage(
    groups: &BTreeMap<String, (String, Vec<Value>)>,
    selected_lines: &[Value],
    selected_suppliers: &[Value],
) -> Result<()> {
    let covered: HashSet<_> = groups
        .values()
        .flat_map(|(_, rows)| rows.iter().map(|r| r["line"]["line_id"].clone().to_string()))
        .collect();
    if selected_lines.is_empty()
        || covered.len() != selected_lines.len()
        || selected_lines.iter().any(|id| !covered.contains(&id.to_string()))
    {
        return Err(Error::ConflictError("所选商品供给已变化，请重新选择".into()));
    }
    if groups.len() != selected_suppliers.len() {
        return Err(Error::ConflictError("所选供应商供给已变化，请重新选择".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use zip::ZipArchive;

    use super::super::quote_import::parse_files;
    use super::*;
    #[test]
    fn export_dates_use_business_timezone() {
        assert_eq!(date(&json!(1792598400)), "2026-10-22");
    }
    #[test]
    fn quote_archive_roundtrips_and_never_contains_recipient_details() {
        let source = json!({"inquiry":{"id":"inquiry","inquiry_no":"IQ1","version":3,"requirement_version":2,"feedback_round":0,"customer_name":"SECRET_CUSTOMER"},"lines":[{"line_id":"L1","sku_id":"SKU1","sku_code":"001","product_name":"茶礼","specification":"盒装","quantity":"2","quantity_scale":0,"unit_code":"BOX","delivery_due_at":1792627200,"delivery_terms":"包邮","allocations":[{"region_code":"110101","region_name":"北京东城","quantity":"2","recipient":"SECRET_RECIPIENT","phone":"SECRET_PHONE","address":"SECRET_ADDRESS"}]}],"products":[{"sku_id":"SKU1","base_unit_code":"BOX","base_unit_name":"盒"}],"offerings":[{"id":"O1","sku_id":"SKU1","supplier_id":"S1","supplier_name":"供货商","supplier_sku_code":"SUP1","status":"ACTIVE"}]});
        let bytes =
            archive(&source, &json!({"line_ids":["L1"],"supplier_ids":["S1"]}), &HashMap::new()).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 1);
        let mut xlsx = Vec::new();
        zip.by_index(0).unwrap().read_to_end(&mut xlsx).unwrap();
        let result = parse_files(
            &[json!({"file_name":"报价.xlsx","content_base64":STANDARD.encode(&xlsx)})],
            &source,
            &json!([]),
        )
        .unwrap();
        assert_eq!(result["issues"], json!([]));
        assert_eq!(result["skipped"], 1);
        let mut workbook = ZipArchive::new(Cursor::new(xlsx)).unwrap();
        for i in 0..workbook.len() {
            let mut text = String::new();
            let mut file = workbook.by_index(i).unwrap();
            if file.name().ends_with(".xml") {
                file.read_to_string(&mut text).unwrap();
                assert!(!text.contains("SECRET_"));
            }
        }
    }
}
