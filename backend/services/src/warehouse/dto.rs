//! 域 D11 `warehouse` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；数量为十进制字符串；
//! 生效日期为 `YYYY-MM-DD`（`BusinessDate` 的既有序列化形态）。
//!
//! 排序白名单校验辅助（`normalize_sort`/`PageParams`/`PageView`）与 D01
//! source_registry、D10 catalog 同构；抽取到冻结的 `services/src/query.rs`
//! 属地基修订候选（见域报告）。

use entities::common::time::BusinessDate;
use entities::ids::{SkuId, WarehouseId};
use entities::money::Quantity;
use entities::warehouse::status::EnableStatus;
use entities::warehouse::warehouse_entity::Warehouse;
use entities::warehouse::warehouse_sku_policy::WarehouseSkuPolicy;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 仓库列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const WAREHOUSE_SORT_FIELDS: &[&str] = &["created_at", "warehouse_code"];
/// 仓库修订列表允许的排序字段白名单。
pub(crate) const WAREHOUSE_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];
/// 仓库-SKU 预警策略列表允许的排序字段白名单。
pub(crate) const WAREHOUSE_SKU_POLICY_SORT_FIELDS: &[&str] = &["created_at", "effective_from"];

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
/// 不生效，空 code/name 需要按「空白视为空」拒绝，落入 HTTP 400）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 仓库创建请求（仓库稳定身份 + 首个仓库修订快照）。
///
/// 地址与联系人为明文输入；Service 按数据模型 §4.5.5 生成带密钥 HMAC 指纹的
/// `SensitiveText`（密文列与查询指纹，禁止裸摘要）后落库。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateWarehouseRequest {
    /// ERP 仓库稳定代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "仓库代码不能为空"))]
    pub warehouse_code: String,
    /// 仓库名称（结构化快照）。
    #[validate(custom(function = "non_blank", message = "仓库名称不能为空"))]
    pub name: String,
    /// 地址（敏感字段，落库前生成密文与查询指纹）。
    #[validate(custom(function = "non_blank", message = "仓库地址不能为空"))]
    pub address: String,
    /// 联系人（敏感字段，落库前生成密文与查询指纹）。
    #[validate(custom(function = "non_blank", message = "仓库联系人不能为空"))]
    pub contact: String,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 仓库更新请求（追加新修订：名称/地址/联系人/有效期/变更原因 + 状态）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateWarehouseRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 仓库名称（结构化快照）。
    #[validate(custom(function = "non_blank", message = "仓库名称不能为空"))]
    pub name: String,
    /// 地址（敏感字段）。
    #[validate(custom(function = "non_blank", message = "仓库地址不能为空"))]
    pub address: String,
    /// 联系人（敏感字段）。
    #[validate(custom(function = "non_blank", message = "仓库联系人不能为空"))]
    pub contact: String,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 仓库响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WarehouseView {
    /// 实体主键。
    pub id: String,
    /// ERP 仓库稳定代码。
    pub warehouse_code: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
}

impl From<Warehouse> for WarehouseView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `warehouse` - 仓库实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(warehouse: Warehouse) -> Self {
        Self {
            id: warehouse.base.id,
            warehouse_code: warehouse.warehouse_code,
            status: warehouse.stable.status,
            created_at: warehouse.base.created_at,
            version: warehouse.base.version,
        }
    }
}

