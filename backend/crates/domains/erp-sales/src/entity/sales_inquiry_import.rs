//! 客户询价工作簿的已校验预览；文件和确认结果均受原询价权限约束。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::{Quantity, UnitPrice};
use erp_core::region::{RegionCode, all_regions};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::sales_inquiry::{
    InquiryDeliveryAllocation, InquiryFulfillmentKind, InquiryRequirementData, InquiryRequirementLine,
};
use crate::{Error, Result};

/// 首版客户工作簿版本。
pub const INQUIRY_TEMPLATE_VERSION: &str = "sales-inquiry-v1";
/// 压缩后的工作簿上限。
pub const INQUIRY_IMPORT_MAX_BYTES: usize = 5 * 1024 * 1024;
/// 配送工作表的数据行上限。
pub const INQUIRY_IMPORT_MAX_ROWS: usize = 2000;

/// 定位到工作表及原始 Excel 行的校验问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryImportIssue {
    pub sheet: String,
    pub row_number: u32,
    pub column: String,
    pub message: String,
}

/// 明确展示的重复配送行及合并结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryImportMerge {
    pub sheet: String,
    pub row_numbers: Vec<u32>,
    pub sku_code: String,
    pub merged_quantity: String,
}

/// 导入确认成功后冻结的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryImportResult {
    pub inquiry_id: String,
    pub preview_id: String,
    pub version: u64,
    pub line_count: usize,
    pub allocation_count: usize,
}

/// 解析边界交付的客户商品原始行。
pub struct InquiryImportProduct {
    pub row_number: u32,
    pub sku_code: String,
    pub sku_is_text: bool,
    pub unit: String,
    pub quantity: String,
    pub sales_reference_price: String,
    pub note: String,
}

/// 解析边界交付的客户配送原始行。
pub struct InquiryImportDelivery {
    pub row_number: u32,
    pub sku_code: String,
    pub sku_is_text: bool,
    pub recipient: String,
    pub phone: String,
    pub phone_is_text: bool,
    pub province: String,
    pub city: String,
    pub district: String,
    pub address: String,
    pub quantity: String,
    pub due_date: String,
    pub terms: String,
}

/// 由当前授权可售资格读取器提供的最小公司商品事实。
#[derive(Clone)]
pub struct InquiryImportSku {
    pub sku_id: String,
    pub sku_revision_id: String,
    pub sku_code: String,
    pub name: String,
    pub specification: String,
    pub unit_code: String,
    pub quantity_scale: u32,
    pub fulfillment_kind: InquiryFulfillmentKind,
}

/// 纯业务校验和合并的完整结果；任何错误均禁止确认。
pub struct InquiryImportValidation {
    pub data: InquiryRequirementData,
    pub errors: Vec<InquiryImportIssue>,
    pub merges: Vec<InquiryImportMerge>,
}

struct ValidatedProduct<'a> {
    raw: &'a InquiryImportProduct,
    sku: &'a InquiryImportSku,
    quantity: Quantity,
    price: Option<UnitPrice>,
}

type DeliveryGroup<'a> = (&'a str, &'a str, &'a str);

impl InquiryImportValidation {
    /// 按精确编码、交期和交付条件建立需求，并显式记录重复配送行合并。
    /// # 参数
    /// `products`、`deliveries` 为原始行，`skus` 仅含当前授权可售商品，
    /// `current` 为保持稳定身份的原草稿，`new_id` 只在新增身份时调用。
    /// # 返回
    /// 返回可展示的需求、逐行错误及待销售确认的合并结果。
    /// # 错误
    /// 全部业务错误收集在 `errors`，不执行任何写入。
    pub fn build(
        products: &[InquiryImportProduct],
        deliveries: &[InquiryImportDelivery],
        skus: &[InquiryImportSku],
        current: &InquiryRequirementData,
        mut new_id: impl FnMut() -> String,
    ) -> Self {
        let mut result = Self { data: InquiryRequirementData::default(), errors: vec![], merges: vec![] };
        let products = validate_products(products, skus, &mut result.errors);
        let groups = group_deliveries(deliveries, &products, &mut result.errors);
        for ((code, due, terms), rows) in groups {
            let Some(product) = products.get(code) else {
                continue;
            };
            let old = current.lines.iter().find(|line| {
                line.sku_id == product.sku.sku_id
                    && line.delivery_due_at == due_instant(due).unwrap_or(line.delivery_due_at)
                    && line.delivery_terms == terms
            });
            if let Some(line) = build_line(product, due, terms, &rows, old, &mut result, &mut new_id) {
                result.data.lines.push(line);
            }
        }
        check_product_totals(&products, deliveries, &result.data, &mut result.errors);
        result.data.note = products
            .values()
            .filter(|product| !product.raw.note.is_empty())
            .map(|product| format!("{}：{}", product.raw.sku_code, product.raw.note))
            .collect::<Vec<_>>()
            .join("\n");
        if result.errors.is_empty()
            && let Err(error) = result.data.normalize(true)
        {
            push_issue(&mut result.errors, "商品清单", 1, "需求", &error.to_string());
        }
        result
    }
}

