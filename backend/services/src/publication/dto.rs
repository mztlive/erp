//! 域 D26 `publication` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额/数量/税率使用
//! `entities::money` 定点类型，JSON 形态为字符串。

use entities::ids::{FileAssetId, ProductCategoryId, SkuId, SkuRevisionId, SourceSystemId};
use entities::money::{Amount, Quantity, Rate};
use entities::publication::{
    MediaRole, ProductCapability, ProductPublication, ProductPublicationRevision, ProductPublicationStatus,
    PublicationDeliveryStatus, SafetyPauseCause, SafetyPauseFollowUp, SafetyPauseSourceObjectType,
    SaleStatus, SystemSafetyPauseOperation,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{page_or_default, page_size_or_default};

/// 发布列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const PRODUCT_PUBLICATION_SORT_FIELDS: &[&str] = &["created_at", "sku_id", "updated_at"];
/// 发布投递列表允许的排序字段白名单。
pub(crate) const PUBLICATION_DELIVERY_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 name 等需要按「空白视为空」拒绝，落入 HTTP 400）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 商品发布创建请求（稳定发布身份，数据模型 §6.15）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductPublicationRequest {
    /// ERP SKU（D10 `sku`）。
    pub sku_id: SkuId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
}

/// 商品发布更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateProductPublicationRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 发布状态；缺省表示不修改。
    pub status: Option<ProductPublicationStatus>,
    /// 当前商城生效版本；缺省表示不修改。
    pub current_revision_id: Option<String>,
}

/// 发布媒体行请求（提交发布必填至少一张主图，§6.15）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MediaItemRequest {
    /// 受控文件资产（D05 `file_asset`，本域仅引用不校验）。
    pub file_asset_id: FileAssetId,
    /// 媒体角色。
    pub media_role: MediaRole,
    /// 同角色内展示顺序。
    pub sort_no: u32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

/// 形成发布修订请求（不可变版本 + 受控媒体原子写入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductPublicationRevisionRequest {
    /// 发布的商品版本（D10 `sku_revision`）。
    pub sku_revision_id: SkuRevisionId,
    /// 本发布版本唯一固定的供给修订（D24 `supplier_offering_revision`）。
    pub supplier_offering_revision_id: entities::ids::SupplierOfferingRevisionId,
    /// 商城发布类目。
    pub category_id: ProductCategoryId,
    /// 商城展示名称快照。
    #[validate(custom(function = "non_blank", message = "发布名称不能为空"))]
    pub name: String,
    /// 规格快照。
    pub specification: Option<String>,
    /// 商城展示销售说明快照（提交发布必填）。
    #[validate(custom(function = "non_blank", message = "销售说明不能为空"))]
    pub sales_description: String,
    /// 商城端最小购买量，按 `base_unit_code`（必须大于零）。
    pub minimum_purchase_quantity: Quantity,
    /// 含税销售价。
    pub sales_price_gross: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 计量单位代码。
    #[validate(custom(function = "non_blank", message = "计量单位不能为空"))]
    pub base_unit_code: String,
    /// 可销售区域快照。
    pub sales_region: Option<String>,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 商品级能力清单。
    pub product_capabilities: Vec<ProductCapability>,
    /// 生效区间开始（秒级时间戳）。
    #[validate(range(min = 1, message = "生效时间必须大于 0"))]
    pub valid_from: u64,
    /// 生效区间结束（秒级时间戳）；必须晚于 `valid_from`。
    pub valid_to: Option<u64>,
    /// 受控媒体行（至少一张主图）。
    pub media: Vec<MediaItemRequest>,
}

