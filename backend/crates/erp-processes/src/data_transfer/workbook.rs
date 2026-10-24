use std::collections::HashMap;
use std::io::Cursor;
use std::result::Result as StdResult;
use std::time::Duration;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_sales::entity::sales_inquiry_pool::{
    DELIVERY_SHEET, INSTRUCTIONS_SHEET, POOL_DELIVERY_HEADERS, POOL_HEADERS as HEADERS, POOL_SHEET,
    POOL_TABLE_NAME, PoolTemplateVersion, automatic_delivery_formula,
};
use erp_support::FileAssetExt;
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use persistence_core::NoTransaction;
use rust_xlsxwriter::{
    ConditionalFormatText, ConditionalFormatTextRule, DocProperties, Format, FormatAlign, FormatBorder,
    Formula, Image, ObjectMovement, Table, TableColumn, TableStyle, Workbook, Worksheet, XlsxError,
};
use serde_json::{Value, json};
use tokio::task::spawn_blocking;
use tokio::time::timeout;
use zune_jpeg::JpegDecoder;

use super::business::BusinessTransfers;
use super::{TransferKind, TransferResult};
use crate::{Error, Result};

const POOL_WIDTHS: [u16; 20] =
    [18, 32, 24, 24, 24, 20, 18, 18, 16, 26, 16, 14, 12, 24, 16, 18, 36, 32, 16, 16];
const IMAGE_CELL_WIDTH: u32 = 128;
const PRODUCT_ROW_HEIGHT: u32 = 112;
const IMAGE_SIZE: u32 = 88;
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
    pool_book(rows, images)?.save_to_buffer().map_err(xlsx)
}

fn pool_book(rows: &[Value], images: &HashMap<String, Vec<u8>>) -> Result<Workbook> {
    let mut book = Workbook::new();
    book.set_properties(
        &DocProperties::new().set_subject(PoolTemplateVersion::CURRENT.marker()).set_author("公司商品池"),
    );
    let sheet = book.add_worksheet();
    sheet.set_name(POOL_SHEET).map_err(xlsx)?;
    configure_pool_sheet(sheet).map_err(xlsx)?;
    sheet
        .merge_range(
            0,
            0,
            0,
            17,
            "填写需求数量即表示选中商品，并在配送需求页填写数量及地址。发送客户前请自行处理内部信息。",
            &catalog_format().set_font_size(10).set_font_color("64748B"),
        )
        .map_err(xlsx)?;
    pool_rows(sheet, rows, images)?;
    let cols: Vec<_> = HEADERS
        .iter()
        .enumerate()
        .map(|(col, name)| TableColumn::new().set_header(*name).set_header_format(pool_header_format(col)))
        .collect();
    sheet
        .add_table(
            1,
            0,
            u32::try_from(rows.len() + 1).map_err(|_| Error::ValidationError("商品数量超限".into()))?,
            19,
            &Table::new()
                .set_name(POOL_TABLE_NAME)
                .set_style(TableStyle::None)
                .set_banded_rows(false)
                .set_columns(&cols),
        )
        .map_err(xlsx)?;
    delivery_sheet(&mut book, rows).map_err(xlsx)?;
    pool_notes(&mut book)?;
    Ok(book)
}

fn catalog_format() -> Format {
    Format::new()
        .set_font_name("微软雅黑")
        .set_font_size(11)
        .set_font_color("243247")
        .set_background_color("FFFFFF")
        .set_align(FormatAlign::VerticalCenter)
        .set_align(FormatAlign::Left)
        .set_text_wrap()
        .set_num_format("@")
}

fn catalog_header_format() -> Format {
    catalog_format().set_bold().set_border_bottom(FormatBorder::Thin).set_border_bottom_color("94A3B8")
}

fn pool_header_format(col: usize) -> Format {
    let format = catalog_header_format().set_align(FormatAlign::Center);
    if (14..=16).contains(&col) { format.set_font_color("245A81") } else { format }
}

