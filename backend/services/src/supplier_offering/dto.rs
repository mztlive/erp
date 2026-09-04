//! 供应商供给 HTTP DTO；Handler 直接复用本文件类型.

use entities::common::time::Instant;
use entities::ids::{SkuId, SupplierAccountId, SupplierApiConnectionId, SupplierOfferingId};
use entities::money::Quantity;
use entities::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, OfferingStatus, PrefillSourceRefs,
    SupplierOfferingAvailabilityData, SupplierOfferingData, SupplierOfferingRevisionData,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::errors::{Error, Result};

pub(crate) const OFFERING_SORT_FIELDS: &[&str] = &["created_at", "supplier_sku_code", "status"];

/// 新增供给命令的稳定操作名。
pub(crate) const CREATE_OFFERING_COMMAND: &str = "create_offering";
/// 追加供给商业条款修订命令的稳定操作名。
pub(crate) const REVISE_OFFERING_COMMAND: &str = "revise_offering";
/// 更新实时可供状态命令的稳定操作名。
pub(crate) const UPDATE_OFFERING_AVAILABILITY_COMMAND: &str = "update_offering_availability";

/// 分页响应。
pub use crate::query::PageView;

/// 排序方向。
pub(crate) use crate::query::SortDir;

/// 规范化并校验排序字段与方向。
///
/// # 参数
/// * `sort_by` - 排序字段
/// * `sort_dir` - 排序方向
/// * `allowed_fields` - 字段白名单
///
/// # 返回
/// 返回规范化排序字段与方向。
///
/// # 错误
/// 字段或方向不在合同范围时返回错误。
pub(crate) use crate::query::normalize_sort;

use crate::query::non_blank;

/// 供给列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierOfferingListParams {
    /// 关键字：供应商订货编码、公司 SKU 编号或 SKU 名称。
    pub q: Option<String>,
    /// 公司 SKU。
    pub sku_id: Option<String>,
    /// 公司 SKU 编号筛选（模糊、忽略大小写）。
    pub sku_no: Option<String>,
    /// 公司商品（SPU）编号筛选（模糊、忽略大小写）。
    pub product_no: Option<String>,
    /// 供应商。
    pub supplier_id: Option<String>,
    /// 供给关系状态。
    pub status: Option<OfferingStatus>,
    /// 登记来源。
    pub source_type: Option<OfferingSourceType>,
    /// 当前可供状态。
    pub availability_status: Option<AvailabilityStatus>,
    /// 页码。
    #[validate(range(min = 1, message = "页码必须大于 0"))]
    pub page: Option<u64>,
    /// 每页数量。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在 1-100 之间"))]
    pub page_size: Option<u32>,
    /// 排序字段。
    pub sort_by: Option<String>,
    /// 排序方向。
    pub sort_dir: Option<String>,
}

/// 供给列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOfferingView {
    /// 供给主键。
    pub id: String,
    /// 公司 SKU。
    pub sku_id: String,
    /// 公司 SKU 编号。
    pub sku_no: Option<String>,
    /// 公司商品编号。
    pub product_no: Option<String>,
    /// 公司 SKU 名称。
    pub sku_name: Option<String>,
    /// 公司 SKU 规格。
    pub specification: Option<String>,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商编号。
    pub supplier_no: Option<String>,
    /// 供应商名称。
    pub supplier_name: Option<String>,
    /// 供应商侧商品编码。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接。
    pub source_connection_id: Option<String>,
    /// 供给关系状态。
    pub status: OfferingStatus,
    /// 当前商业条款修订。
    pub current_revision_id: Option<String>,
    /// 当前修订号。
    pub current_revision_no: Option<u32>,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: Option<String>,
    /// 一件代发不含税价。
    pub dropship_supply_price_net: Option<String>,
    /// 集采含税价。
    pub bulk_supply_price_gross: Option<String>,
    /// 集采不含税价。
    pub bulk_supply_price_net: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 可供区域。
    pub supply_region: Vec<String>,
    /// 商品级能力。
    pub product_capabilities: Vec<String>,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<String>,
    /// 服务费。
    pub service_fee_amount: Option<String>,
    /// 生效日期。
    pub valid_from: Option<String>,
    /// 失效日期。
    pub valid_to: Option<String>,
    /// 当前可供状态。
    pub availability_status: Option<AvailabilityStatus>,
    /// 当前可供数量。
    pub available_quantity: Option<String>,
    /// 可供来源更新时间。
    pub availability_source_updated_at: Option<i64>,
    /// 可供投影版本。
    pub availability_version: Option<u64>,
    /// 供给乐观锁版本。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
}

