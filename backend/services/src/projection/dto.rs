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
use entities::money::Amount;
use entities::projection::{
    CardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection, SalesOrderProjectionRevision,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{page_or_default, page_size_or_default};

/// 投影列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SALES_ORDER_PROJECTION_SORT_FIELDS: &[&str] =
    &["created_at", "sales_order_id", "updated_at"];
/// 投影下发列表允许的排序字段白名单。
pub(crate) const PROJECTION_DELIVERY_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

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
/// 不生效，空标识需要按「空白视为空」拒绝，落入 HTTP 400）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

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
    /// 商城确认时间（秒级时间戳）。
    pub mall_ack_at: Option<i64>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 下发结果视图（成功/失败均返回消息信封与错误任务 ID）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectionDeliveryResultView {
    /// 下发记录 ID。
    pub delivery_id: String,
    /// 下发状态。
    pub delivery_status: ProjectionDeliveryStatus,
    /// 承接本次下发的消息信封 ID（`inbox_message`）。
    pub inbox_message_id: String,
    /// 失败时创建的集成错误任务 ID；成功路径为 `None`。
    pub error_task_id: Option<String>,
    /// 商城执行基线；成功路径有值。
    pub mall_execution_baseline: Option<String>,
    /// 投影稳定身份乐观锁版本。
    pub projection_version: u64,
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

/// 计算投影版本内容指纹（投影内容指纹，§6.16；由白名单快照字段的规范化文本派生）。
///
/// # 参数
/// * `revision` - 待指纹化的投影版本实体
///
/// # 返回
/// 返回 64 位 FNV-1a 十六进制指纹（长度上限 128 内）。
pub(crate) fn projection_content_hash(revision: &SalesOrderProjectionRevision) -> String {
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        revision.projection_source.as_str(),
        revision.sales_order_revision_id,
        revision.customer_external_identity,
        revision.voucher_category_external_identity,
        revision.voucher_expiry_at.unix_secs(),
        revision.face_value,
        revision.card_count,
        revision.card_form.as_str(),
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
    use super::{normalize_sort, projection_content_hash, SalesOrderProjectionListParams, SortDir};
    use entities::ids::{SalesOrderId, SourceSystemId};
    use entities::projection::{
        CardForm, ProjectionSource, SalesOrderProjectionRevision, SalesOrderProjectionRevisionData,
    };
    use std::str::FromStr;

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

    #[test]
    fn content_hash_is_deterministic_and_bounded() {
        let revision = SalesOrderProjectionRevision::new(
            entities::ids::SalesOrderProjectionRevisionId::new("proj-rev-1"),
            1,
            SalesOrderProjectionRevisionData {
                projection_id: entities::ids::SalesOrderProjectionId::new("proj-1"),
                projection_source: ProjectionSource::CutoverSnapshot,
                sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("so-rev-1"),
                customer_external_identity: "mall-customer-001".to_string(),
                voucher_category_external_identity: "mall-voucher-001".to_string(),
                voucher_expiry_at: entities::common::time::Instant::from_unix_secs(1_800_000_000),
                face_value: entities::money::Amount::from_str("100.00").unwrap(),
                card_count: 100,
                card_form: CardForm::Electronic,
                effective_at: entities::common::time::Instant::from_unix_secs(1_700_000_000),
                content_hash: "placeholder".to_string(),
            },
        )
        .unwrap();
        let first = projection_content_hash(&revision);
        let second = projection_content_hash(&revision);
        assert_eq!(first, second, "指纹必须确定");
        assert!(first.len() <= 128);
    }
}