fn pool_cell_format(col: usize) -> Format {
    let format = catalog_format().set_border_bottom(FormatBorder::Hair).set_border_bottom_color("E2E8F0");
    match col {
        0 | 10..=12 => format.set_align(FormatAlign::Center),
        5..=8 | 14..=15 => format.set_align(FormatAlign::Right).set_indent(1),
        1 => format.set_bold().set_indent(1),
        3 | 4 | 9 | 13 | 17 => format.set_font_color("64748B").set_indent(1),
        _ => format.set_indent(1),
    }
}

fn configure_pool_sheet(sheet: &mut Worksheet) -> StdResult<(), XlsxError> {
    sheet.set_screen_gridlines(false);
    sheet.set_freeze_panes(2, 2)?;
    sheet.set_row_height(0, 32)?;
    sheet.set_row_height(1, 36)?;
    for (col, width) in POOL_WIDTHS.iter().enumerate() {
        let col = u16::try_from(col).map_err(|_| XlsxError::ParameterError("列数超限".into()))?;
        sheet.set_column_width(col, *width)?;
    }
    sheet.set_column_width_pixels(0, IMAGE_CELL_WIDTH)?;
    sheet.set_column_hidden(18)?.set_column_hidden(19)?;
    Ok(())
}
fn delivery_sheet(book: &mut Workbook, rows: &[Value]) -> StdResult<(), XlsxError> {
    let sheet = book.add_worksheet();
    sheet.set_name(DELIVERY_SHEET)?;
    sheet.set_screen_gridlines(false);
    sheet.set_freeze_panes(1, 3)?;
    configure_delivery_sheet(sheet)?;
    sheet.set_column_hidden(16)?.set_column_hidden(17)?;
    delivery_colors(sheet)?;
    for (i, row) in rows.iter().enumerate() {
        if row["product_kind"] == "VOUCHER" {
            continue;
        }
        let n = u32::try_from(i + 1).map_err(|_| XlsxError::ParameterError("行数超限".into()))?;
        sheet.set_row_height(n, 42)?;
        quantity_checks(sheet, n)?;
        for (col, field) in [(0, "商品名称"), (1, "SKU 编号"), (2, "SPU 编号"), (9, "需求数量")] {
            let formula = automatic_delivery_formula(&format!("line-{}", i + 1), field);
            sheet.write_formula(n, col, Formula::new(formula).set_result(""))?;
        }
    }
    Ok(())
}