/// 发布投递请求（携带幂等键；`(publication_revision_id, target_mall_id)` 唯一索引承接）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DeliverPublicationRevisionRequest {
    /// 调用方幂等键（重复投递不产生第二笔外部调用与第二份投递记录）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 商品发布响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductPublicationView {
    /// 实体主键。
    pub id: String,
    /// ERP SKU。
    pub sku_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 发布状态。
    pub status: ProductPublicationStatus,
    /// 当前商城生效版本。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发布修订响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductPublicationRevisionView {
    /// 实体主键。
    pub id: String,
    /// 所属稳定发布。
    pub product_publication_id: String,
    /// 修订序号（同一发布内从 1 递增）。
    pub revision_no: u32,
    /// 商城展示名称快照。
    pub name: String,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 含税销售价。
    pub sales_price_gross: Amount,
    /// 生效区间开始（秒级时间戳）。
    pub valid_from: i64,
    /// 生效区间结束（秒级时间戳）。
    pub valid_to: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发布修订媒体响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductPublicationRevisionMediaView {
    /// 实体主键。
    pub id: String,
    /// 所属商城发布版本。
    pub product_publication_revision_id: String,
    /// 受控文件资产。
    pub file_asset_id: String,
    /// 媒体角色。
    pub media_role: MediaRole,
    /// 同角色内展示顺序。
    pub sort_no: u32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

/// 发布投递响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductPublicationDeliveryView {
    /// 实体主键。
    pub id: String,
    /// 所属发布修订。
    pub publication_revision_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 跨全部尝试保持不变的消息键。
    pub message_key: String,
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近真实发送时间。
    pub last_attempt_at: Option<i64>,
    /// 下次受控处理时间。
    pub next_attempt_at: Option<i64>,
    /// 商城确认时间。
    pub mall_ack_at: Option<i64>,
    /// 商城确认版本。
    pub mall_version: Option<String>,
    /// 稳定错误分类。
    pub error_class: Option<entities::integration_ops::ErrorClass>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 脱敏错误摘要。
    pub error_summary: Option<String>,
    /// 原消息信封。
    pub inbox_message_id: Option<String>,
    /// W29 错误对象。
    pub error_task_id: Option<String>,
    /// W29 正式待办。
    pub work_item_id: Option<String>,
    /// 服务端按当前投递事实开放的动作。
    pub allowed_actions: Vec<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投递结果视图（成功/失败均返回消息信封与错误任务 ID）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PublicationDeliveryResultView {
    /// 投递记录 ID。
    pub delivery_id: String,
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 承接本次投递的消息信封 ID（`inbox_message`）。
    pub inbox_message_id: String,
    /// 失败时创建的集成错误任务 ID；成功路径为 `None`。
    pub error_task_id: Option<String>,
    /// 商城确认版本；成功路径有值。
    pub mall_version: Option<String>,
    /// 发布主表乐观锁版本（确认后推进为商城生效）。
    pub publication_version: u64,
}

/// W22 发布投递强类型对象动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublicationDeliveryAction {
    /// 查询原消息的商城最终结果。
    QueryResult,
    /// 沿原消息身份安排受控重试。
    Retry,
    /// 升级为 W29 正式错误对象与待办。
    Escalate,
}

impl PublicationDeliveryAction {
    /// 返回协议稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueryResult => "QUERY_RESULT",
            Self::Retry => "RETRY",
            Self::Escalate => "ESCALATE",
        }
    }
}

/// W22 发布投递强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PublicationDeliveryCommand {
    /// 稳定发布身份。
    #[validate(custom(function = "non_blank", message = "发布ID不能为空"))]
    pub publication_id: String,
    /// 不可变发布修订。
    #[validate(custom(function = "non_blank", message = "发布修订ID不能为空"))]
    pub publication_revision_id: String,
    /// 固定投递身份。
    #[validate(custom(function = "non_blank", message = "投递ID不能为空"))]
    pub delivery_id: String,
    /// 强类型对象动作。
    pub action: PublicationDeliveryAction,
    /// 查询所得投递版本。
    pub expected_object_version: u64,
    /// 调用方幂等请求身份；不会原文写入审计。
    #[validate(length(max = 128, message = "请求ID过长"))]
    #[validate(custom(function = "non_blank", message = "请求ID不能为空"))]
    pub request_id: String,
}