impl SupplierOfferingView {
    /// 清除采购成本、税率和费用字段。
    pub fn redact_costs(&mut self) {
        self.dropship_supply_price_gross = None;
        self.dropship_supply_price_net = None;
        self.bulk_supply_price_gross = None;
        self.bulk_supply_price_net = None;
        self.input_tax_rate = None;
        self.freight_amount = None;
        self.service_fee_amount = None;
    }
}

/// 供给商业条款公共写入字段。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierOfferingTermsWrite {
    /// 一件代发含税价。
    #[validate(custom(function = "non_blank", message = "一件代发供给价不能为空"))]
    pub dropship_supply_price_gross: String,
    /// 集采含税价。
    #[validate(custom(function = "non_blank", message = "集采供给价不能为空"))]
    pub bulk_supply_price_gross: String,
    /// 进项税率。
    #[validate(custom(function = "non_blank", message = "进项税率不能为空"))]
    pub input_tax_rate: String,
    /// 集采起订量。
    #[validate(custom(function = "non_blank", message = "集采起订量不能为空"))]
    pub bulk_minimum_order_quantity: String,
    /// 可供区域。
    #[validate(length(min = 1, message = "可供区域不能为空"))]
    pub supply_region: Vec<String>,
    /// 商品级能力。
    #[serde(default)]
    pub product_capabilities: Vec<String>,
    /// 生效日期。
    #[validate(custom(function = "non_blank", message = "有效期开始不能为空"))]
    pub valid_from: String,
    /// 失效日期。
    pub valid_to: Option<String>,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<String>,
    /// 服务费。
    pub service_fee_amount: Option<String>,
}

/// 新增供给请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierOfferingRequest {
    /// 公司 SKU。
    #[validate(custom(function = "non_blank", message = "公司 SKU 不能为空"))]
    pub sku_id: String,
    /// 供应商。
    #[validate(custom(function = "non_blank", message = "供应商不能为空"))]
    pub supplier_id: String,
    /// 供应商侧商品编码。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    #[validate(custom(function = "non_blank", message = "供应商 SKU 编码不能为空"))]
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接。
    pub source_connection_id: Option<String>,
    /// 首版商业条款。
    #[validate(nested)]
    pub terms: SupplierOfferingTermsWrite,
    /// 初始可供状态。
    pub availability_status: AvailabilityStatus,
    /// 初始可供数量。
    pub available_quantity: Option<String>,
    /// 来源更新时间；空表示服务端接收时间。
    pub source_updated_at: Option<i64>,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "登记原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CreateSupplierOfferingRequest {
    /// 计算新增供给命令的稳定指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回对 `(操作名, 请求体)` 元组 JSON 进行 SHA-256 后的 64 位十六进制指纹，与存量命令格式一致。
    ///
    /// # 错误
    /// JSON 序列化失败时返回 `Internal` 错误。
    ///
    /// # 约束
    /// 输入为 `&self`，不触及外部 I/O；序列化形态与历史实现字节一致，存量幂等键可继续重放。
    pub(crate) fn command_fingerprint(&self) -> Result<String> {
        let payload = serde_json::to_vec(&(CREATE_OFFERING_COMMAND, self))
            .map_err(|error| Error::Internal(format!("序列化供给命令指纹失败: {error}")))?;
        Ok(hex::encode(Sha256::digest(payload)))
    }
}

/// 新增供给结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateSupplierOfferingResult {
    /// 供给主键。
    pub offering_id: String,
    /// 首版商业条款主键。
    pub revision_id: String,
    /// 实时可供投影主键。
    pub availability_id: String,
    /// 修订号。
    pub revision_no: u32,
    /// 供给状态。
    pub status: OfferingStatus,
}

/// 保存供给商业条款修订请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReviseSupplierOfferingRequest {
    /// 期望当前修订号。
    #[validate(range(min = 1, message = "期望供给修订号必须大于 0"))]
    pub expected_revision_no: u32,
    /// 新商业条款。
    #[validate(nested)]
    pub terms: SupplierOfferingTermsWrite,
    /// 可选的新供给关系状态。
    pub status: Option<OfferingStatus>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl ReviseSupplierOfferingRequest {
    /// 计算修订供给命令的稳定指纹。
    ///
    /// # 参数
    /// * `offering_id` - 命令目标供给主键
    ///
    /// # 返回
    /// 返回对 `(操作名, (目标供给, 请求体))` 元组 JSON 进行 SHA-256 后的 64 位十六进制指纹，
    /// 与存量命令格式一致。
    ///
    /// # 错误
    /// JSON 序列化失败时返回 `Internal` 错误。
    ///
    /// # 约束
    /// 输入为 `&self` 与目标供给 ID，不触及外部 I/O；序列化形态与历史实现字节一致，
    /// 存量幂等键可继续重放。
    pub(crate) fn command_fingerprint(&self, offering_id: &str) -> Result<String> {
        let payload = serde_json::to_vec(&(REVISE_OFFERING_COMMAND, (offering_id, self)))
            .map_err(|error| Error::Internal(format!("序列化供给命令指纹失败: {error}")))?;
        Ok(hex::encode(Sha256::digest(payload)))
    }
}

