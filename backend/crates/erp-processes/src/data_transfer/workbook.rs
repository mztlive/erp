use std::collections::HashMap;
use std::io::Cursor;
use std::result::Result as StdResult;
use std::time::Duration;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_support::FileAssetExt;
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use persistence_core::NoTransaction;
use rust_xlsxwriter::{
    ConditionalFormatText, ConditionalFormatTextRule, DocProperties, Format, Formula, Image, Table,
    TableColumn, Workbook, Worksheet, XlsxError,
};
use serde_json::{Value, json};
use tokio::task::spawn_blocking;
use tokio::time::timeout;
use zune_jpeg::JpegDecoder;

use super::business::BusinessTransfers;
use super::{TransferKind, TransferResult};
use crate::{Error, Result};

const HEADERS: [&str; 20] = [
    "主图",
    "商品名称",
    "规格",
    "SKU 编号",
    "SPU 编号",
    "一件代发价（含税）",
    "集采价（含税）",
    "市场价（含税）",
    "集采起订量",
    "供应商编号",
    "供应保障",
    "商品类型",
    "基础单位",
    "条码",
    "需求数量",
    "销售参考价",
    "需求备注",
    "询价说明",
    "单位编码",
    "配送关联",
];
pub(super) const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
impl BusinessTransfers {
    pub(super) async fn workbook(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<TransferResult> {
        if kind == TransferKind::SupplierQuoteArchive {
            return self.quote_archive(input, actor).await;
        }
        let rows = self.pool_source(input, actor).await?;
        let authorization = self.workbook_evidence("rows", &json!(rows), &rows).await?;
        let images = self.images(&rows).await?;
        let row_count = rows.len();
        let bytes = spawn_blocking(move || pool_workbook(&rows, &images))
            .await
            .map_err(|_| Error::Internal("Excel 生成中断".into()))??;
        if bytes.len() > 5 * 1024 * 1024 {
            return Err(Error::ValidationError("导出文件超过询价可导入的 5 MiB，请减少商品".into()));
        }
        let mut result = TransferResult::file("公司商品池.xlsx".into(), XLSX_MIME, bytes, authorization)?;
        result.metadata = json!({"row_count":row_count});
        Ok(result)
    }
    pub(super) async fn images(&self, rows: &[Value]) -> Result<HashMap<String, Vec<u8>>> {
        let mut images = HashMap::new();
        for id in rows.iter().filter_map(|row| row["main_image_asset_id"].as_str()) {
            if images.contains_key(id) {
                continue;
            }
            let asset = self
                .db
                .file_assets()
                .find_by_id(id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("商品图片不存在，请核对商品资料".into()))?;
            if asset.destroyed_at.is_some()
                || asset.expires_at.is_some_and(|at| at <= Instant::now())
                || asset.byte_size > 20 * 1024 * 1024
            {
                return Err(Error::ValidationError("商品图片不可用或超过 20 MiB".into()));
            }
            let bytes = timeout(Duration::from_secs(60), self.storage.read(&asset.storage_object_key))
                .await
                .map_err(|_| Error::Internal("商品图片读取超时，请重试".into()))?
                .map_err(|_| Error::Internal("商品图片读取失败，请重试".into()))?;
            let jpeg = spawn_blocking(move || thumbnail(bytes))
                .await
                .map_err(|_| Error::Internal("商品图片转换中断".into()))??;
            images.insert(id.into(), jpeg);
        }
        Ok(images)
    }
}
fn thumbnail(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(image_error());
    }
    let image = if image::guess_format(&bytes).map_err(|_| image_error())? == ImageFormat::Jpeg {
        jpeg_image(&bytes)?
    } else {
        let dimensions = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| image_error())?
            .into_dimensions()
            .map_err(|_| image_error())?;
        check_dimensions(dimensions.0, dimensions.1)?;
        image::load_from_memory(&bytes).map_err(|_| image_error())?
    };
    let thumbnail = image.thumbnail(152, 152);
    let mut out = Cursor::new(Vec::new());
    thumbnail.write_to(&mut out, ImageFormat::Png).map_err(|_| image_error())?;
    Ok(out.into_inner())
}
fn check_dimensions(width: u32, height: u32) -> Result<()> {
    if u64::from(width) * u64::from(height) > 40_000_000 {
        return Err(image_error());
    }
    Ok(())
}
fn jpeg_image(bytes: &[u8]) -> Result<DynamicImage> {
    // 复用关闭 SIMD 特性的解码器，避免本地 Cranelift 编译器不支持的 NEON 指令。
    let mut decoder = JpegDecoder::new(Cursor::new(bytes));
    decoder.decode_headers().map_err(|_| image_error())?;
    let info = decoder.info().ok_or_else(image_error)?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    check_dimensions(width, height)?;
    let decoded = decoder.decode().map_err(|_| image_error())?;
    if decoded.len() == (u64::from(width) * u64::from(height)) as usize {
        return GrayImage::from_raw(width, height, decoded)
            .map(DynamicImage::ImageLuma8)
            .ok_or_else(image_error);
    }
    RgbImage::from_raw(width, height, decoded).map(DynamicImage::ImageRgb8).ok_or_else(image_error)
}

