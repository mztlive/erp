//! 域 D28 `card_instance` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额一律十进制字符串。

use entities::card_instance::{CardSourceType, CorrectionType, CutoverStatus};
use entities::ids::{ExternalIdentityMapId, MallCardInstanceId, SalesOrderId, SalesOrderRevisionId};
use entities::money::Amount;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 切换记录列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const CUTOVER_SORT_FIELDS: &[&str] = &["created_at", "enabled_at"];
/// 卡实例列表允许的排序字段白名单。
pub(crate) const CARD_INSTANCE_SORT_FIELDS: &[&str] = &["baseline_at", "created_at"];
/// 余额快照列表允许的排序字段白名单。
pub(crate) const BALANCE_SNAPSHOT_SORT_FIELDS: &[&str] = &["snapshot_at", "created_at"];
/// 纠错列表允许的排序字段白名单。
pub(crate) const CORRECTION_SORT_FIELDS: &[&str] = &["correction_no", "created_at"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
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

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

/// 商城切换记录创建请求（W28 上线切换）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCutoverRequest {
    /// 目标商城（`source_system.code`）。
    #[validate(custom(function = "non_blank", message = "目标商城不能为空"))]
    pub mall_id: String,
    /// 上线核对文档引用（可空）。
    pub checklist_reference: Option<String>,
}

/// 启用切换请求（登记唯一 `T`，乐观锁语义）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EnableCutoverRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启用时间 `T`（秒级时间戳）。
    #[validate(range(min = 1, message = "启用时间必须大于 0"))]
    pub enabled_at: u64,
}

/// 切换记录响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CutoverView {
    /// 实体主键。
    pub id: String,
    /// 目标商城。
    pub mall_id: String,
    /// 切换状态。
    pub status: CutoverStatus,
    /// 启用时间 `T`；启用前为空。
    pub enabled_at: Option<u64>,
    /// 上线负责人。
    pub enabled_by: Option<String>,
    /// 上线核对文档引用。
    pub checklist_reference: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