/// 保存供给商业条款结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviseSupplierOfferingResult {
    /// 供给主键。
    pub offering_id: String,
    /// 新修订主键。
    pub revision_id: String,
    /// 新修订号。
    pub revision_no: u32,
    /// 供给关系状态。
    pub status: OfferingStatus,
    /// 供给乐观锁版本。
    pub version: u64,
    /// 发布侧影响已移除，恒为空。
    pub safety_pause: Option<serde_json::Value>,
}

/// 更新实时可供状态请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierOfferingAvailabilityRequest {
    /// 可选期望投影版本；人工编辑时用于并发保护。
    pub expected_version: Option<u64>,
    /// 当前可供状态。
    pub availability_status: AvailabilityStatus,
    /// 当前可供数量。
    pub available_quantity: Option<String>,
    /// 来源更新时间；空表示服务端接收时间。
    pub source_updated_at: Option<i64>,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl UpdateSupplierOfferingAvailabilityRequest {
    /// 计算更新可供状态命令的稳定指纹。
    ///
    /// # 参数
    /// * `offering_id` - 命令目标供给主键
    ///
    /// # 返回
    /// 返回对 `(操作名, (目标供给, 请求体))` 元组 JSON 进行 SHA-256 后的 64 位十六进制指纹，
    /// 与存量命令格式一致。
    ///
    /// # 错误
    /// JSON 序列化失败时返回 `Internal` 错误。
    ///
    /// # 约束
    /// 输入为 `&self` 与目标供给 ID，不触及外部 I/O；序列化形态与历史实现字节一致，
    /// 存量幂等键可继续重放。
    pub(crate) fn command_fingerprint(&self, offering_id: &str) -> Result<String> {
        let payload = serde_json::to_vec(&(UPDATE_OFFERING_AVAILABILITY_COMMAND, (offering_id, self)))
            .map_err(|error| Error::Internal(format!("序列化供给命令指纹失败: {error}")))?;
        Ok(hex::encode(Sha256::digest(payload)))
    }
}

/// 更新实时可供状态结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateSupplierOfferingAvailabilityResult {
    /// 供给主键。
    pub offering_id: String,
    /// 当前可供状态。
    pub availability_status: AvailabilityStatus,
    /// 可供投影版本。
    pub availability_version: u64,
    /// 来源更新时间。
    pub source_updated_at: i64,
    /// 发布侧影响已移除，恒为空。
    pub safety_pause: Option<serde_json::Value>,
}

/// 供应停止后续任务的固定决定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierSupplyExceptionDecisionType {
    /// 确认停供来源与安全暂停影响已经核对，安全暂停继续生效。
    AcknowledgeSafetyPause,
}

/// 供应停止后续任务的强类型决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSupplyExceptionDecision {
    /// 固定决定类型。
    #[serde(rename = "type")]
    pub decision_type: SupplierSupplyExceptionDecisionType,
    /// 任务绑定的供应商供给。
    #[validate(length(min = 1, max = 128, message = "供给ID不能为空或过长"))]
    pub offering_id: String,
    /// 已核对处置的外部或内部证据引用。
    #[validate(
        length(min = 1, max = 256, message = "证据引用不能为空或过长"),
        custom(function = "non_blank", message = "证据引用不能为空")
    )]
    pub evidence_reference: String,
    /// 核对结论；不得表达为恢复供给或恢复发布。
    #[validate(
        length(min = 1, max = 500, message = "核对结论不能为空或过长"),
        custom(function = "non_blank", message = "核对结论不能为空")
    )]
    pub comment: String,
}

/// 完成供应停止后续任务请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompleteSupplierSupplyExceptionTaskRequest {
    /// 当前正式工作项。
    #[validate(length(min = 1, max = 128, message = "任务ID不能为空或过长"))]
    pub work_item_id: String,
    /// 当前工作项版本。
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    /// 工作项冻结的安全暂停来源版本。
    #[validate(length(min = 1, max = 128, message = "来源版本不能为空或过长"))]
    pub expected_subject_version: String,
    /// 固定强类型决定。
    #[validate(nested)]
    pub decision: SupplierSupplyExceptionDecision,
    /// 客户端操作号。
    #[validate(
        length(min = 1, max = 128, message = "操作号不能为空或过长"),
        custom(function = "non_blank", message = "操作号不能为空")
    )]
    pub idempotency_key: String,
}

/// 完成供应停止后续任务结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteSupplierSupplyExceptionTaskResult {
    /// 已完成任务。
    pub work_item_id: String,
    /// 关联任务标识。
    pub safety_pause_operation_id: String,
    /// 本次核对证据引用。
    pub evidence_reference: String,
    /// 固定结果说明。
    pub message: String,
}