fn image_error() -> Error {
    Error::ValidationError("图片格式或尺寸无效，请维护商品图片".into())
}
pub(super) fn xlsx(error: XlsxError) -> Error {
    let _ = error;
    Error::Internal("Excel 文件生成失败".into())
}
pub(super) fn text(row: &Value, field: &str) -> String {
    row[field].as_str().map(str::to_owned).unwrap_or_default()
}
pub(super) fn write_row(sheet: &mut Worksheet, row: u32, values: &[String]) -> StdResult<(), XlsxError> {
    for (index, value) in values.iter().enumerate() {
        let col = u16::try_from(index).map_err(|_| XlsxError::ParameterError("列数超限".into()))?;
        sheet.write_string_with_format(
            row,
            col,
            value,
            &Format::new().set_num_format("@").set_text_wrap(),
        )?;
    }
    Ok(())
}
fn pool_workbook(rows: &[Value], images: &HashMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    let mut book = Workbook::new();
    book.set_properties(&DocProperties::new().set_subject("sales-inquiry-pool-v4").set_author("公司商品池"));
    let sheet = book.add_worksheet();
    sheet.set_name("选品").map_err(xlsx)?;
    sheet.set_freeze_panes(2, 0).map_err(xlsx)?;
    sheet.set_column_range_width(0, 19, 18).map_err(xlsx)?;
    sheet.set_column_hidden(18).map_err(xlsx)?.set_column_hidden(19).map_err(xlsx)?;
    sheet
        .merge_range(
            0,
            0,
            0,
            19,
            "填写需求数量即表示选中商品，并在配送需求页填写数量及地址。发送客户前请自行处理内部信息。",
            &Format::new().set_text_wrap(),
        )
        .map_err(xlsx)?;
    write_row(sheet, 1, &HEADERS.map(str::to_owned)).map_err(xlsx)?;
    pool_rows(sheet, rows, images)?;
    let cols: Vec<_> = HEADERS.iter().map(|s| TableColumn::new().set_header(*s)).collect();
    sheet
        .add_table(
            1,
            0,
            u32::try_from(rows.len() + 1).map_err(|_| Error::ValidationError("商品数量超限".into()))?,
            19,
            &Table::new().set_name("InquirySelection").set_columns(&cols),
        )
        .map_err(xlsx)?;
    delivery_sheet(&mut book, rows).map_err(xlsx)?;
    pool_notes(&mut book)?;
    book.save_to_buffer().map_err(xlsx)
}
fn delivery_sheet(book: &mut Workbook, rows: &[Value]) -> StdResult<(), XlsxError> {
    let sheet = book.add_worksheet();
    sheet.set_name("配送需求")?;
    sheet.set_freeze_panes(1, 3)?;
    write_row(
        sheet,
        0,
        &[
            "商品名称",
            "SKU 编号",
            "SPU 编号",
            "收件人",
            "联系电话",
            "省",
            "市",
            "区县",
            "详细地址",
            "配送数量",
            "要求交付日期",
            "交付条件",
            "需求数量",
            "已分配数量",
            "剩余数量",
            "分配状态",
            "核对数量",
            "核对商品",
        ]
        .map(str::to_owned),
    )?;
    sheet.set_column_range_width(0, 15, 22)?;
    sheet.set_column_hidden(16)?.set_column_hidden(17)?;
    delivery_colors(sheet)?;
    for (i, row) in rows.iter().enumerate() {
        if row["product_kind"] == "VOUCHER" {
            continue;
        }
        let n = u32::try_from(i + 1).map_err(|_| XlsxError::ParameterError("行数超限".into()))?;
        quantity_checks(sheet, n)?;
        for (col, field) in [(0, "商品名称"), (1, "SKU 编号"), (2, "SPU 编号"), (9, "需求数量")] {
            let lookup = |field: &str| {
                format!(
                    "INDEX(InquirySelection[{field}],MATCH(\"line-{}\",InquirySelection[配送关联],0))",
                    i + 1
                )
            };
            let formula = format!("IF({}=\"\",\"\",{})", lookup("需求数量"), lookup(field));
            sheet.write_formula(n, col, Formula::new(formula).set_result(""))?;
        }
    }
    Ok(())
}