fn validate_products<'a>(
    products: &'a [InquiryImportProduct],
    skus: &'a [InquiryImportSku],
    errors: &mut Vec<InquiryImportIssue>,
) -> BTreeMap<String, ValidatedProduct<'a>> {
    let mut valid = BTreeMap::new();
    for row in products {
        if !row.sku_is_text {
            push_issue(errors, "商品清单", row.row_number, "SKU编码", "SKU 编码必须为文本，保留前导零");
        }
        if valid.contains_key(&row.sku_code) {
            push_issue(errors, "商品清单", row.row_number, "SKU编码", "商品清单同一 SKU 只允许一行");
            continue;
        }
        let Some(sku) = skus.iter().find(|sku| sku.sku_code == row.sku_code) else {
            push_issue(errors, "商品清单", row.row_number, "SKU编码", "商品不存在或不可选用");
            continue;
        };
        if row.unit != sku.unit_code {
            push_issue(errors, "商品清单", row.row_number, "单位", "单位与当前公司商品不一致");
        }
        let quantity = match positive_quantity(&row.quantity, sku.quantity_scale) {
            Ok(quantity) => quantity,
            Err(message) => {
                push_issue(errors, "商品清单", row.row_number, "需求数量", &message);
                continue;
            },
        };
        let price = match reference_price(&row.sales_reference_price) {
            Ok(price) => price,
            Err(message) => {
                push_issue(errors, "商品清单", row.row_number, "销售价", &message);
                continue;
            },
        };
        valid.insert(row.sku_code.clone(), ValidatedProduct { raw: row, sku, quantity, price });
    }
    if products.is_empty() {
        push_issue(errors, "商品清单", 2, "SKU编码", "至少填写一条商品需求");
    }
    valid
}

fn group_deliveries<'a>(
    rows: &'a [InquiryImportDelivery],
    products: &BTreeMap<String, ValidatedProduct<'_>>,
    errors: &mut Vec<InquiryImportIssue>,
) -> BTreeMap<DeliveryGroup<'a>, Vec<&'a InquiryImportDelivery>> {
    let mut groups: BTreeMap<DeliveryGroup<'_>, Vec<&InquiryImportDelivery>> = BTreeMap::new();
    for row in rows {
        if !row.sku_is_text {
            push_issue(errors, "配送需求", row.row_number, "SKU编码", "SKU 编码必须为文本，保留前导零");
        }
        let physical = products
            .get(&row.sku_code)
            .is_some_and(|product| product.sku.fulfillment_kind == InquiryFulfillmentKind::Physical);
        check_phone(row, physical, errors);
        if !products.contains_key(&row.sku_code) {
            push_issue(
                errors,
                "配送需求",
                row.row_number,
                "SKU编码",
                "商品不存在或不可选用，或未列入商品清单",
            );
            continue;
        }
        if due_instant(&row.due_date).is_none() {
            push_issue(
                errors,
                "配送需求",
                row.row_number,
                "要求交付日期",
                if row.due_date.is_empty() {
                    "请填写要求交付日期"
                } else {
                    "交付日期无效，请填写 YYYY-MM-DD 文本或不含时间的 Excel 日期"
                },
            );
            continue;
        }
        groups.entry((&row.sku_code, &row.due_date, &row.terms)).or_default().push(row);
    }
    groups
}

/// 区分必填缺失和无法可靠还原的数字电话；解析层已将明确的数字手机号转为文本。
fn check_phone(row: &InquiryImportDelivery, physical: bool, errors: &mut Vec<InquiryImportIssue>) {
    let message = if row.phone.is_empty() && physical {
        "请填写联系电话"
    } else if !row.phone.is_empty() && !row.phone_is_text {
        "数字电话仅支持 11 位大陆手机号；座机、国际号码等请按文本填写并核对前导零"
    } else {
        return;
    };
    push_issue(errors, "配送需求", row.row_number, "联系电话", message);
}

fn build_line(
    product: &ValidatedProduct<'_>,
    due: &str,
    terms: &str,
    rows: &[&InquiryImportDelivery],
    old: Option<&InquiryRequirementLine>,
    result: &mut InquiryImportValidation,
    new_id: &mut impl FnMut() -> String,
) -> Option<InquiryRequirementLine> {
    let without_address = product.sku.fulfillment_kind != InquiryFulfillmentKind::Physical
        && rows.iter().all(|row| {
            [&row.recipient, &row.phone, &row.province, &row.city, &row.district, &row.address]
                .iter()
                .all(|value| value.is_empty())
        });
    let allocations =
        if without_address { vec![] } else { line_allocations(product, rows, old, result, new_id) };
    let quantity = if without_address {
        unallocated_quantity(product, rows, result)?
    } else {
        sum_quantities(allocations.iter().map(|allocation| allocation.quantity)).ok()?
    };
    Some(InquiryRequirementLine {
        line_id: old.map_or_else(&mut *new_id, |line| line.line_id.clone()),
        sku_id: product.sku.sku_id.clone(),
        sku_revision_id: product.sku.sku_revision_id.clone(),
        sku_code: product.sku.sku_code.clone(),
        product_name: product.sku.name.clone(),
        specification: product.sku.specification.clone(),
        unit_code: product.sku.unit_code.clone(),
        quantity_scale: product.sku.quantity_scale,
        quantity,
        fulfillment_kind: product.sku.fulfillment_kind,
        delivery_due_at: due_instant(due)?,
        delivery_terms: terms.into(),
        sales_reference_price_gross: product.price,
        allocations,
    })
}