/// 供给列表查询参数的类型化 ID 转换与写入 DTO 的领域数据转换。
///
/// 本扩展把字符串规整（去空白、空值缺省）与数值/日期解析收敛在 DTO 层，
/// 实体层 `supplier_offering::write_data` 负责无 I/O 的解析边界；
/// Service 只分配 ID、计算时钟时间戳、执行资格检查并组织事务。
impl SupplierOfferingListParams {
    /// 返回规整后的公司 SKU 主键。
    ///
    /// # 参数
    /// 无，读取 `self.sku_id`。
    ///
    /// # 返回
    /// 去空白后非空时返回类型化主键，否则返回 `None`。
    ///
    /// # 错误
    /// 永不失败；非法形态由后续仓储精确过滤处理。
    ///
    /// # 约束
    /// 纯内存转换，不触碰 I/O、时钟或密钥。
    pub fn typed_sku_id(&self) -> Option<SkuId> {
        typed_id(self.sku_id.as_deref(), SkuId::new)
    }

    /// 返回规整后的供应商主键。
    ///
    /// # 参数
    /// 无，读取 `self.supplier_id`。
    ///
    /// # 返回
    /// 去空白后非空时返回类型化主键，否则返回 `None`。
    ///
    /// # 错误
    /// 永不失败；非法形态由后续仓储精确过滤处理。
    ///
    /// # 约束
    /// 纯内存转换，不触碰 I/O、时钟或密钥。
    pub fn typed_supplier_id(&self) -> Option<SupplierAccountId> {
        typed_id(self.supplier_id.as_deref(), SupplierAccountId::new)
    }
}

/// 规整可选 ID 字符串。
///
/// # 参数
/// * `value` - 原始字符串
/// * `constructor` - 类型化主键构造器
///
/// # 返回
/// 去空白后非空时返回类型化主键，否则返回 `None`。
///
/// # 错误
/// 永不失败。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟或密钥。
fn typed_id<T>(value: Option<&str>, constructor: impl Fn(String) -> T) -> Option<T> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| constructor(value.to_string()))
}

impl SupplierOfferingTermsWrite {
    /// 将写入条款转换为供给修订领域数据。
    ///
    /// # 参数
    /// * `offering_id` - 所属供给主键，由 Service 分配
    /// * `revision_no` - 修订号，首版为 1，后续由领域版本规则推进
    ///
    /// # 返回
    /// 返回含税/不含税换算后的修订数据。
    ///
    /// # 错误
    /// 空白、非法数值、非法日期时返回 `ValidationError`，文案与历史 Service
    /// helper 逐字一致（`非法进项税率/供给价/起订量/金额/业务日期`）。
    ///
    /// # 约束
    /// 纯内存转换；不分配 ID、不读取时钟、不触碰 I/O 或密钥。
    pub fn try_into_revision_data(
        &self,
        offering_id: SupplierOfferingId,
        revision_no: u32,
    ) -> Result<SupplierOfferingRevisionData> {
        use entities::supplier_offering::write_data;
        let rate = write_data::parse_input_tax_rate(&self.input_tax_rate)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let dropship_gross =
            write_data::parse_unit_price(&self.dropship_supply_price_gross, "一件代发供给价")
                .map_err(|error| Error::ValidationError(error.to_string()))?;
        let bulk_gross = write_data::parse_unit_price(&self.bulk_supply_price_gross, "集采供给价")
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let freight = write_data::parse_optional_amount(self.freight_amount.as_deref())
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let service_fee = write_data::parse_optional_amount(self.service_fee_amount.as_deref())
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let moq = write_data::parse_minimum_order_quantity(&self.bulk_minimum_order_quantity)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let valid_from = write_data::parse_business_date(&self.valid_from)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let valid_to = self
            .valid_to
            .as_deref()
            .map(write_data::parse_business_date)
            .transpose()
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        Ok(SupplierOfferingRevisionData::from_gross_prices(
            offering_id,
            revision_no,
            dropship_gross,
            bulk_gross,
            rate,
            self.dropship_express.clone(),
            freight,
            service_fee,
            moq,
            self.supply_region.clone(),
            self.product_capabilities.clone(),
            valid_from,
            valid_to,
            PrefillSourceRefs::default(),
        ))
    }
}