/// 仓库列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WarehouseListParams {
    /// 仓库代码精确筛选。
    pub warehouse_code: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`warehouse_code`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的仓库列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WarehouseListQuery {
    /// 仓库代码精确筛选。
    pub warehouse_code: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl WarehouseListParams {
    /// 归一化仓库列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<WarehouseListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, WAREHOUSE_SORT_FIELDS)?;
        Ok(WarehouseListQuery {
            warehouse_code: normalized_text(self.warehouse_code.as_deref()),
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

/// 仓库修订响应视图（不含加密地址/联系人等敏感字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WarehouseRevisionView {
    /// 实体主键。
    pub id: String,
    /// 所属仓库。
    pub warehouse_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 仓库名称（结构化快照）。
    pub name: String,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

/// 仓库修订列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WarehouseRevisionListParams {
    /// 所属仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的仓库修订列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WarehouseRevisionListQuery {
    /// 所属仓库筛选。
    pub warehouse_id: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl WarehouseRevisionListParams {
    /// 归一化仓库修订列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<WarehouseRevisionListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, WAREHOUSE_REVISION_SORT_FIELDS)?;
        Ok(WarehouseRevisionListQuery {
            warehouse_id: self.warehouse_id.as_ref().map(|id| id.to_string()),
            name: normalized_text(self.name.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 仓库-SKU 预警策略创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateWarehouseSkuPolicyRequest {
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 最低可用量预警阈值（定点数，非负）。
    pub minimum_available_quantity: Quantity,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

/// 仓库-SKU 预警策略更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateWarehouseSkuPolicyRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 最低可用量预警阈值；缺省表示不修改。
    pub minimum_available_quantity: Option<Quantity>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 仓库-SKU 预警策略响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WarehouseSkuPolicyView {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// SKU。
    pub sku_id: String,
    /// 最低可用量预警阈值（字符串形态）。
    pub minimum_available_quantity: Quantity,
    /// 启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<WarehouseSkuPolicy> for WarehouseSkuPolicyView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `policy` - 预警策略实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(policy: WarehouseSkuPolicy) -> Self {
        Self {
            id: policy.base.id,
            warehouse_id: policy.warehouse_id.to_string(),
            sku_id: policy.sku_id.to_string(),
            minimum_available_quantity: policy.minimum_available_quantity,
            status: policy.status,
            effective_from: policy.effective_from,
            effective_to: policy.effective_to,
            created_at: policy.base.created_at,
            version: policy.base.version,
        }
    }
}

/// 仓库-SKU 预警策略列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WarehouseSkuPolicyListParams {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`effective_from`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的仓库-SKU 预警策略列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WarehouseSkuPolicyListQuery {
    /// 仓库筛选。
    pub warehouse_id: Option<String>,
    /// SKU 筛选。
    pub sku_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl WarehouseSkuPolicyListParams {
    /// 归一化仓库-SKU 预警策略列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<WarehouseSkuPolicyListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, WAREHOUSE_SKU_POLICY_SORT_FIELDS)?;
        Ok(WarehouseSkuPolicyListQuery {
            warehouse_id: self.warehouse_id.as_ref().map(|id| id.to_string()),
            sku_id: self.sku_id.as_ref().map(|id| id.to_string()),
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

#[cfg(test)]
mod tests {
    use super::normalize_sort;
    use super::SortDir;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" warehouse_code ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "warehouse_code"],
        )
        .unwrap();
        assert_eq!(field, "warehouse_code");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn warehouse_params_normalize_filters_and_defaults() {
        let params: super::WarehouseListParams = serde_json::from_value(serde_json::json!({
            "warehouse_code": " WH-1 ",
            "status": "active",
            "page": 2,
            "page_size": 50,
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.warehouse_code.as_deref(), Some("WH-1"));
        assert_eq!(query.status, Some(super::EnableStatus::Active));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn policy_params_reject_unbounded_page_size() {
        let params: super::WarehouseSkuPolicyListParams = serde_json::from_value(serde_json::json!({
            "page": 0,
            "page_size": 1000,
        }))
        .unwrap();
        assert!(params.validate().is_err());
    }

    #[test]
    fn create_warehouse_request_rejects_blank_sensitive_fields() {
        let request: super::CreateWarehouseRequest = serde_json::from_value(serde_json::json!({
            "warehouse_code": "WH-1",
            "name": "一号仓",
            "address": "  ",
            "contact": "张三",
            "effective_from": "2026-01-01",
            "change_reason": "期初建仓",
        }))
        .unwrap();
        assert!(request.validate().is_err(), "空白地址必须被拒绝");
    }
}