fn line_allocations(
    product: &ValidatedProduct<'_>,
    rows: &[&InquiryImportDelivery],
    old: Option<&InquiryRequirementLine>,
    result: &mut InquiryImportValidation,
    new_id: &mut impl FnMut() -> String,
) -> Vec<InquiryDeliveryAllocation> {
    let mut allocations = Vec::new();
    let mut groups: HashMap<String, Vec<u32>> = HashMap::new();
    for row in rows {
        let allocation = match delivery_allocation(row, product.sku.quantity_scale, old, new_id) {
            Ok(allocation) => allocation,
            Err((column, message)) => {
                push_issue(&mut result.errors, "配送需求", row.row_number, column, &message);
                continue;
            },
        };
        let key = allocation_key(&allocation);
        groups.entry(key.clone()).or_default().push(row.row_number);
        if let Some(existing) = allocations.iter_mut().find(|value| allocation_key(value) == key) {
            match add_quantity(existing.quantity, allocation.quantity) {
                Ok(quantity) => existing.quantity = quantity,
                Err(message) => {
                    push_issue(&mut result.errors, "配送需求", row.row_number, "配送数量", &message)
                },
            }
        } else {
            allocations.push(allocation);
        }
    }
    add_merges(&product.raw.sku_code, &allocations, &groups, &mut result.merges);
    if allocations.len() > 100 {
        push_issue(
            &mut result.errors,
            "配送需求",
            rows[0].row_number,
            "详细地址",
            "同 SKU、交期及交付条件最多允许 100 个收货地址",
        );
    }
    allocations
}

fn unallocated_quantity(
    product: &ValidatedProduct<'_>,
    rows: &[&InquiryImportDelivery],
    result: &mut InquiryImportValidation,
) -> Option<Quantity> {
    let mut values = Vec::new();
    for row in rows {
        match positive_quantity(&row.quantity, product.sku.quantity_scale) {
            Ok(value) => values.push(value),
            Err(message) => push_issue(&mut result.errors, "配送需求", row.row_number, "配送数量", &message),
        }
    }
    let quantity = sum_quantities(values.into_iter()).ok()?;
    if rows.len() > 1 {
        result.merges.push(InquiryImportMerge {
            sheet: "配送需求".into(),
            row_numbers: rows.iter().map(|row| row.row_number).collect(),
            sku_code: product.sku.sku_code.clone(),
            merged_quantity: quantity.to_string(),
        });
    }
    Some(quantity)
}

fn delivery_allocation(
    row: &InquiryImportDelivery,
    scale: u32,
    old: Option<&InquiryRequirementLine>,
    new_id: &mut impl FnMut() -> String,
) -> std::result::Result<InquiryDeliveryAllocation, (&'static str, String)> {
    for (value, label, limit) in
        [(&row.recipient, "收件人", 64), (&row.phone, "联系电话", 32), (&row.address, "详细地址", 512)]
    {
        if value.is_empty() || value.chars().count() > limit {
            return Err((label, format!("{label}不能为空且不得超过 {limit} 字")));
        }
    }
    let region_code =
        resolve_region(&row.province, &row.city, &row.district).map_err(|error| ("区县", error))?;
    let quantity = positive_quantity(&row.quantity, scale).map_err(|error| ("配送数量", error))?;
    let mut allocation = InquiryDeliveryAllocation {
        allocation_id: String::new(),
        recipient: row.recipient.clone(),
        phone: row.phone.clone(),
        region_name: region_code.full_name(),
        region_code,
        address: row.address.clone(),
        quantity,
    };
    allocation.allocation_id = old
        .and_then(|line| {
            line.allocations.iter().find(|old| allocation_key(old) == allocation_key(&allocation))
        })
        .map_or_else(new_id, |old| old.allocation_id.clone());
    Ok(allocation)
}