/// W22 投递强动作结果分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublicationDeliveryActionResult {
    /// 商城权威确认。
    Acked,
    /// 商城明确失败，或调用前失败关闭。
    Failed,
    /// 最终结果仍未知。
    StillUnknown,
    /// 已沿原身份排入受控重试。
    RetryScheduled,
    /// 已创建或复用 W29 错误对象与待办。
    Escalated,
}

/// W22 投递对象动作正式结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicationDeliveryActionResultView {
    /// 稳定操作编号。
    pub operation_id: String,
    /// 固定投递 ID。
    pub delivery_id: String,
    /// 动作结果。
    pub result: PublicationDeliveryActionResult,
    /// W29 正式待办。
    pub work_item_id: Option<String>,
    /// W29 错误对象。
    pub error_task_id: Option<String>,
    /// 结果形成时间。
    pub occurred_at: i64,
    /// 服务端下一步。
    pub next_action: Option<String>,
}

/// W22 有界待发送处理请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProcessPublicationDeliveriesRequest {
    /// 单批上限。
    #[validate(range(min = 1, max = 100, message = "单批处理数必须在1-100之间"))]
    pub limit: Option<u32>,
}

/// W22 有界待发送处理结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcessPublicationDeliveriesResult {
    /// 扫描候选数量。
    pub scanned: u32,
    /// 商城确认数量。
    pub acked: u32,
    /// 明确失败数量。
    pub failed: u32,
    /// 结果未知数量。
    pub still_unknown: u32,
    /// CAS 未取得或状态变化数量。
    pub skipped: u32,
    /// 逐项正式结果。
    pub items: Vec<PublicationDeliveryActionResultView>,
}

/// 可信目录/供给服务传给 W22 的系统安全暂停触发。
///
/// 本类型不用于浏览器反序列化；`affected_publication_ids` 刻意不存在，影响集只能
/// 由服务端在调用方事务内按当前在售事实冻结。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SystemSafetyPauseTrigger {
    /// 固定安全原因。
    pub cause: SafetyPauseCause,
    /// 可信来源对象类型。
    pub source_object_type: SafetyPauseSourceObjectType,
    /// 可信来源对象 ID。
    pub source_object_id: String,
    /// 服务端来源事实版本。
    pub source_version: String,
    /// 来源事实发生时间。
    pub occurred_at: entities::common::time::Instant,
    /// 调用链幂等键。
    pub idempotency_key: String,
}

impl SystemSafetyPauseTrigger {
    /// 校验强类型触发边界；未知原因/来源与空身份一律失败关闭。
    ///
    /// # 错误
    /// 触发不是已注册常量或稳定身份为空时返回验证错误。
    pub(crate) fn validate_contract(&self) -> Result<()> {
        if self.cause == SafetyPauseCause::Unknown {
            return Err(Error::ValidationError(
                "UNKNOWN 安全暂停原因未注册，必须失败关闭".to_string(),
            ));
        }
        if self.source_object_type == SafetyPauseSourceObjectType::Unknown {
            return Err(Error::ValidationError(
                "UNKNOWN 安全暂停来源对象未注册，必须失败关闭".to_string(),
            ));
        }
        if self.source_object_id.trim().is_empty()
            || self.source_version.trim().is_empty()
            || self.idempotency_key.trim().is_empty()
        {
            return Err(Error::ValidationError(
                "安全暂停来源对象、来源版本和幂等键均不能为空".to_string(),
            ));
        }
        Ok(())
    }
}

/// 安全暂停子结果中的不可变证据种类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyPauseArtifactKindView {
    /// 本实现固定形成新的暂停发布修订。
    Revision,
}

/// 已冻结的单个发布子结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPauseAffectedPublicationView {
    /// 稳定发布 ID。
    pub publication_id: String,
    /// 证据种类，固定为 `REVISION`。
    pub pause_artifact_kind: SafetyPauseArtifactKindView,
    /// 不可变暂停修订 ID。
    pub pause_revision_id: String,
    /// 指向暂停修订的投递 ID。
    pub delivery_id: String,
}