fn delivery_colors(sheet: &mut Worksheet) -> StdResult<(), XlsxError> {
    for (label, color) in [
        ("数量一致", "E2F0D9"),
        ("尚未分配完", "FFF2CC"),
        ("超出需求数量", "FCE4D6"),
        ("数量格式错误", "FCE4D6"),
    ] {
        let rule = ConditionalFormatText::new()
            .set_rule(ConditionalFormatTextRule::Contains(label.into()))
            .set_format(Format::new().set_background_color(color));
        sheet.add_conditional_format(1, 15, 9999, 15, &rule)?;
    }
    Ok(())
}

fn quantity_checks(sheet: &mut Worksheet, row: u32) -> StdResult<(), XlsxError> {
    let n = row + 1;
    let criterion =
        format!("SUBSTITUTE(SUBSTITUTE(SUBSTITUTE(R{n},\"~\",\"~~\"),\"*\",\"~*\"),\"?\",\"~?\")");
    let formulas = [
        format!(
            "IF(B{n}=\"\",\"\",INDEX(InquirySelection[需求数量],MATCH(B{n},InquirySelection[SKU 编号],0)))"
        ),
        format!("IF(B{n}=\"\",\"\",SUMIF($R$2:$R$10000,{criterion},$Q$2:$Q$10000))"),
        format!("IF(B{n}=\"\",\"\",IFERROR(ROUND(M{n}-N{n},6),\"数量格式错误\"))"),
        format!(
            "IF(B{n}=\"\",\"\",IF(COUNTIFS($R$2:$R$10000,{criterion},$Q$2:$Q$10000,\"数量格式错误\")>0,\"数量格式错误\",IF(O{n}=0,\"数量一致\",IF(O{n}>0,\"尚未分配完\",\"超出需求数量\"))))"
        ),
        format!(
            "IF(J{n}=\"\",0,IFERROR(IF(AND(VALUE(J{n})>0,ROUND(VALUE(J{n}),6)=VALUE(J{n})),VALUE(J{n}),\"数量格式错误\"),\"数量格式错误\"))"
        ),
        format!("IF(B{n}=\"\",\"\",\"sku:\"&B{n})"),
    ];
    for (i, formula) in formulas.iter().enumerate() {
        sheet.write_formula(
            row,
            u16::try_from(i + 12).map_err(|_| XlsxError::ParameterError("列数超限".into()))?,
            Formula::new(formula).set_result(""),
        )?;
    }
    Ok(())
}