fn configure_delivery_sheet(sheet: &mut Worksheet) -> StdResult<(), XlsxError> {
    let headers = POOL_DELIVERY_HEADERS
        .iter()
        .chain(PoolTemplateVersion::CURRENT.check_headers())
        .copied()
        .collect::<Vec<_>>();
    let widths = [32, 24, 24, 16, 22, 16, 16, 16, 40, 16, 20, 28, 16, 16, 16, 24, 16, 24];
    sheet.set_row_height(0, 36)?;
    for (col, width) in widths.iter().enumerate() {
        let col = u16::try_from(col).map_err(|_| XlsxError::ParameterError("列数超限".into()))?;
        let format = if col == 9 || (12..=14).contains(&col) {
            catalog_format().set_align(FormatAlign::Right).set_indent(1)
        } else {
            catalog_format().set_indent(1)
        };
        sheet.set_column_width(col, *width)?.set_column_format(col, &format)?;
        sheet.write_string_with_format(0, col, headers[usize::from(col)], &catalog_header_format())?;
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
    let formats: Vec<_> = (0..HEADERS.len()).map(pool_cell_format).collect();
    for (i, row) in rows.iter().enumerate() {
        let n = u32::try_from(i + 2).map_err(|_| Error::ValidationError("商品数量超限".into()))?;
        let image = row["main_image_asset_id"].as_str().and_then(|id| images.get(id));
        let mut values = pool_values(row, i);
        if image.is_some() {
            values[0].clear();
        }
        for (col, value) in values.iter().enumerate() {
            let column = u16::try_from(col).map_err(|_| Error::ValidationError("列数超限".into()))?;
            sheet.write_string_with_format(n, column, value, &formats[col]).map_err(xlsx)?;
        }
        sheet.set_row_height_pixels(n, PRODUCT_ROW_HEIGHT).map_err(xlsx)?;
        if let Some(bytes) = image {
            insert_pool_image(sheet, n, bytes)?;
        }
    }
    Ok(())
}

fn insert_pool_image(sheet: &mut Worksheet, row: u32, bytes: &[u8]) -> Result<()> {
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| image_error())?
        .into_dimensions()
        .map_err(|_| image_error())?;
    let longest = width.max(height);
    if longest == 0 {
        return Err(image_error());
    }
    // 图片已由 thumbnail 限制为 152px；按等比缩放后的尺寸计算留白。
    let x = (IMAGE_CELL_WIDTH - width * IMAGE_SIZE / longest) / 2;
    let y = (PRODUCT_ROW_HEIGHT - height * IMAGE_SIZE / longest) / 2;
    let image = Image::new_from_buffer(bytes).map_err(xlsx)?;
    sheet
        .insert_image_with_offset(
            row,
            0,
            &image
                .set_scale_to_size(IMAGE_SIZE, IMAGE_SIZE, true)
                .set_object_movement(ObjectMovement::MoveAndSizeWithCells),
            x,
            y,
        )
        .map_err(xlsx)?;
    Ok(())
}