impl CreateSupplierOfferingRequest {
    /// 将创建请求转换为供给稳定身份数据。
    ///
    /// # 参数
    /// 无，读取请求内的 SKU、供应商与来源字段。
    ///
    /// # 返回
    /// 返回规整后的供给身份数据；ID 分配与实体构造由 Service 继续完成。
    ///
    /// # 错误
    /// 当前仅做去空白与空值缺省，不做数值解析，故永不失败；
    /// 编码超长与来源约束由供给实体构造时拒绝。
    ///
    /// # 约束
    /// 纯内存转换，不分配 ID、不读取时钟、不触碰 I/O 或密钥。
    pub fn try_into_offering_data(&self) -> Result<SupplierOfferingData> {
        Ok(SupplierOfferingData {
            sku_id: SkuId::new(self.sku_id.trim().to_string()),
            supplier_id: SupplierAccountId::new(self.supplier_id.trim().to_string()),
            supplier_product_code: self.supplier_product_code.clone(),
            supplier_sku_code: self.supplier_sku_code.clone(),
            source_type: self.source_type,
            source_connection_id: self
                .source_connection_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| SupplierApiConnectionId::new(value.to_string())),
        })
    }

    /// 将创建请求转换为初始可供投影数据。
    ///
    /// # 参数
    /// * `offering_id` - 所属供给主键，由 Service 分配
    /// * `source_updated_at` - 来源更新时间，由 Service 解析或取接收时间
    /// * `received_at` - ERP 接收时间，由 Service 在调用前取时钟
    /// * `updated_by` - 更新人，由 Service 传入审计身份
    ///
    /// # 返回
    /// 返回可供投影数据；实体构造由 Service 继续完成。
    ///
    /// # 错误
    /// 数量非法时返回 `ValidationError`，文案为 `非法数量: {value}`。
    ///
    /// # 约束
    /// 纯内存转换；时钟时间戳由调用方传入，本方法不读取全局时钟。
    pub fn try_into_availability_data(
        &self,
        offering_id: SupplierOfferingId,
        source_updated_at: Instant,
        received_at: Instant,
        updated_by: String,
    ) -> Result<SupplierOfferingAvailabilityData> {
        let available_quantity = parse_availability_quantity(self.available_quantity.as_deref())?;
        Ok(SupplierOfferingAvailabilityData {
            supplier_offering_id: offering_id,
            availability_status: self.availability_status,
            available_quantity,
            source_updated_at,
            received_at,
            source_revision_token: self.source_revision_token.clone(),
            updated_by,
        })
    }
}

impl UpdateSupplierOfferingAvailabilityRequest {
    /// 将更新请求转换为可供投影数据。
    ///
    /// # 参数
    /// * `offering_id` - 所属供给主键，由 Service 从路径解析
    /// * `source_updated_at` - 来源更新时间，由 Service 解析或取接收时间
    /// * `received_at` - ERP 接收时间，由 Service 在调用前取时钟
    /// * `updated_by` - 更新人，由 Service 传入审计身份
    ///
    /// # 返回
    /// 返回可供投影数据；版本校验与时间倒退拒绝由可供实体 `apply` 负责。
    ///
    /// # 错误
    /// 数量非法时返回 `ValidationError`，文案为 `非法数量: {value}`。
    ///
    /// # 约束
    /// 纯内存转换；时钟时间戳由调用方传入，本方法不读取全局时钟。
    pub fn try_into_availability_data(
        &self,
        offering_id: SupplierOfferingId,
        source_updated_at: Instant,
        received_at: Instant,
        updated_by: String,
    ) -> Result<SupplierOfferingAvailabilityData> {
        let available_quantity = parse_availability_quantity(self.available_quantity.as_deref())?;
        Ok(SupplierOfferingAvailabilityData {
            supplier_offering_id: offering_id,
            availability_status: self.availability_status,
            available_quantity,
            source_updated_at,
            received_at,
            source_revision_token: self.source_revision_token.clone(),
            updated_by,
        })
    }
}

/// 解析可供数量字符串。
///
/// # 参数
/// * `value` - 原始数量字符串；`None` 或空白表示缺省
///
/// # 返回
/// 缺省时返回 `None`，否则返回类型化数量。
///
/// # 错误
/// 非法数值时返回 `ValidationError`，文案为 `非法数量: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟或密钥。
fn parse_availability_quantity(value: Option<&str>) -> Result<Option<Quantity>> {
    use entities::supplier_offering::write_data;
    write_data::parse_optional_quantity(value).map_err(|error| Error::ValidationError(error.to_string()))
}