fn check_product_totals(
    products: &BTreeMap<String, ValidatedProduct<'_>>,
    deliveries: &[InquiryImportDelivery],
    data: &InquiryRequirementData,
    errors: &mut Vec<InquiryImportIssue>,
) {
    let invalid_products: BTreeSet<_> =
        errors.iter().filter(|error| error.sheet == "商品清单").map(|error| error.row_number).collect();
    let invalid_deliveries: BTreeSet<_> =
        errors.iter().filter(|error| error.sheet == "配送需求").map(|error| error.row_number).collect();
    let incomplete_codes: BTreeSet<_> = deliveries
        .iter()
        .filter(|row| invalid_deliveries.contains(&row.row_number))
        .map(|row| row.sku_code.as_str())
        .collect();
    for (code, product) in products {
        // 字段错误会使配送行无法形成完整需求，此时不把部分结果误报为数量不守恒。
        if invalid_products.contains(&product.raw.row_number) || incomplete_codes.contains(code.as_str()) {
            continue;
        }
        match sum_quantities(
            data.lines.iter().filter(|line| line.sku_code == *code).map(|line| line.quantity),
        ) {
            Ok(sum) if sum == product.quantity => {},
            _ => push_issue(
                errors,
                "商品清单",
                product.raw.row_number,
                "需求数量",
                "配送数量合计必须等于商品需求数量",
            ),
        }
    }
}

fn add_merges(
    code: &str,
    allocations: &[InquiryDeliveryAllocation],
    groups: &HashMap<String, Vec<u32>>,
    merges: &mut Vec<InquiryImportMerge>,
) {
    for allocation in allocations {
        if let Some(rows) = groups.get(&allocation_key(allocation)).filter(|rows| rows.len() > 1) {
            merges.push(InquiryImportMerge {
                sheet: "配送需求".into(),
                row_numbers: rows.clone(),
                sku_code: code.into(),
                merged_quantity: allocation.quantity.to_string(),
            });
        }
    }
}

fn allocation_key(row: &InquiryDeliveryAllocation) -> String {
    // JSON 长度边界避免地址或姓名中包含分隔字符造成错误合并。
    serde_json::to_string(&[&row.recipient, &row.phone, row.region_code.as_str(), &row.address])
        .expect("string array serialization cannot fail")
}

fn positive_quantity(value: &str, scale: u32) -> std::result::Result<Quantity, String> {
    let quantity: Quantity = value.parse().map_err(|_| "数量格式无效，最多 6 位小数".to_string())?;
    if quantity.to_decimal() <= Decimal::ZERO || quantity.to_decimal().normalize().scale() > scale {
        return Err(format!("数量须为正数且最多 {scale} 位小数"));
    }
    Ok(quantity)
}

fn reference_price(value: &str) -> std::result::Result<Option<UnitPrice>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    let price: UnitPrice = value.parse().map_err(|_| "销售价格式无效，最多 4 位小数".to_string())?;
    if price.to_decimal() < Decimal::ZERO {
        return Err("销售价不得为负数".into());
    }
    Ok(Some(price))
}

fn sum_quantities(mut values: impl Iterator<Item = Quantity>) -> std::result::Result<Quantity, String> {
    let sum = values.try_fold(Decimal::ZERO, |sum, value| {
        sum.checked_add(value.to_decimal()).ok_or_else(|| "数量合计超出允许范围".to_string())
    })?;
    Quantity::try_from(sum).map_err(|_| "数量合计超出允许范围".into())
}

fn add_quantity(left: Quantity, right: Quantity) -> std::result::Result<Quantity, String> {
    sum_quantities([left, right].into_iter())
}

fn due_instant(value: &str) -> Option<Instant> {
    if value.len() != 10 {
        return None;
    }
    let day = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    if day.format("%Y-%m-%d").to_string() != value {
        return None;
    }
    // 客户交期按中国标准时间当天 23:59:59 冻结为 UTC 秒。
    Instant::try_from_unix_secs(day.and_hms_opt(15, 59, 59)?.and_utc().timestamp()).ok()
}

fn resolve_region(province: &str, city: &str, district: &str) -> std::result::Result<RegionCode, String> {
    let nodes = all_regions();
    let mut candidates = nodes
        .iter()
        .filter(|node| node.name == district || node.code == district)
        .filter(|node| {
            let Some(parent) = nodes.iter().find(|candidate| Some(candidate.code) == node.parent_code) else {
                return false;
            };
            let Some(grandparent) = nodes.iter().find(|candidate| Some(candidate.code) == parent.parent_code)
            else {
                return false;
            };
            let province_matches = grandparent.name == province || grandparent.code == province;
            let city_matches = parent.name == city
                || parent.code == city
                || (parent.name == "市辖区" && grandparent.name == city);
            province_matches && city_matches
        })
        .filter_map(|node| RegionCode::parse_leaf(node.code).ok());
    match (candidates.next(), candidates.next()) {
        (Some(region), None) => Ok(region),
        _ => Err("省、市、区县无法唯一匹配，请填写完整标准名称或地区代码".into()),
    }
}

fn push_issue(
    errors: &mut Vec<InquiryImportIssue>,
    sheet: &str,
    row_number: u32,
    column: &str,
    message: &str,
) {
    // 同一字段保留最先定位的原因，避免后续地址构建重复报告空电话等错误。
    if errors
        .iter()
        .any(|error| error.sheet == sheet && error.row_number == row_number && error.column == column)
    {
        return;
    }
    errors.push(InquiryImportIssue {
        sheet: sheet.into(),
        row_number,
        column: column.into(),
        message: message.into(),
    });
}

