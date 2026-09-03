//! 域 D27 `projection` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额使用 `entities::money`
//! 定点类型，JSON 形态为字符串。
//!
//! 投影只包含销售单号、版本、客户、卡券类目、履约期限、面额、数量、卡形态和
//! 生效时间（§6.16 字段集即白名单）：响应视图不回传成交金额、配赠、税率、
//! 开票与应收。

use entities::ids::{SalesOrderId, SourceSystemId};
use entities::integration_ops::ErrorClass;
use entities::money::Amount;
use entities::projection::{CardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

/// 投影列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SALES_ORDER_PROJECTION_SORT_FIELDS: &[&str] =
    &["created_at", "sales_order_id", "updated_at"];
/// 投影下发列表允许的排序字段白名单。
pub(crate) const PROJECTION_DELIVERY_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
pub use crate::query::SortDir;

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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空标识需要按「空白视为空」拒绝，落入 HTTP 400）。
use crate::query::non_blank;

/// 建立执行投影请求（存量单切换的第一份投影版本，phase-2 §8.5.4）。
///
/// 唯一卡券明细执行字段（面额/卡张数/卡形态）与表头履约期限/生效时间由 ERP
/// 销售单当前版本派生，请求只携带商城侧标识。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesOrderProjectionRequest {
    /// 卡券销售单（D13 `sales_order`）。
    pub sales_order_id: SalesOrderId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 商城客户标识。
    #[validate(custom(function = "non_blank", message = "商城客户标识不能为空"))]
    pub customer_external_identity: String,
    /// 商城卡券类目标识。
    #[validate(custom(function = "non_blank", message = "商城卡券类目标识不能为空"))]
    pub voucher_category_external_identity: String,
}

/// 推进执行投影版本请求（后续 ERP 销售版本，投影来源 `ErpRevision`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesOrderProjectionRevisionRequest {
    /// 商城客户标识。
    #[validate(custom(function = "non_blank", message = "商城客户标识不能为空"))]
    pub customer_external_identity: String,
    /// 商城卡券类目标识。
    #[validate(custom(function = "non_blank", message = "商城卡券类目标识不能为空"))]
    pub voucher_category_external_identity: String,
}

/// 投影下发请求（携带幂等键；`(projection_revision_id, target_mall_id)` 唯一索引承接）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DeliverProjectionRevisionRequest {
    /// 调用方幂等键（重复下发不产生第二笔外部调用与第二份下发记录）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 执行投影响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderProjectionView {
    /// 实体主键。
    pub id: String,
    /// 卡券销售单。
    pub sales_order_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 商城最后确认版本。
    pub current_acked_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 执行投影列表项；一次返回当前页所需的最新修订与投递事实。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderProjectionListItemView {
    /// 稳定投影身份。
    #[serde(flatten)]
    pub projection: SalesOrderProjectionView,
    /// 最新修订；尚未形成修订时为空。
    pub latest_revision: Option<SalesOrderProjectionRevisionView>,
    /// 最新修订对应的投递；尚未形成投递时为空。
    pub latest_delivery: Option<SalesOrderProjectionDeliveryView>,
}

/// 投影修订响应视图（白名单字段，§6.16）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderProjectionRevisionView {
    /// 实体主键。
    pub id: String,
    /// 所属投影稳定身份。
    pub projection_id: String,
    /// 修订序号（同一投影内从 1 递增）。
    pub revision_no: u32,
    /// 投影来源。
    pub projection_source: ProjectionSource,
    /// ERP 销售版本。
    pub sales_order_revision_id: String,
    /// 商城客户标识。
    pub customer_external_identity: String,
    /// 卡券面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 电子卡或实体卡。
    pub card_form: CardForm,
    /// ERP 生效时间（秒级时间戳）。
    pub effective_at: i64,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投影下发响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderProjectionDeliveryView {
    /// 实体主键。
    pub id: String,
    /// 待下发投影版本。
    pub projection_revision_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 下发状态。
    pub status: ProjectionDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近发送时间（秒级时间戳）。
    pub last_attempt_at: Option<i64>,
    /// 下次受控处理时间（秒级时间戳）。
    pub next_attempt_at: Option<i64>,
    /// 商城确认时间（秒级时间戳）。
    pub mall_ack_at: Option<i64>,
    /// 商城执行基线。
    pub mall_execution_baseline: Option<String>,
    /// 稳定错误分类。
    pub error_class: Option<ErrorClass>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 脱敏错误摘要。
    pub error_summary: Option<String>,
    /// W29 错误对象。
    pub error_task_id: Option<String>,
    /// W29 正式待办。
    pub work_item_id: Option<String>,
    /// 服务端根据当前投递事实开放的对象动作。
    pub allowed_actions: Vec<String>,
    /// 当前动作阻断原因。
    pub action_blockers: Vec<ProjectionActionBlockerView>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// W23 投递对象动作阻断投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectionActionBlockerView {
    /// 被阻断的动作代码。
    pub action: String,
    /// 稳定阻断代码。
    pub code: String,
    /// 面向处理人的明确说明。
    pub message: String,
}