/// 解析来源更新时间，缺省时回退到接收时间。
///
/// # 参数
/// * `requested` - 请求携带的来源 Unix 秒；`None` 表示来源未提供时间
/// * `received_at` - ERP 接收时间，由 Service 在调用前取时钟
///
/// # 返回
/// 来源提供时返回其时间，否则返回接收时间。
///
/// # 错误
/// 永不失败。
///
/// # 约束
/// 纯内存转换；时钟读取由调用方完成，本方法不触碰全局时钟。
pub(crate) fn resolve_source_updated_at(requested: Option<i64>, received_at: Instant) -> Instant {
    requested.map(Instant::from_unix_secs).unwrap_or(received_at)
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, CreateSupplierOfferingRequest, ReviseSupplierOfferingRequest, SortDir,
        SupplierOfferingListParams, SupplierOfferingTermsWrite, SupplierOfferingView,
        UpdateSupplierOfferingAvailabilityRequest, CREATE_OFFERING_COMMAND, OFFERING_SORT_FIELDS,
        REVISE_OFFERING_COMMAND, UPDATE_OFFERING_AVAILABILITY_COMMAND,
    };
    use entities::supplier_offering::{AvailabilityStatus, OfferingSourceType, OfferingStatus};
    use serde::Serialize;

    #[test]
    fn sort_contract_rejects_unknown_fields() {
        assert_eq!(
            normalize_sort(
                &Some("status".to_string()),
                &Some("asc".to_string()),
                OFFERING_SORT_FIELDS
            )
            .unwrap(),
            ("status", SortDir::Asc)
        );
        assert!(normalize_sort(&Some("unsafe".to_string()), &None, OFFERING_SORT_FIELDS).is_err());
    }

    #[test]
    fn list_filter_contract_accepts_source_and_availability() {
        let params: SupplierOfferingListParams = serde_json::from_value(serde_json::json!({
            "source_type": "EXCEL",
            "availability_status": "AVAILABLE"
        }))
        .unwrap();
        assert_eq!(params.source_type, Some(OfferingSourceType::Excel));
        assert_eq!(params.availability_status, Some(AvailabilityStatus::Available));
    }

    #[test]
    fn cost_redaction_keeps_identity_and_availability() {
        let mut view = SupplierOfferingView {
            id: "o1".to_string(),
            sku_id: "s1".to_string(),
            sku_no: Some("SKU-1".to_string()),
            product_no: None,
            sku_name: Some("商品".to_string()),
            specification: None,
            supplier_id: "supplier-1".to_string(),
            supplier_no: None,
            supplier_name: None,
            supplier_product_code: None,
            supplier_sku_code: "S-1".to_string(),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
            status: OfferingStatus::Active,
            current_revision_id: None,
            current_revision_no: None,
            dropship_supply_price_gross: Some("10".to_string()),
            dropship_supply_price_net: Some("9".to_string()),
            bulk_supply_price_gross: Some("8".to_string()),
            bulk_supply_price_net: Some("7".to_string()),
            input_tax_rate: Some("0.13".to_string()),
            bulk_minimum_order_quantity: Some("10".to_string()),
            supply_region: vec![],
            product_capabilities: vec![],
            dropship_express: None,
            freight_amount: Some("1".to_string()),
            service_fee_amount: None,
            valid_from: None,
            valid_to: None,
            availability_status: None,
            available_quantity: Some("5".to_string()),
            availability_source_updated_at: None,
            availability_version: None,
            version: 1,
            created_at: 1,
        };
        view.redact_costs();
        assert!(view.dropship_supply_price_gross.is_none());
        assert_eq!(view.available_quantity.as_deref(), Some("5"));
        assert_eq!(view.supplier_sku_code, "S-1");
    }

    /// 覆盖新增供给命令指纹：同一请求确定稳定，任一载荷字段变化必须产生不同指纹。
    #[test]
    fn create_command_fingerprint_is_deterministic_and_payload_sensitive() {
        let req = create_request();
        let fp1 = req.command_fingerprint().unwrap();
        assert_eq!(fp1, req.command_fingerprint().unwrap());
        assert_eq!(fp1.len(), 64);

        let mut other = req.clone();
        other.supplier_sku_code = "SKU-2".to_string();
        assert_ne!(fp1, other.command_fingerprint().unwrap());

        let mut other_key = req.clone();
        other_key.idempotency_key = "key-2".to_string();
        assert_ne!(fp1, other_key.command_fingerprint().unwrap());
    }

    /// 覆盖修订与更新命令指纹：目标供给或载荷任一变化必须产生不同指纹。
    #[test]
    fn revise_and_update_command_fingerprints_are_target_sensitive() {
        let revise = revise_request();
        let fp1 = revise.command_fingerprint("offering-1").unwrap();
        assert_ne!(fp1, revise.command_fingerprint("offering-2").unwrap());
        let mut other = revise.clone();
        other.expected_revision_no = 3;
        assert_ne!(fp1, other.command_fingerprint("offering-1").unwrap());

        let update = update_request();
        let fp2 = update.command_fingerprint("offering-1").unwrap();
        assert_ne!(fp2, update.command_fingerprint("offering-2").unwrap());
        let mut other_update = update.clone();
        other_update.available_quantity = Some("9".to_string());
        assert_ne!(fp2, other_update.command_fingerprint("offering-1").unwrap());
    }

    /// 覆盖指纹与历史算法字节一致：`(操作名, 请求体)` 元组 JSON 的 SHA-256 裸十六进制。
    #[test]
    fn command_fingerprint_matches_legacy_tuple_serialization() {
        let create = create_request();
        assert_eq!(
            create.command_fingerprint().unwrap(),
            legacy_fingerprint(CREATE_OFFERING_COMMAND, &create)
        );
        let revise = revise_request();
        assert_eq!(
            revise.command_fingerprint("offering-1").unwrap(),
            legacy_fingerprint(REVISE_OFFERING_COMMAND, &("offering-1", &revise))
        );
        let update = update_request();
        assert_eq!(
            update.command_fingerprint("offering-1").unwrap(),
            legacy_fingerprint(UPDATE_OFFERING_AVAILABILITY_COMMAND, &("offering-1", &update))
        );
    }

    /// 历史实现：`serde_json::to_vec(&(operation, request))` 的 SHA-256 裸十六进制。
    fn legacy_fingerprint<T: Serialize>(operation: &str, request: &T) -> String {
        use sha2::{Digest, Sha256};

        let bytes = serde_json::to_vec(&(operation, request)).unwrap();
        hex::encode(Sha256::digest(bytes))
    }

    /// 覆盖指纹金 test：锁定三个操作的稳定字节合同，防止未来漂移破坏存量重放。
    #[test]
    fn command_fingerprint_golden_contracts() {
        assert_eq!(
            create_request().command_fingerprint().unwrap(),
            "b4ffb0315515f864f185763a8629534919751a5bdb898cd241e31036f97d436b"
        );
        assert_eq!(
            revise_request().command_fingerprint("offering-1").unwrap(),
            "bf555bf28973bd5c274b1e8b2aa359bc5bf72f636cd33040614a0db03823a7d1"
        );
        assert_eq!(
            update_request().command_fingerprint("offering-1").unwrap(),
            "b2a0134c236897bfb7c3c0eb73c9ea26d31904cd479bf763490f83a601b77aab"
        );
    }

    fn terms() -> SupplierOfferingTermsWrite {
        SupplierOfferingTermsWrite {
            dropship_supply_price_gross: "10.00".to_string(),
            bulk_supply_price_gross: "9.00".to_string(),
            input_tax_rate: "0.13".to_string(),
            bulk_minimum_order_quantity: "10".to_string(),
            supply_region: vec!["CN".to_string()],
            product_capabilities: vec!["DROP_SHIP".to_string()],
            valid_from: "2026-01-01".to_string(),
            valid_to: None,
            dropship_express: Some("顺丰".to_string()),
            freight_amount: None,
            service_fee_amount: None,
        }
    }

    fn create_request() -> CreateSupplierOfferingRequest {
        CreateSupplierOfferingRequest {
            sku_id: "sku-1".to_string(),
            supplier_id: "supplier-1".to_string(),
            supplier_product_code: Some("P-1".to_string()),
            supplier_sku_code: "SKU-1".to_string(),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
            terms: terms(),
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some("100".to_string()),
            source_updated_at: Some(1_700_000_000),
            source_revision_token: None,
            change_reason: "登记新供给".to_string(),
            idempotency_key: "key-1".to_string(),
        }
    }

    fn revise_request() -> ReviseSupplierOfferingRequest {
        ReviseSupplierOfferingRequest {
            expected_revision_no: 2,
            terms: terms(),
            status: Some(OfferingStatus::Active),
            change_reason: "调整条款".to_string(),
            idempotency_key: "key-1".to_string(),
        }
    }

    fn update_request() -> UpdateSupplierOfferingAvailabilityRequest {
        UpdateSupplierOfferingAvailabilityRequest {
            expected_version: Some(1),
            availability_status: AvailabilityStatus::Unavailable,
            available_quantity: Some("0".to_string()),
            source_updated_at: Some(1_700_000_001),
            source_revision_token: Some("token-1".to_string()),
            change_reason: "库存更新".to_string(),
            idempotency_key: "key-1".to_string(),
        }
    }

    #[test]
    fn typed_list_ids_trim_and_omit_blank() {
        let params = SupplierOfferingListParams {
            q: None,
            sku_id: Some("  ".to_string()),
            sku_no: None,
            product_no: None,
            supplier_id: Some(" supplier-1 ".to_string()),
            status: None,
            source_type: None,
            availability_status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.typed_sku_id().is_none());
        assert_eq!(params.typed_supplier_id().unwrap().to_string(), "supplier-1");
    }

    #[test]
    fn terms_try_into_revision_data_covers_first_and_next_revision() {
        use entities::ids::SupplierOfferingId;
        let req = create_request();
        let first = req
            .terms
            .try_into_revision_data(SupplierOfferingId::new("offering-1"), 1)
            .unwrap();
        assert_eq!(first.revision_no, 1);
        let next = req
            .terms
            .try_into_revision_data(SupplierOfferingId::new("offering-1"), 2)
            .unwrap();
        assert_eq!(next.revision_no, 2);
        assert_eq!(first.valid_from, next.valid_from);
    }

    #[test]
    fn terms_try_into_revision_data_rejects_blank_and_illegal() {
        use entities::ids::SupplierOfferingId;
        let mut bad = terms();
        bad.input_tax_rate = "  ".to_string();
        assert!(bad
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .is_err());
        let mut bad_price = terms();
        bad_price.dropship_supply_price_gross = "abc".to_string();
        let err = bad_price
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap_err();
        assert!(err.to_string().contains("非法一件代发供给价"));
        let mut bad_moq = terms();
        bad_moq.bulk_minimum_order_quantity = "0".to_string();
        assert!(bad_moq
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .is_ok());
        let mut bad_window = terms();
        bad_window.valid_from = "not-a-date".to_string();
        assert!(bad_window
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .is_err());
        let mut bad_amount = terms();
        bad_amount.freight_amount = Some("abc".to_string());
        let err = bad_amount
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap_err();
        assert!(err.to_string().contains("非法金额"));
    }

    #[test]
    fn availability_try_into_data_keeps_nullable_quantity_and_caller_timestamps() {
        use entities::common::time::Instant;
        use entities::ids::SupplierOfferingId;
        let req = create_request();
        let source = Instant::from_unix_secs(1_700_000_000);
        let received = Instant::from_unix_secs(1_700_000_010);
        let data = req
            .try_into_availability_data(
                SupplierOfferingId::new("offering-1"),
                source,
                received,
                "actor-1".to_string(),
            )
            .unwrap();
        assert_eq!(data.source_updated_at, source);
        assert_eq!(data.received_at, received);
        assert!(data.available_quantity.is_some());
        let mut missing = req.clone();
        missing.available_quantity = Some("  ".to_string());
        let data = missing
            .try_into_availability_data(
                SupplierOfferingId::new("offering-1"),
                source,
                received,
                "actor-1".to_string(),
            )
            .unwrap();
        assert!(data.available_quantity.is_none());
        let mut illegal = req.clone();
        illegal.available_quantity = Some("abc".to_string());
        assert!(illegal
            .try_into_availability_data(
                SupplierOfferingId::new("offering-1"),
                source,
                received,
                "actor-1".to_string()
            )
            .is_err());
    }

    #[test]
    fn terms_reject_illegal_valid_to_and_blank_nullable_amounts() {
        use entities::ids::SupplierOfferingId;
        let mut bad_to = terms();
        bad_to.valid_to = Some("abc".to_string());
        let err = bad_to
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap_err();
        assert!(err.to_string().contains("非法业务日期"));
        let mut blank_amounts = terms();
        blank_amounts.freight_amount = Some("  ".to_string());
        blank_amounts.service_fee_amount = Some(String::new());
        let data = blank_amounts
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap();
        assert!(data.freight_amount.is_none());
        assert!(data.service_fee_amount.is_none());
    }

    #[test]
    fn inverted_validity_window_fails_closed_at_entity_construction() {
        use entities::ids::{SupplierOfferingId, SupplierOfferingRevisionId};
        use entities::supplier_offering::SupplierOfferingRevision;
        let mut window = terms();
        window.valid_from = "2026-02-01".to_string();
        window.valid_to = Some("2026-01-01".to_string());
        let data = window
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap();
        let err = SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("r1"), data).unwrap_err();
        assert!(err.to_string().contains("有效期结束必须晚于开始"));
    }

    #[test]
    fn revision_parse_reports_first_error_in_documented_order() {
        use entities::ids::SupplierOfferingId;
        let mut multi = terms();
        multi.input_tax_rate = "abc".to_string();
        multi.dropship_supply_price_gross = "abc".to_string();
        multi.bulk_minimum_order_quantity = "abc".to_string();
        multi.valid_from = "abc".to_string();
        let err = multi
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap_err();
        assert!(err.to_string().contains("非法进项税率"));
        let mut second = terms();
        second.dropship_supply_price_gross = "abc".to_string();
        second.bulk_supply_price_gross = "abc".to_string();
        let err = second
            .try_into_revision_data(SupplierOfferingId::new("o1"), 1)
            .unwrap_err();
        assert!(err.to_string().contains("非法一件代发供给价"));
    }

    #[test]
    fn missing_source_time_falls_back_to_received_time() {
        use entities::common::time::Instant;
        let received = Instant::from_unix_secs(1_700_000_010);
        assert_eq!(super::resolve_source_updated_at(None, received), received);
        assert_eq!(
            super::resolve_source_updated_at(Some(1_700_000_000), received),
            Instant::from_unix_secs(1_700_000_000)
        );
    }
}