/// `SUPPLIER_STOPPED` 的唯一正式任务引用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPauseFollowUpWorkItemView {
    /// 任务 ID。
    pub work_item_id: String,
    /// 任务乐观锁版本；按 W22 合同返回稳定字符串。
    pub task_version: String,
    /// 固定任务类型。
    pub work_item_type: String,
    /// 与触发来源一致的业务对象类型。
    pub business_object_type: String,
    /// 与触发来源一致的业务对象 ID。
    pub business_object_id: String,
    /// 任务实际冻结的来源版本。
    pub subject_version: String,
    /// 固定路由 W21 的 handler key。
    pub handler_key: String,
}

/// 非供应停止原因的强类型 blocker。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPauseFollowUpBlockerView {
    /// 稳定 blocker 代码。
    pub code: String,
    /// 业务说明。
    pub message: String,
    /// 不可变证据引用。
    pub evidence_reference: String,
}

/// 已提交操作的互斥后续分支。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum KnownSafetyPauseFollowUpView {
    /// 仅 `SUPPLIER_STOPPED` 可返回。
    WorkItem {
        /// 唯一正式任务。
        follow_up_work_item: SafetyPauseFollowUpWorkItemView,
    },
    /// 所有其它原因只能返回 blocker。
    Blocker {
        /// 强类型后续 blocker。
        follow_up_blocker: SafetyPauseFollowUpBlockerView,
    },
}

/// 已提交或幂等重放的安全暂停事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnownSystemSafetyPauseOperationView {
    /// 不可变操作 ID。
    pub operation_id: String,
    /// 固定安全原因。
    pub cause: SafetyPauseCause,
    /// 来源对象类型。
    pub source_object_type: SafetyPauseSourceObjectType,
    /// 来源对象 ID。
    pub source_object_id: String,
    /// 来源事实版本。
    pub source_version: String,
    /// 本地供给效果，固定为暂停。
    pub availability_effect: String,
    /// 首次冻结的完整非空影响集。
    pub affected_publications: Vec<SafetyPauseAffectedPublicationView>,
    /// 本地提交时间（秒级时间戳）。
    pub committed_at: i64,
    /// 与原因互斥匹配的后续任务或 blocker。
    #[serde(flatten)]
    pub follow_up: KnownSafetyPauseFollowUpView,
}

/// 结果未知时的失败关闭视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnknownSystemSafetyPauseOperationView {
    /// 客户端/调用方可跟踪的操作 ID。
    pub operation_id: String,
    /// 固定安全原因。
    pub cause: SafetyPauseCause,
    /// 来源对象类型。
    pub source_object_type: SafetyPauseSourceObjectType,
    /// 来源对象 ID。
    pub source_object_id: String,
    /// 来源事实版本。
    pub source_version: String,
    /// 原幂等键；只能以该键查询，不得创建新操作。
    pub original_idempotency_key: String,
    /// 固定失败关闭效果。
    pub availability_effect: String,
}

/// W22 系统安全暂停操作唯一响应结构。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result_status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SystemSafetyPauseOperationView {
    /// 首次本地事务提交成功。
    Committed(KnownSystemSafetyPauseOperationView),
    /// 相同来源事件的幂等重放；影响集与首次提交完全相同。
    AlreadySafe(KnownSystemSafetyPauseOperationView),
    /// 提交终态暂时无法确认；不得携带影响集、任务或 blocker。
    Unknown(UnknownSystemSafetyPauseOperationView),
}

impl SystemSafetyPauseOperationView {
    /// 从已落库事实构造首次提交视图。
    pub(crate) fn committed(operation: SystemSafetyPauseOperation) -> Self {
        Self::Committed(known_safety_pause_view(operation))
    }

    /// 从已落库事实构造幂等重放视图。
    pub(crate) fn already_safe(operation: SystemSafetyPauseOperation) -> Self {
        Self::AlreadySafe(known_safety_pause_view(operation))
    }
}