/// 投递对象强动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectionDeliveryAction {
    /// 查询原稳定消息键的商城最终结果。
    QueryResult,
    /// 沿原稳定消息键安排重试。
    Retry,
    /// 按固定责任规则升级为 W29 错误对象与待办。
    Escalate,
}

impl ProjectionDeliveryAction {
    /// 返回协议稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueryResult => "QUERY_RESULT",
            Self::Retry => "RETRY",
            Self::Escalate => "ESCALATE",
        }
    }
}

/// 投递对象强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProjectionDeliveryCommand {
    /// 投影稳定身份。
    #[validate(custom(function = "non_blank", message = "投影ID不能为空"))]
    pub projection_id: String,
    /// 投影不可变修订。
    #[validate(custom(function = "non_blank", message = "投影修订ID不能为空"))]
    pub projection_revision_id: String,
    /// 固定投递身份。
    #[validate(custom(function = "non_blank", message = "投递ID不能为空"))]
    pub delivery_id: String,
    /// 强类型对象动作。
    pub action: ProjectionDeliveryAction,
    /// 查询所得投递对象版本。
    pub expected_object_version: u64,
    /// 调用方幂等请求身份；不会原文写入日志。
    #[validate(length(max = 128, message = "请求ID过长"))]
    #[validate(custom(function = "non_blank", message = "请求ID不能为空"))]
    pub request_id: String,
}

/// 批量投递命令动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectionBulkAction {
    /// 逐项查询原稳定消息键的商城最终结果。
    BulkQuery,
    /// 逐项沿原稳定消息键安排受控重试。
    BulkRetry,
}

impl ProjectionBulkAction {
    /// 返回协议稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BulkQuery => "BULK_QUERY",
            Self::BulkRetry => "BULK_RETRY",
        }
    }

    /// 返回单项投递动作。
    pub fn delivery_action(self) -> ProjectionDeliveryAction {
        match self {
            Self::BulkQuery => ProjectionDeliveryAction::QueryResult,
            Self::BulkRetry => ProjectionDeliveryAction::Retry,
        }
    }
}

/// 批量投递命令请求。
///
/// 客户端只提交显式选中的稳定投影 ID；投影修订、投递身份与当前
/// 对象版本由服务端在命令执行时解析。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProjectionBulkCommandRequest {
    /// 批量动作。
    pub action: ProjectionBulkAction,
    /// 显式选中的投影 ID，上限 20 条。
    #[validate(length(min = 1, max = 20, message = "批量选择数必须在1-20之间"))]
    pub projection_ids: Vec<String>,
    /// 批次幂等请求身份。
    #[validate(length(max = 128, message = "请求ID过长"))]
    #[validate(custom(function = "non_blank", message = "请求ID不能为空"))]
    pub request_id: String,
}

/// 批量命令单项结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionBulkItemResultView {
    /// 投影稳定身份。
    pub projection_id: String,
    /// 销售单稳定身份；未能解析投影时回退为投影 ID。
    pub sales_order_no: String,
    /// 投递身份；解析前失败时为空。
    pub delivery_id: String,
    /// `SUCCEEDED` / `SKIPPED` / `FAILED` / `STILL_UNKNOWN`。
    pub outcome: String,
    /// 面向操作人的结果说明。
    pub reason: String,
}