fn pool_rows(sheet: &mut Worksheet, rows: &[Value], images: &HashMap<String, Vec<u8>>) -> Result<()> {
    for (i, row) in rows.iter().enumerate() {
        let n = u32::try_from(i + 2).map_err(|_| Error::ValidationError("商品数量超限".into()))?;
        let values = pool_values(row, i);
        write_row(sheet, n, &values).map_err(xlsx)?;
        sheet.set_row_height(n, 62).map_err(xlsx)?;
        if let Some(bytes) = row["main_image_asset_id"].as_str().and_then(|id| images.get(id)) {
            sheet.write_string(n, 0, "").map_err(xlsx)?;
            sheet
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

fn pool_notes(book: &mut Workbook) -> Result<()> {
    let notes = book.add_worksheet();
    notes.set_name("填写说明").map_err(xlsx)?;
    write_row(notes, 0, &["模板版本".into(), "sales-inquiry-pool-v4".into()]).map_err(xlsx)?;
    write_row(
        notes,
        1,
        &[
            "填写要求".into(),
            "每个地址填写一行。多地址时复制该商品整行，分别填写收件信息和配送数量；填写后导回系统校验。"
                .into(),
        ],
    )
    .map_err(xlsx)?;
    Ok(())
}

fn pool_values(row: &Value, i: usize) -> Vec<String> {
    let codes = row["supplier_codes"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("、"))
        .unwrap_or_default();
    let mut values = vec![
        "暂无图片".into(),
        text(row, "name"),
        text(row, "specification"),
        text(row, "sku_no"),
        text(row, "product_no"),
        text(row, "sales_visible_price_gross"),
        text(row, "bulk_price_gross"),
        text(row, "market_price"),
        text(row, "bulk_min_quantity"),
        codes,
        format!("{} 家可供", row["supplier_count"]),
        match row["product_kind"].as_str() {
            Some("PHYSICAL") => "实物商品",
            Some("VOUCHER") => "卡券",
            Some("VIRTUAL") => "虚拟商品",
            Some("OFFLINE_SERVICE") => "线下服务",
            _ => "未知类型",
        }
        .into(),
        text(row, "base_unit_name"),
        text(row, "barcode"),
        String::new(),
        String::new(),
        String::new(),
        "填写需求数量后参与询价".into(),
        text(row, "base_unit_code"),
        format!("line-{}", i + 1),
    ];
    if row["product_kind"] == "VOUCHER" {
        values[17] = "卡券不适用销售询价".into();
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sales_inquiry_import::parse;
    #[test]
    fn generated_pool_v4_roundtrips_through_real_inquiry_parser() {
        let row = json!({"name":"礼盒","sku_no":"001-A*","product_no":"P1","specification":"红","sales_visible_price_gross":"123456.1234","base_unit_code":"BOX","base_unit_name":"盒","product_kind":"PHYSICAL","supplier_count":1,"supplier_codes":["S1"]});
        let bytes = pool_workbook(&[row], &HashMap::new()).unwrap();
        let parsed = parse::parse(&bytes).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.product_sheet, "选品");
        assert!(parsed.automatic_delivery);
    }
    #[test]
    fn valid_png_and_jpeg_are_bounded_thumbnails() {
        let image = DynamicImage::new_rgb8(32, 32);
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png).unwrap();
        for bytes in [png.into_inner(), include_bytes!("../demo_master_data/images/01.jpg").to_vec()] {
            let result = thumbnail(bytes).unwrap();
            let dimensions = image::ImageReader::new(Cursor::new(result))
                .with_guessed_format()
                .unwrap()
                .into_dimensions()
                .unwrap();
            assert!(dimensions.0 <= 152 && dimensions.1 <= 152);
        }
    }
    #[test]
    fn unsupported_image_bytes_are_rejected() {
        assert!(thumbnail(vec![1, 2, 3]).is_err());
    }
}
