use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

/// 可同步的展示字段；规格、单位和销售价格不属于本合同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceField {
    Name,
    Description,
    SkuName,
    MainImage,
    CarouselMedia,
    DetailMedia,
}

/// 具有来源证据及可选归档资产的有序图片项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceImage {
    pub url: Option<String>,
    pub file_asset_id: Option<String>,
    pub content_fingerprint: Option<String>,
}

/// 展示资料值。图片序列的顺序属于内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceValue {
    Text(Option<String>),
    Images(Vec<SourceImage>),
}

/// 一个字段已处理的来源基线和人工维护责任。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFieldState {
    pub field: SourceField,
    #[serde(default)]
    pub sku_id: Option<String>,
    pub baseline: SourceValue,
    #[serde(default)]
    pub baseline_confirmed: bool,
    pub latest: SourceValue,
    pub company: SourceValue,
    pub manually_maintained: bool,
    pub pending: bool,
}

/// 一个公司商品唯一的资料同步来源，以商品 ID 作为稳定身份。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ProductSource {
    #[serde(flatten)]
    pub base: BaseModel,
    pub product_id: String,
    pub supplier_id: String,
    pub supplier_name: Option<String>,
    pub connection_id: String,
    pub catalog_item_id: String,
    pub source_product_id: String,
    pub sku_sources: Vec<SourceSkuBinding>,
    pub observed_at: u64,
    pub fields: Vec<SourceFieldState>,
    pub pending_count: u32,
    pub pending_fields: Vec<SourceField>,
}

/// 一次来源观察中明确提供的字段；缺席字段不表示清空。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceObservation {
    pub values: Vec<(SourceField, Option<String>, SourceValue)>,
    pub observed_at: u64,
}

/// 用户对差异的选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceDecisionAction {
    Adopt,
    Keep,
    Edit,
}

/// 单字段差异处理命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDecision {
    pub field: SourceField,
    #[serde(default)]
    pub sku_id: Option<String>,
    pub action: SourceDecisionAction,
    pub value: Option<SourceValue>,
}

/// 同时锁定已比较的来源与公司版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveSourceDiffRequest {
    pub idempotency_key: String,
    pub expected_product_version: u64,
    pub expected_source_version: u64,
    #[serde(default)]
    pub expected_sku_versions: std::collections::BTreeMap<String, u64>,
    pub decisions: Vec<SourceDecision>,
}

/// 对比界面的来源身份，不含采购成本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductSourceIdentity {
    pub supplier_id: String,
    pub supplier_name: Option<String>,
    pub connection_id: String,
    pub catalog_item_id: String,
}

/// 一个待确认字段的真实三方值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDiffField {
    pub field: SourceField,
    #[serde(default)]
    pub sku_id: Option<String>,
    pub label: String,
    pub sku_no: Option<String>,
    pub company: SourceValue,
    pub supplier: SourceValue,
    pub baseline: SourceValue,
    pub manually_maintained: bool,
    pub verification_required: bool,
    pub application_blocker: Option<String>,
}

/// 商品详情的差异快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductSourceDiffView {
    pub product_id: String,
    pub product_version: u64,
    pub source_version: u64,
    pub sku_versions: std::collections::BTreeMap<String, u64>,
    pub source: ProductSourceIdentity,
    pub observed_at: u64,
    pub fields: Vec<SourceDiffField>,
    pub pending_count: u32,
}

/// 确认回执与业务状态同事务写入，重复操作不会覆盖后续修改。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SourceResolutionReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub product_id: String,
    pub request: ResolveSourceDiffRequest,
    pub product_version: u64,
    pub source_version: u64,
    pub result: ProductSourceDiffView,
}

/// 同一来源商品下不同规格的稳定绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSkuBinding {
    pub catalog_item_id: String,
    pub sku_id: String,
}

/// 公司当前展示字段及各SKU乐观锁版本。
#[derive(Debug, Clone, Default)]
pub struct CompanySourceContent {
    pub values: Vec<(SourceField, Option<String>, SourceValue)>,
    pub sku_versions: std::collections::BTreeMap<String, u64>,
    pub sku_numbers: std::collections::BTreeMap<String, String>,
}