/// 持久化预览只保存客户字段及系统商品快照，不保存供给资料。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct InquiryImportPreview {
    #[serde(flatten)]
    pub base: BaseModel,
    pub inquiry_id: String,
    pub actor_id: String,
    /// 新建导入绑定的客户；旧预览缺省为已有询价导入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation_customer_id: Option<String>,
    pub expected_version: u64,
    pub input_fingerprint: String,
    pub errors: Vec<InquiryImportIssue>,
    pub merges: Vec<InquiryImportMerge>,
    pub data: InquiryRequirementData,
    pub expires_at: i64,
    pub result: Option<InquiryImportResult>,
}

/// 导入命令的独立强类型回执；不能依赖过期预览恢复结果。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct InquiryImportReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub actor_id: String,
    pub inquiry_id: String,
    pub payload_fingerprint: String,
    pub result: InquiryImportResult,
}

impl InquiryImportReceipt {
    /// 核对命令身份、请求指纹及强类型结果后恢复首次确认结果。
    /// # 参数
    /// `command_id`、`actor`、`inquiry`、`preview_id` 与 `fingerprint` 为当前命令身份。
    /// # 返回
    /// 返回首次成功确认冻结的结果，不依赖仍然存在的临时预览。
    /// # 错误
    /// 回执结构损坏时拒绝恢复；同一操作号的对象或载荷不同时返回冲突。
    pub fn recover(
        &self,
        command_id: &str,
        actor: &str,
        inquiry: &str,
        preview_id: &str,
        fingerprint: &str,
    ) -> Result<InquiryImportResult> {
        if self.base.is_deleted()
            || self.base.id != command_id
            || self.result.inquiry_id != self.inquiry_id
            || self.actor_id.is_empty()
            || self.inquiry_id.is_empty()
            || self.result.preview_id.is_empty()
            || self.result.version == 0
            || self.result.line_count == 0
            || self.result.line_count > 200
            || self.result.allocation_count > INQUIRY_IMPORT_MAX_ROWS
            || self.payload_fingerprint.len() != 64
            || !self.payload_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::Internal("导入命令回执无效".into()));
        }
        if self.actor_id != actor
            || self.inquiry_id != inquiry
            || self.result.preview_id != preview_id
            || self.payload_fingerprint != fingerprint
        {
            return Err(Error::ConflictError("同一操作号已用于不同的导入确认".into()));
        }
        Ok(self.result.clone())
    }
}

impl InquiryImportPreview {
    /// 核对未创建询价的客户专用预览。
    /// # 参数
    /// `actor`、`customer`、`fingerprint` 为请求绑定，`merge` 为合并确认，`now` 为当前秒。
    /// # 返回
    /// 返回可一次性创建的已校验需求。
    /// # 错误
    /// 客户或用途不符、已处理、过期、文件错误及未确认合并时拒绝。
    pub fn ensure_creatable(
        &self,
        actor: &str,
        customer: &str,
        fingerprint: &str,
        merge: bool,
        now: i64,
    ) -> Result<()> {
        if self.creation_customer_id.as_deref() != Some(customer) || !self.inquiry_id.is_empty() {
            return Err(Error::NotFound("客户导入预览不存在或不可用".into()));
        }
        self.ensure_confirmable(actor, "", 0, fingerprint, merge, now)?;
        if self.result.is_some() {
            return Err(Error::ConflictError("该文件预览已创建询价，请打开原询价".into()));
        }
        Ok(())
    }