fn known_safety_pause_view(operation: SystemSafetyPauseOperation) -> KnownSystemSafetyPauseOperationView {
    let affected_publications = operation
        .affected_publications
        .into_iter()
        .map(|item| SafetyPauseAffectedPublicationView {
            publication_id: item.publication_id.to_string(),
            pause_artifact_kind: SafetyPauseArtifactKindView::Revision,
            pause_revision_id: item.pause_revision_id.to_string(),
            delivery_id: item.delivery_id.to_string(),
        })
        .collect();
    let follow_up = match operation.follow_up {
        SafetyPauseFollowUp::WorkItem(item) => KnownSafetyPauseFollowUpView::WorkItem {
            follow_up_work_item: SafetyPauseFollowUpWorkItemView {
                work_item_id: item.work_item_id,
                task_version: item.task_version.to_string(),
                work_item_type: "BUSINESS_EXCEPTION".to_string(),
                business_object_type: item.business_object_type,
                business_object_id: item.business_object_id,
                subject_version: item.subject_version,
                handler_key: item.handler_key,
            },
        },
        SafetyPauseFollowUp::Blocker(blocker) => KnownSafetyPauseFollowUpView::Blocker {
            follow_up_blocker: SafetyPauseFollowUpBlockerView {
                code: blocker.code.as_str().to_string(),
                message: blocker.message,
                evidence_reference: blocker.evidence_reference,
            },
        },
    };
    KnownSystemSafetyPauseOperationView {
        operation_id: operation.base.id,
        cause: operation.cause,
        source_object_type: operation.source_object_type,
        source_object_id: operation.source_object_id,
        source_version: operation.source_version,
        availability_effect: "PAUSED".to_string(),
        affected_publications,
        committed_at: operation.committed_at.unix_secs(),
        follow_up,
    }
}