/// 切换记录列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CutoverListParams {
    /// 目标商城模糊筛选。
    pub mall_id: Option<String>,
    /// 切换状态筛选。
    pub status: Option<CutoverStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`enabled_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的切换记录列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CutoverListQuery {
    /// 目标商城模糊筛选。
    pub mall_id: Option<String>,
    /// 切换状态筛选。
    pub status: Option<CutoverStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CutoverListParams {
    /// 归一化切换记录列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CutoverListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, CUTOVER_SORT_FIELDS)?;
        Ok(CutoverListQuery {
            mall_id: normalized_text(self.mall_id.as_deref()),
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

/// 卡实例创建请求（实时或历史基线，随单形成初始余额快照）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCardInstanceRequest {
    /// 来源商城。
    #[validate(custom(function = "non_blank", message = "来源商城不能为空"))]
    pub mall_id: String,
    /// 不可反推卡号、卡密的稳定引用。
    #[validate(custom(function = "non_blank", message = "卡实例稳定引用不能为空"))]
    pub opaque_instance_ref: String,
    /// 原商城卡券销售单的 `external_identity_map` 稳定身份。
    pub origin_sales_order_source_identity_id: ExternalIdentityMapId,
    /// 映射后的 ERP 销售单。
    pub origin_sales_order_id: SalesOrderId,
    /// 基线形成时生效的销售单版本。
    pub origin_sales_order_revision_id: SalesOrderRevisionId,
    /// 商城提供的卡实例基线版本（可空）。
    pub source_baseline_version: Option<String>,
    /// 初始余额（十进制字符串，2 位小数）。
    #[validate(custom(function = "valid_amount", message = "初始余额必须是合法金额"))]
    pub initial_balance: String,
    /// 基线形成时间（秒级时间戳）。
    #[validate(range(min = 1, message = "基线时间必须大于 0"))]
    pub baseline_at: u64,
    /// 实时或历史基线。
    pub source_type: CardSourceType,
}

/// 校验金额字符串为合法非负定点数值（小数位 ≤ 2）。
fn valid_amount(value: &str) -> std::result::Result<(), validator::ValidationError> {
    let amount = Amount::from_str(value).map_err(|_| validator::ValidationError::new("不是合法定点数值"))?;
    if amount.to_decimal().is_sign_negative() {
        return Err(validator::ValidationError::new("金额不能为负"));
    }
    Ok(())
}

/// 卡实例响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardInstanceView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 稳定引用。
    pub opaque_instance_ref: String,
    /// 映射后的 ERP 销售单。
    pub origin_sales_order_id: String,
    /// 基线时生效的销售单版本（列表投影不提供时为 `None`）。
    pub origin_sales_order_revision_id: Option<String>,
    /// 商城提供的基线版本。
    pub source_baseline_version: Option<String>,
    /// 初始余额（字符串）。
    pub initial_balance: String,
    /// 基线形成时间（秒级时间戳）。
    pub baseline_at: u64,
    /// 实时或历史基线。
    pub source_type: CardSourceType,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

/// 卡实例列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CardInstanceListParams {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 稳定引用模糊筛选。
    pub opaque_instance_ref: Option<String>,
    /// 来源类型筛选。
    pub source_type: Option<CardSourceType>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`baseline_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的卡实例列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CardInstanceListQuery {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 稳定引用模糊筛选。
    pub opaque_instance_ref: Option<String>,
    /// 来源类型筛选。
    pub source_type: Option<CardSourceType>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CardInstanceListParams {
    /// 归一化卡实例列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CardInstanceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, CARD_INSTANCE_SORT_FIELDS)?;
        Ok(CardInstanceListQuery {
            mall_id: normalized_text(self.mall_id.as_deref()),
            opaque_instance_ref: normalized_text(self.opaque_instance_ref.as_deref()),
            source_type: self.source_type,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 卡实例详情视图（W28 卡券消费台账：基线 + 最新余额 + 纠错摘要）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardInstanceDetailView {
    /// 基线视图。
    pub instance: CardInstanceView,
    /// 最新余额快照（字符串）；尚无快照时为空。
    pub latest_balance: Option<String>,
    /// 余额快照笔数。
    pub balance_snapshot_count: u64,
    /// 纠错条数。
    pub correction_count: u64,
}

/// 余额快照创建请求（商城余额快照回流）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateBalanceSnapshotRequest {
    /// 卡实例 ID。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 快照时间（秒级时间戳）。
    #[validate(range(min = 1, message = "快照时间必须大于 0"))]
    pub snapshot_at: u64,
    /// 商城当时有效余额（十进制字符串）。
    #[validate(custom(function = "valid_amount", message = "余额必须是合法金额"))]
    pub balance: String,
    /// 商城余额快照版本（可空）。
    pub source_snapshot_version: Option<String>,
    /// 必填来源消息事件 ID。
    #[validate(custom(function = "non_blank", message = "来源事件ID不能为空"))]
    pub source_event_id: String,
}

/// 余额快照响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BalanceSnapshotView {
    /// 实体主键。
    pub id: String,
    /// 卡实例。
    pub mall_card_instance_id: String,
    /// 快照时间（秒级时间戳）。
    pub snapshot_at: u64,
    /// 当时有效余额（字符串）。
    pub balance: String,
    /// 商城快照版本。
    pub source_snapshot_version: Option<String>,
    /// 来源消息事件 ID。
    pub source_event_id: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 余额快照列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BalanceSnapshotListParams {
    /// 卡实例筛选。
    pub mall_card_instance_id: Option<MallCardInstanceId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`snapshot_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的余额快照列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BalanceSnapshotListQuery {
    /// 卡实例筛选。
    pub mall_card_instance_id: Option<MallCardInstanceId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl BalanceSnapshotListParams {
    /// 归一化余额快照列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<BalanceSnapshotListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, BALANCE_SNAPSHOT_SORT_FIELDS)?;
        Ok(BalanceSnapshotListQuery {
            mall_card_instance_id: self.mall_card_instance_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 卡实例纠错响应视图（只读，纠错由财务复核流程追加）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CorrectionView {
    /// 实体主键。
    pub id: String,
    /// 卡实例。
    pub mall_card_instance_id: String,
    /// 同卡实例递增纠错号。
    pub correction_no: u32,
    /// 纠错类型。
    pub correction_type: CorrectionType,
    /// 原值。
    pub before_value: String,
    /// 经确认的新值。
    pub after_value: String,
    /// 纠错依据。
    pub reason: String,
    /// 审批人。
    pub approved_by: String,
    /// 审批时间（秒级时间戳）。
    pub approved_at: u64,
    /// 承接的同卡实例上一纠错。
    pub supersedes_correction_id: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 纠错列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CorrectionListParams {
    /// 卡实例筛选。
    pub mall_card_instance_id: Option<MallCardInstanceId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`correction_no`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的纠错列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorrectionListQuery {
    /// 卡实例筛选。
    pub mall_card_instance_id: Option<MallCardInstanceId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CorrectionListParams {
    /// 归一化纠错列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CorrectionListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, CORRECTION_SORT_FIELDS)?;
        Ok(CorrectionListQuery {
            mall_card_instance_id: self.mall_card_instance_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir};
    use crate::card_instance::dto::{CardInstanceListParams, CutoverListParams};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" enabled_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "enabled_at"],
        )
        .unwrap();
        assert_eq!(field, "enabled_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = CutoverListParams {
            mall_id: Some(" mall-a ".to_string()),
            status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.mall_id.as_deref(), Some("mall-a"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);

        let params = CardInstanceListParams {
            mall_id: None,
            opaque_instance_ref: Some(" card-ref ".to_string()),
            source_type: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("baseline_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.opaque_instance_ref.as_deref(), Some("card-ref"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.sort_by, "baseline_at");
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = CutoverListParams {
            mall_id: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(validator::Validate::validate(&params).is_err());
    }
}