fn pool_notes(book: &mut Workbook) -> Result<()> {
    let notes = book.add_worksheet();
    notes.set_name(INSTRUCTIONS_SHEET).map_err(xlsx)?;
    notes.set_screen_gridlines(false);
    notes.set_column_width(0, 20).map_err(xlsx)?;
    notes.set_column_width(1, 100).map_err(xlsx)?;
    write_row(notes, 0, &["模板版本".into(), PoolTemplateVersion::CURRENT.marker().into()]).map_err(xlsx)?;
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
    for row in 0..2 {
        notes.set_row_height(row, if row == 0 { 30 } else { 54 }).map_err(xlsx)?;
        notes.set_cell_format(row, 0, &catalog_format().set_bold()).map_err(xlsx)?;
        notes.set_cell_format(row, 1, &catalog_format()).map_err(xlsx)?;
    }
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
    use std::io::{Read, Write};

    use zip::write::SimpleFileOptions;
    use zip::{ZipArchive, ZipWriter};

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

    fn filled_pool_bytes() -> Vec<u8> {
        let rows = [
            json!({"name":"=1+1 礼盒","sku_no":"001-A*","product_no":"000-SPU","specification":"红","sales_visible_price_gross":"999.9999","bulk_price_gross":"888.8888","base_unit_code":"BOX","base_unit_name":"盒","product_kind":"PHYSICAL","supplier_count":1,"supplier_codes":["private-supplier"]}),
            json!({"name":"未选商品","sku_no":"002","product_no":"P2","base_unit_code":"PCS","product_kind":"PHYSICAL","supplier_count":0}),
        ];
        let mut book = pool_book(&rows, &HashMap::new()).unwrap();
        let sheet = book.worksheet_from_name(POOL_SHEET).unwrap();
        for (column, value) in [(14, "2.500001"), (15, "12.5001"), (16, "客户填写备注")] {
            sheet.write_string(2, column, value).unwrap();
        }
        let sheet = book.worksheet_from_name(DELIVERY_SHEET).unwrap();
        for (column, value) in [
            (3, "张三"),
            (4, "0013800000000"),
            (5, "广东省"),
            (6, "深圳市"),
            (7, "南山区"),
            (8, "科技园 001 号"),
            (10, "2027-01-02"),
            (11, "工作日配送"),
        ] {
            sheet.write_string(1, column, value).unwrap();
        }
        book.save_to_buffer().unwrap()
    }

    fn replace_shared_text(bytes: Vec<u8>, from: &str, to: &str) -> Vec<u8> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().to_string();
            let mut content = Vec::new();
            file.read_to_end(&mut content).unwrap();
            if name == "xl/sharedStrings.xml" {
                let xml = String::from_utf8(content).unwrap();
                assert!(xml.contains(from));
                content = xml.replace(from, to).into_bytes();
            }
            writer.start_file(name, SimpleFileOptions::default()).unwrap();
            writer.write_all(&content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn filled_export_preserves_exact_demand_and_never_imports_display_prices_or_supplier() {
        let parsed = parse::parse(&filled_pool_bytes()).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.products.len(), 1);
        let product = &parsed.products[0];
        assert_eq!(product.number, 3);
        assert_eq!(
            product.cells.iter().map(|cell| cell.value.as_str()).collect::<Vec<_>>(),
            ["001-A*", "=1+1 礼盒", "红", "BOX", "2.500001", "12.5001", "客户填写备注"]
        );
        assert!(product.cells.iter().all(|cell| cell.text && !cell.formula));
        assert_eq!(parsed.deliveries.len(), 1);
        assert_eq!(
            parsed.deliveries[0].cells.iter().map(|cell| cell.value.as_str()).collect::<Vec<_>>(),
            [
                "001-A*",
                "张三",
                "0013800000000",
                "广东省",
                "深圳市",
                "南山区",
                "科技园 001 号",
                "2.500001",
                "2027-01-02",
                "工作日配送"
            ]
        );
        assert!(parsed.deliveries[0].cells.iter().all(|cell| !cell.formula));
    }

    #[test]
    fn filled_export_accepts_legacy_price_header_and_rejects_both_price_names() {
        let bytes = filled_pool_bytes();
        let legacy = replace_shared_text(bytes.clone(), "<t>销售价</t>", "<t>销售参考价</t>");
        let parsed = parse::parse(&legacy).unwrap();
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.products[0].cells[5].value, "12.5001");
        let duplicate = replace_shared_text(bytes, "<t>条码</t>", "<t>销售参考价</t>");
        assert!(
            matches!(parse::parse(&duplicate), Err(Error::ValidationError(message)) if message.contains("重复表头"))
        );
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

    #[test]
    fn pool_images_keep_aspect_ratio_and_centered_padding() {
        for (width, height, x, y, cx, cy) in
            [(152, 76, 20, 34, 88, 44), (76, 152, 42, 12, 44, 88), (152, 152, 20, 12, 88, 88)]
        {
            let mut image = Cursor::new(Vec::new());
            DynamicImage::new_rgb8(width, height).write_to(&mut image, ImageFormat::Png).unwrap();
            let row =
                json!({"name":"礼盒","sku_no":"001","product_kind":"PHYSICAL","main_image_asset_id":"image"});
            let bytes =
                pool_workbook(&[row], &HashMap::from([("image".into(), image.into_inner())])).unwrap();
            assert!(parse::parse(&bytes).unwrap().errors.is_empty());
            let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
            let mut drawing = String::new();
            archive.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut drawing).unwrap();
            assert!(drawing.contains(&format!("<xdr:colOff>{}</xdr:colOff>", x * 9525)));
            assert!(drawing.contains(&format!("<xdr:rowOff>{}</xdr:rowOff>", y * 9525)));
            assert!(drawing.contains(&format!("<a:ext cx=\"{}\" cy=\"{}\"/>", cx * 9525, cy * 9525)));
        }
    }

    #[test]
    fn pool_rejects_invalid_image_without_producing_a_partial_workbook() {
        let row = json!({"main_image_asset_id":"broken"});
        assert!(pool_workbook(&[row], &HashMap::from([("broken".into(), vec![1, 2, 3])])).is_err());
    }
}