/// 商品发布列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductPublicationListParams {
    /// ERP SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 发布状态筛选。
    pub status: Option<ProductPublicationStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`sku_id`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商品发布列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductPublicationListQuery {
    /// ERP SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 发布状态筛选。
    pub status: Option<ProductPublicationStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductPublicationListParams {
    /// 归一化商品发布列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductPublicationListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PRODUCT_PUBLICATION_SORT_FIELDS)?;
        Ok(ProductPublicationListQuery {
            sku_id: self.sku_id.clone(),
            target_mall_id: self.target_mall_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 发布投递列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductPublicationDeliveryListParams {
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 投递状态筛选。
    pub delivery_status: Option<PublicationDeliveryStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的发布投递列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductPublicationDeliveryListQuery {
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 投递状态筛选。
    pub delivery_status: Option<PublicationDeliveryStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductPublicationDeliveryListParams {
    /// 归一化发布投递列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductPublicationDeliveryListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PUBLICATION_DELIVERY_SORT_FIELDS)?;
        Ok(ProductPublicationDeliveryListQuery {
            target_mall_id: self.target_mall_id.clone(),
            delivery_status: self.delivery_status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 计算发布修订内容指纹（发布内容指纹，§6.15；由快照字段的规范化文本派生）。
///
/// # 参数
/// * `revision` - 待指纹化的发布修订实体
///
/// # 返回
/// 返回 64 位 FNV-1a 十六进制指纹（长度上限 128 内）。
pub(crate) fn publication_content_hash(revision: &ProductPublicationRevision) -> String {
    let canonical = format!(
        "{}|{:?}|{}|{}|{}|{}|{}|{:?}|{}|{:?}|{}|{:?}",
        revision.name,
        revision.specification,
        revision.sales_description,
        revision.minimum_purchase_quantity,
        revision.sales_price_gross,
        revision.sales_tax_rate,
        revision.base_unit_code,
        revision.sales_region,
        revision.sale_status.as_str(),
        revision.product_capabilities,
        revision.valid_from.unix_secs(),
        revision.valid_to.map(|at| at.unix_secs()),
    );
    let hash = fnv1a64(canonical.as_bytes());
    format!("{hash:016x}")
}

/// FNV-1a 64 位哈希。
///
/// # 参数
/// * `bytes` - 待哈希字节
///
/// # 返回
/// 返回哈希值。
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl From<ProductPublication> for ProductPublicationView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `publication` - 发布实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(publication: ProductPublication) -> Self {
        Self {
            id: publication.base.id,
            sku_id: publication.sku_id.to_string(),
            target_mall_id: publication.target_mall_id.to_string(),
            status: publication.stable.status,
            current_revision_id: publication.stable.current_revision_id,
            version: publication.base.version,
            created_at: publication.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, publication_content_hash, ProductPublicationListParams, SortDir,
        SystemSafetyPauseTrigger,
    };
    use entities::ids::{ProductPublicationId, SkuId, SourceSystemId};
    use entities::publication::{ProductPublicationStatus, SafetyPauseCause, SafetyPauseSourceObjectType};
    use std::str::FromStr;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("status".to_string()), &None, &["created_at", "sku_id"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" sku_id ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "sku_id"],
        )
        .unwrap();
        assert_eq!(field, "sku_id");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_and_sort_defaults() {
        let params = ProductPublicationListParams {
            sku_id: Some(SkuId::new("sku-1")),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: Some(ProductPublicationStatus::PendingPublish),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
    }

    #[test]
    fn content_hash_is_deterministic_and_bounded() {
        let revision = entities::publication::ProductPublicationRevision::new(
            entities::ids::ProductPublicationRevisionId::new("rev-1"),
            1,
            entities::publication::ProductPublicationRevisionData {
                product_publication_id: ProductPublicationId::new("pub-1"),
                sku_revision_id: entities::ids::SkuRevisionId::new("sku-rev-1"),
                supplier_offering_revision_id: entities::ids::SupplierOfferingRevisionId::new("off-rev-1"),
                category_id: entities::ids::ProductCategoryId::new("cat-1"),
                name: "福利商城卡".to_string(),
                specification: None,
                sales_description: "员工福利采购".to_string(),
                minimum_purchase_quantity: entities::money::Quantity::from_str("1.000000").unwrap(),
                sales_price_gross: entities::money::Amount::from_str("100.00").unwrap(),
                sales_tax_rate: entities::money::Rate::from_str("0.130000").unwrap(),
                base_unit_code: "张".to_string(),
                sales_region: None,
                sale_status: entities::publication::SaleStatus::OnSale,
                product_capabilities: vec![entities::publication::ProductCapability::Cancel],
                valid_from: entities::common::time::Instant::from_unix_secs(1_700_000_000),
                valid_to: Some(entities::common::time::Instant::from_unix_secs(1_800_000_000)),
                content_hash: "placeholder".to_string(),
            },
        )
        .unwrap();
        let first = publication_content_hash(&revision);
        let second = publication_content_hash(&revision);
        assert_eq!(first, second, "指纹必须确定");
        assert!(first.len() <= 128);
    }

    #[test]
    fn trigger_rejects_unknown_and_blank_source_identity() {
        let unknown = SystemSafetyPauseTrigger {
            cause: SafetyPauseCause::Unknown,
            source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
            source_object_id: "offering-1".to_string(),
            source_version: "availability:2".to_string(),
            occurred_at: entities::common::time::Instant::from_unix_secs(1),
            idempotency_key: "event-1".to_string(),
        };
        assert!(unknown.validate_contract().is_err());

        let blank = SystemSafetyPauseTrigger {
            cause: SafetyPauseCause::SupplyUnavailable,
            source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
            source_object_id: " ".to_string(),
            source_version: "availability:2".to_string(),
            occurred_at: entities::common::time::Instant::from_unix_secs(1),
            idempotency_key: "event-1".to_string(),
        };
        assert!(blank.validate_contract().is_err());
    }
}