/// 批量投递命令结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionBulkCommandResultView {
    /// 稳定批次编号。
    pub job_id: String,
    /// 批量动作。
    pub action: ProjectionBulkAction,
    /// `SUCCEEDED` / `PARTIAL` / `FAILED`。
    pub status: String,
    /// 选中总数。
    pub total: u32,
    /// 已处理数。
    pub completed: u32,
    /// 成功数。
    pub succeeded: u32,
    /// 跳过数。
    pub skipped: u32,
    /// 失败数。
    pub failed: u32,
    /// 结果仍未知数。
    pub still_unknown: u32,
    /// 本次显式选择的稳定摘要身份。
    pub selection_snapshot_id: String,
    /// 逐项结果。
    pub items: Vec<ProjectionBulkItemResultView>,
    /// 开始时间（秒级时间戳）。
    pub started_at: i64,
    /// 完成时间（秒级时间戳）。
    pub finished_at: i64,
    /// 服务端给出的下一步。
    pub next_action: String,
}

/// 投递对象强动作结果分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectionDeliveryActionResult {
    /// 商城权威确认。
    Acked,
    /// 商城明确失败，或发送入口在调用前失败关闭。
    Failed,
    /// 查询后仍无法确定最终结果。
    StillUnknown,
    /// 已沿原身份排入受控重试。
    RetryScheduled,
    /// 已创建或复用 W29 错误对象与待办。
    Escalated,
}

/// 下发与对象动作结果视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionDeliveryResultView {
    /// 稳定操作编号；同一请求重试返回相同编号与原结果。
    pub operation_id: String,
    /// 固定投递记录 ID。
    pub delivery_id: String,
    /// 正式动作结果。
    pub result: ProjectionDeliveryActionResult,
    /// 升级 W29 时的正式待办。
    pub work_item_id: Option<String>,
    /// 升级 W29 时的错误对象。
    pub error_task_id: Option<String>,
    /// 结果形成时间（秒级时间戳）。
    pub occurred_at: i64,
    /// 服务端给出的下一步。
    pub next_action: Option<String>,
}

/// 受控待发送处理请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProcessProjectionDeliveriesRequest {
    /// 本批最大处理数，避免无界扫描。
    #[validate(range(min = 1, max = 100, message = "单批处理数必须在1-100之间"))]
    pub limit: Option<u32>,
}

/// 受控待发送处理结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcessProjectionDeliveriesResult {
    /// 本批取得的候选数量。
    pub scanned: u32,
    /// 商城确认数量。
    pub acked: u32,
    /// 明确失败数量。
    pub failed: u32,
    /// 结果未知数量。
    pub still_unknown: u32,
    /// CAS 未取得或状态已变化数量。
    pub skipped: u32,
    /// 逐项正式结果。
    pub items: Vec<ProjectionDeliveryResultView>,
}

/// 执行投影列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesOrderProjectionListParams {
    /// 卡券销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`sales_order_id`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的执行投影列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesOrderProjectionListQuery {
    /// 卡券销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesOrderProjectionListParams {
    /// 归一化执行投影列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesOrderProjectionListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SALES_ORDER_PROJECTION_SORT_FIELDS)?;
        Ok(SalesOrderProjectionListQuery {
            sales_order_id: self.sales_order_id.clone(),
            target_mall_id: self.target_mall_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 投影下发列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesOrderProjectionDeliveryListParams {
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 下发状态筛选。
    pub status: Option<ProjectionDeliveryStatus>,
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

/// 归一化后的投影下发列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesOrderProjectionDeliveryListQuery {
    /// 目标商城筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 下发状态筛选。
    pub status: Option<ProjectionDeliveryStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesOrderProjectionDeliveryListParams {
    /// 归一化投影下发列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesOrderProjectionDeliveryListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PROJECTION_DELIVERY_SORT_FIELDS)?;
        Ok(SalesOrderProjectionDeliveryListQuery {
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

impl From<SalesOrderProjection> for SalesOrderProjectionView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `projection` - 投影实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(projection: SalesOrderProjection) -> Self {
        Self {
            id: projection.base.id,
            sales_order_id: projection.sales_order_id.to_string(),
            target_mall_id: projection.target_mall_id.to_string(),
            current_acked_revision_id: projection.current_acked_revision_id.map(|id| id.to_string()),
            version: projection.base.version,
            created_at: projection.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SalesOrderProjectionListParams, SortDir};
    use entities::ids::{SalesOrderId, SourceSystemId};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(
            &Some("status".to_string()),
            &None,
            &["created_at", "sales_order_id"]
        )
        .is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" sales_order_id ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "sales_order_id"],
        )
        .unwrap();
        assert_eq!(field, "sales_order_id");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_and_sort_defaults() {
        let params = SalesOrderProjectionListParams {
            sales_order_id: Some(SalesOrderId::new("so-1")),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.sales_order_id.as_deref(), Some("so-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }
}