    /// 核对预览绑定及确认前置条件。
    /// # 参数
    /// `actor_id`、`inquiry_id`、`version` 和 `fingerprint` 为当前请求上下文，
    /// `confirm_merge` 为销售明确确认，`now` 为服务端 Unix 秒。
    /// # 返回
    /// 全部匹配且预览没有错误时返回空结果。
    /// # 错误
    /// 绑定不符、过期、错误未修正或重复行未经确认时拒绝导入。
    pub fn ensure_confirmable(
        &self,
        actor_id: &str,
        inquiry_id: &str,
        version: u64,
        fingerprint: &str,
        confirm_merge: bool,
        now: i64,
    ) -> Result<()> {
        if self.actor_id != actor_id || self.inquiry_id != inquiry_id || self.base.is_deleted() {
            return Err(Error::NotFound("导入预览不存在或不可用".into()));
        }
        if self.expected_version != version || self.input_fingerprint != fingerprint {
            return Err(Error::ConflictError("需求版本或原始文件指纹已变化，请重新预览".into()));
        }
        if self.expires_at <= now {
            return Err(Error::ConflictError("导入预览已过期，请重新预览".into()));
        }
        if !self.errors.is_empty() {
            return Err(Error::ValidationError("预览存在错误，修正或移除错误行后重新预览".into()));
        }
        if !self.merges.is_empty() && !confirm_merge {
            return Err(Error::ValidationError("请明确确认预览中的重复配送行合并结果".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product() -> InquiryImportProduct {
        InquiryImportProduct {
            row_number: 2,
            sku_code: "001".into(),
            sku_is_text: true,
            unit: "BOX".into(),
            quantity: "3".into(),
            sales_reference_price: "12.3456".into(),
            note: "客户参考".into(),
        }
    }

    fn sku() -> InquiryImportSku {
        InquiryImportSku {
            sku_id: "sku".into(),
            sku_revision_id: "revision".into(),
            sku_code: "001".into(),
            name: "系统商品名".into(),
            specification: "系统规格".into(),
            unit_code: "BOX".into(),
            quantity_scale: 0,
            fulfillment_kind: InquiryFulfillmentKind::Physical,
        }
    }

    fn delivery(row_number: u32, quantity: &str, due: &str) -> InquiryImportDelivery {
        InquiryImportDelivery {
            row_number,
            sku_code: "001".into(),
            sku_is_text: true,
            recipient: "客户".into(),
            phone: "02100123456".into(),
            phone_is_text: true,
            province: "上海市".into(),
            city: "上海市".into(),
            district: "黄浦区".into(),
            address: "客户地址 1 号".into(),
            quantity: quantity.into(),
            due_date: due.into(),
            terms: "送货上门".into(),
        }
    }

    fn validate(
        products: &[InquiryImportProduct],
        deliveries: &[InquiryImportDelivery],
        current: &InquiryRequirementData,
    ) -> InquiryImportValidation {
        let mut next = 0;
        InquiryImportValidation::build(products, deliveries, &[sku()], current, || {
            next += 1;
            format!("id-{next}")
        })
    }

    #[test]
    fn duplicates_are_explicitly_merged_and_distinct_delivery_dates_keep_line_identity() {
        let rows =
            [delivery(2, "1", "2026-11-01"), delivery(3, "1", "2026-11-01"), delivery(4, "1", "2026-11-02")];
        let result = validate(&[product()], &rows, &InquiryRequirementData::default());
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.data.lines.len(), 2);
        assert_eq!(result.merges[0].row_numbers, vec![2, 3]);
        assert_eq!(result.merges[0].merged_quantity, "2");
        assert_eq!(result.data.lines[0].quantity.to_string(), "2");
        assert_eq!(result.data.lines[0].product_name, "系统商品名");
        assert_eq!(result.data.lines[0].allocations[0].phone, "02100123456");
        assert_ne!(result.data.lines[0].line_id, result.data.lines[1].line_id);
        let again = validate(&[product()], &rows, &result.data);
        assert_eq!(again.data, result.data);
    }

    #[test]
    fn errors_locate_unknown_units_precision_regions_and_totals_without_silent_acceptance() {
        let mut product = product();
        product.unit = "OTHER".into();
        let mut bad = delivery(9, "1.1", "2026-11-01");
        bad.phone_is_text = false;
        let result = validate(&[product], &[bad], &InquiryRequirementData::default());
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.sheet == "商品清单" && error.row_number == 2 && error.column == "单位")
        );
        assert!(
            result.errors.iter().any(|error| error.sheet == "配送需求"
                && error.row_number == 9
                && error.column == "配送数量")
        );
        assert!(result.errors.iter().any(|error| error.column == "联系电话"));
        assert!(!result.errors.iter().any(|error| error.column == "需求数量"));
        assert!(resolve_region("", "", "鼓楼区").is_err());
        assert!(resolve_region("北京市", "北京市", "黄浦区").is_err());
    }

    #[test]
    fn region_resolution_accepts_names_and_codes_and_reports_missing_matches() {
        let expected = RegionCode::parse_leaf("310115").unwrap();
        assert_eq!(resolve_region("上海市", "上海市", "浦东新区").unwrap(), expected);
        assert_eq!(resolve_region("310000", "310100", "310115").unwrap(), expected);
        assert_eq!(
            resolve_region("北京市", "北京市", "浦东新区").unwrap_err(),
            "省、市、区县无法唯一匹配，请填写完整标准名称或地区代码"
        );
    }

    #[test]
    fn invalid_sales_prices_identify_the_exported_price_column() {
        for (value, message) in [("invalid", "销售价格式无效，最多 4 位小数"), ("-1", "销售价不得为负数")]
        {
            let mut row = product();
            row.sales_reference_price = value.into();
            let result =
                validate(&[row], &[delivery(2, "3", "2026-11-01")], &InquiryRequirementData::default());
            assert!(result.data.lines.is_empty());
            let issue = result.errors.iter().find(|issue| issue.column == "销售价").unwrap();
            assert_eq!(issue.sheet, "商品清单");
            assert_eq!(issue.row_number, 2);
            assert_eq!(issue.message, message);
        }
    }

    #[test]
    fn unknown_and_unselectable_products_share_one_error_semantics() {
        let result = InquiryImportValidation::build(
            &[product()],
            &[],
            &[],
            &InquiryRequirementData::default(),
            || "id".into(),
        );
        assert!(result.errors.iter().any(|error| error.message == "商品不存在或不可选用"));
        assert!(result.data.lines.is_empty());
    }

    #[test]
    fn missing_phone_and_date_report_root_causes_without_cascading_totals_or_empty_demand() {
        let mut row = delivery(2, "3", "");
        row.phone.clear();
        row.phone_is_text = false;
        let result = validate(&[product()], &[row], &InquiryRequirementData::default());
        assert_eq!(result.errors.len(), 2, "{:?}", result.errors);
        assert_eq!(result.errors[0].message, "请填写联系电话");
        assert_eq!(result.errors[1].message, "请填写要求交付日期");
        assert!(result.data.lines.is_empty());
    }

    #[test]
    fn missing_phone_or_invalid_address_does_not_duplicate_errors_or_report_partial_totals() {
        let mut missing_phone = delivery(2, "3", "2026-11-01");
        missing_phone.phone.clear();
        let result = validate(&[product()], &[missing_phone], &InquiryRequirementData::default());
        assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
        assert_eq!(result.errors[0].message, "请填写联系电话");
        let mut missing_address = delivery(2, "3", "2026-11-01");
        missing_address.address.clear();
        let result = validate(&[product()], &[missing_address], &InquiryRequirementData::default());
        assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
        assert_eq!(result.errors[0].column, "详细地址");
    }

    #[test]
    fn invalid_dates_do_not_hide_quantity_mismatches_for_other_products() {
        let mut other_product = product();
        other_product.row_number = 3;
        other_product.sku_code = "002".into();
        let mut other_sku = sku();
        other_sku.sku_code = "002".into();
        other_sku.sku_id = "other-sku".into();
        let mut other_delivery = delivery(3, "2", "2026-11-01");
        other_delivery.sku_code = "002".into();
        let result = InquiryImportValidation::build(
            &[product(), other_product],
            &[delivery(2, "3", "invalid"), other_delivery],
            &[sku(), other_sku],
            &InquiryRequirementData::default(),
            || "id".into(),
        );
        assert_eq!(result.errors.len(), 2, "{:?}", result.errors);
        assert_eq!(result.errors[0].column, "要求交付日期");
        assert_eq!(result.errors[1].column, "需求数量");
        assert_eq!(result.errors[1].row_number, 3);
    }

    #[test]
    fn real_quantity_mismatches_and_missing_deliveries_remain_blocking() {
        for rows in [vec![], vec![delivery(2, "2", "2026-11-01")], vec![delivery(2, "4", "2026-11-01")]] {
            let result = validate(&[product()], &rows, &InquiryRequirementData::default());
            assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
            assert_eq!(result.errors[0].column, "需求数量");
        }
    }

    #[test]
    fn nonphysical_rows_require_explicit_due_date_but_allow_empty_shipping_address() {
        let mut sku = sku();
        sku.fulfillment_kind = InquiryFulfillmentKind::Virtual;
        let mut row = delivery(2, "3", "2026-11-01");
        row.recipient.clear();
        row.phone.clear();
        row.phone_is_text = false;
        row.province.clear();
        row.city.clear();
        row.district.clear();
        row.address.clear();
        let result = InquiryImportValidation::build(
            &[product()],
            &[row],
            &[sku],
            &InquiryRequirementData::default(),
            || "new-line".into(),
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.data.lines[0].quantity.to_string(), "3");
        assert!(result.data.lines[0].allocations.is_empty());
        assert_eq!(result.data.lines[0].delivery_due_at, due_instant("2026-11-01").unwrap());
    }

    #[test]
    fn more_than_one_hundred_addresses_are_rejected_instead_of_truncated() {
        let mut product = product();
        product.quantity = "101".into();
        let rows = (0..101)
            .map(|index| {
                let mut row = delivery(index + 2, "1", "2026-11-01");
                row.address = format!("地址 {index}");
                row
            })
            .collect::<Vec<_>>();
        let result = validate(&[product], &rows, &InquiryRequirementData::default());
        assert_eq!(result.data.lines[0].allocations.len(), 101);
        assert!(result.errors.iter().any(|error| error.message.contains("100 个收货地址")));
    }

    fn preview() -> InquiryImportPreview {
        InquiryImportPreview {
            base: BaseModel::new("preview".into()),
            actor_id: "actor".into(),
            creation_customer_id: None,
            inquiry_id: "inquiry".into(),
            expected_version: 1,
            input_fingerprint: "a".repeat(64),
            errors: vec![],
            merges: vec![InquiryImportMerge {
                sheet: "配送需求".into(),
                row_numbers: vec![2, 3],
                sku_code: "001".into(),
                merged_quantity: "2".into(),
            }],
            data: InquiryRequirementData::default(),
            expires_at: 200,
            result: None,
        }
    }

    #[test]
    fn confirmation_rejects_other_actor_version_input_errors_and_unconfirmed_merges() {
        let mut preview = preview();
        assert!(preview.ensure_confirmable("actor", "inquiry", 1, &"a".repeat(64), true, 100).is_ok());
        assert!(preview.ensure_confirmable("other", "inquiry", 1, &"a".repeat(64), true, 100).is_err());
        assert!(preview.ensure_confirmable("actor", "other", 1, &"a".repeat(64), true, 100).is_err());
        assert!(preview.ensure_confirmable("actor", "inquiry", 2, &"a".repeat(64), true, 100).is_err());
        assert!(preview.ensure_confirmable("actor", "inquiry", 1, &"b".repeat(64), true, 100).is_err());
        assert!(preview.ensure_confirmable("actor", "inquiry", 1, &"a".repeat(64), false, 100).is_err());
        assert!(preview.ensure_confirmable("actor", "inquiry", 1, &"a".repeat(64), true, 200).is_err());
        push_issue(&mut preview.errors, "配送需求", 2, "配送数量", "数量非法");
        assert!(preview.ensure_confirmable("actor", "inquiry", 1, &"a".repeat(64), true, 100).is_err());
    }

    #[test]
    fn permanent_receipt_recovers_original_result_and_rejects_other_payload_or_scope() {
        let result = InquiryImportResult {
            inquiry_id: "inquiry".into(),
            preview_id: "preview".into(),
            version: 7,
            line_count: 2,
            allocation_count: 3,
        };
        let receipt = InquiryImportReceipt {
            base: BaseModel::new("command".into()),
            actor_id: "actor".into(),
            inquiry_id: "inquiry".into(),
            payload_fingerprint: "a".repeat(64),
            result: result.clone(),
        };
        assert_eq!(
            receipt.recover("command", "actor", "inquiry", "preview", &"a".repeat(64)).unwrap(),
            result
        );
        assert!(receipt.recover("command", "actor", "inquiry", "preview", &"b".repeat(64)).is_err());
        assert!(receipt.recover("command", "actor", "other-inquiry", "preview", &"a".repeat(64)).is_err());
        assert!(receipt.recover("command", "other-actor", "inquiry", "preview", &"a".repeat(64)).is_err());
        let mut invalid = receipt;
        invalid.result.inquiry_id = "other-inquiry".into();
        assert!(invalid.recover("command", "actor", "inquiry", "preview", &"a".repeat(64)).is_err());
        invalid.result.inquiry_id = "inquiry".into();
        invalid.base.deleted_at = 1;
        assert!(invalid.recover("command", "actor", "inquiry", "preview", &"a".repeat(64)).is_err());
    }

    #[test]
    fn creation_preview_binds_customer_actor_input_and_single_use() {
        let mut preview = preview();
        let fingerprint = "a".repeat(64);
        assert!(preview.ensure_creatable("actor", "customer", &fingerprint, true, 100).is_err());
        preview.creation_customer_id = Some("customer".into());
        preview.inquiry_id.clear();
        preview.expected_version = 0;
        assert!(preview.ensure_creatable("actor", "customer", &fingerprint, true, 100).is_ok());
        assert!(preview.ensure_creatable("actor", "other", &fingerprint, true, 100).is_err());
        assert!(preview.ensure_creatable("other", "customer", &fingerprint, true, 100).is_err());
        assert!(preview.ensure_creatable("actor", "customer", &"b".repeat(64), true, 100).is_err());
        assert!(preview.ensure_creatable("actor", "customer", &fingerprint, false, 100).is_err());
        assert!(preview.ensure_creatable("actor", "customer", &fingerprint, true, 200).is_err());
        assert!(preview.ensure_confirmable("actor", "inquiry", 1, &fingerprint, true, 100).is_err());
        preview.result = Some(InquiryImportResult {
            inquiry_id: "created".into(),
            preview_id: preview.base.id.clone(),
            version: 1,
            line_count: 1,
            allocation_count: 1,
        });
        assert!(preview.ensure_creatable("actor", "customer", &fingerprint, true, 100).is_err());
        preview.result = None;
        push_issue(&mut preview.errors, "配送需求", 2, "配送数量", "数量非法");
        assert!(preview.ensure_creatable("actor", "customer", &fingerprint, true, 100).is_err());
    }

    #[test]
    fn existing_preview_without_creation_customer_stays_an_existing_inquiry_import() {
        let value = serde_json::to_value(preview()).unwrap();
        assert!(value.get("creation_customer_id").is_none());
        let loaded: InquiryImportPreview = serde_json::from_value(value).unwrap();
        assert!(loaded.creation_customer_id.is_none());
        assert!(loaded.ensure_confirmable("actor", "inquiry", 1, &"a".repeat(64), true, 100).is_ok());
        assert!(loaded.ensure_creatable("actor", "customer", &"a".repeat(64), true, 100).is_err());
    }
}
