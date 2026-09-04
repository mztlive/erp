//! 域 D01 `source_registry` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。

use entities::source_registry::{
    ExternalIdentityMap, ExternalObjectType, MappingStatus, RelationRole, SourceSystem,
    SourceSystemData, SourceSystemId, SourceSystemStatus, SourceSystemType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 来源系统列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const SOURCE_SYSTEM_SORT_FIELDS: &[&str] = &["created_at", "code", "name"];
/// 外部身份映射列表允许的排序字段白名单。
pub(crate) const EXTERNAL_IDENTITY_MAP_SORT_FIELDS: &[&str] = &["created_at"];

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

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。///
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
///
/// `services::Page` 只序列化 `items`/`total`（冻结），列表接口按契约在此补齐
/// `page`/`page_size`，不静默沿用 `{items,total}` 直出。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 code/name 需要按「空白视为空」拒绝，落入 HTTP 400）。
use crate::query::non_blank;

/// 来源系统创建请求.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSourceSystemRequest {
    /// 稳定代码（唯一）。
    #[validate(custom(function = "non_blank", message = "来源系统代码不能为空"))]
    pub code: String,
    /// 显示名称。
    #[validate(custom(function = "non_blank", message = "来源系统名称不能为空"))]
    pub name: String,
    /// 系统类型.
    pub system_type: SourceSystemType,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<SourceSystemStatus>,
}

impl CreateSourceSystemRequest {
    /// 转换为实体创建数据（`status` 缺省按启用处理）。
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self) -> SourceSystemData {
        SourceSystemData {
            code: self.code,
            system_type: self.system_type,
            name: self.name,
            status: self.status.unwrap_or(SourceSystemStatus::Active),
        }
    }
}

/// 来源系统更新请求（携带乐观锁版本，`BaseModel.version` ≡ `lock_version`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSourceSystemRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 显示名称；缺省表示不修改。
    pub name: Option<String>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<SourceSystemStatus>,
}

/// 来源系统响应视图（契约形状：`id`/`code`/`name`/`system_type`/`status`/`created_at`，
/// 另附 `version` 供前端乐观锁更新回传）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceSystemView {
    /// 实体主键。
    pub id: String,
    /// 稳定代码。
    pub code: String,
    /// 显示名称。
    pub name: String,
    /// 系统类型。
    pub system_type: SourceSystemType,
    /// 启停状态。
    pub status: SourceSystemStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
}

impl From<SourceSystem> for SourceSystemView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `system` - 来源系统实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(system: SourceSystem) -> Self {
        Self {
            id: system.base.id,
            code: system.code,
            name: system.name,
            system_type: system.system_type,
            status: system.stable.status,
            created_at: system.base.created_at,
            version: system.base.version,
        }
    }
}

/// 来源系统列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SourceSystemListParams {
    /// 代码精确筛选。
    pub code: Option<String>,
    /// 系统类型筛选。
    pub system_type: Option<SourceSystemType>,
    /// 启停状态筛选。
    pub status: Option<SourceSystemStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的来源系统列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceSystemListQuery {
    /// 代码精确筛选。
    pub code: Option<String>,
    /// 系统类型筛选。
    pub system_type: Option<SourceSystemType>,
    /// 启停状态筛选。
    pub status: Option<SourceSystemStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SourceSystemListParams {
    /// 归一化来源系统列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SourceSystemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SOURCE_SYSTEM_SORT_FIELDS)?;
        Ok(SourceSystemListQuery {
            code: normalized_text(self.code.as_deref()),
            system_type: self.system_type,
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

/// 外部身份映射创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateExternalIdentityMapRequest {
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 外部对象类型。
    pub object_type: ExternalObjectType,
    /// 来源稳定 ID 或单号原值。
    #[validate(length(min = 1, message = "外部ID不能为空"))]
    pub external_id: String,
    /// ERP 规范对象类型。
    pub internal_object_type: ExternalObjectType,
    /// ERP 规范对象 ID。
    #[validate(length(min = 1, message = "内部对象ID不能为空"))]
    pub internal_object_id: String,
    /// 关系角色。
    pub relation_role: RelationRole,
    /// 映射生效时间（秒级时间戳）。
    #[validate(range(min = 1, message = "生效时间必须大于 0"))]
    pub valid_from: u64,
    /// 映射失效时间（秒级时间戳）；缺省表示长期有效。
    pub valid_to: Option<u64>,
}

/// 外部身份映射响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalIdentityMapView {
    /// 实体主键。
    pub id: String,
    /// 来源系统 ID。
    pub source_system_id: String,
    /// 外部对象类型。
    pub object_type: ExternalObjectType,
    /// 来源原值。
    pub external_id: String,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 映射时间（秒级时间戳）。
    pub mapped_at: Option<u64>,
    /// 映射责任人。
    pub mapped_by: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<ExternalIdentityMap> for ExternalIdentityMapView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `map` - 外部身份映射实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露二进制比较键）。
    fn from(map: ExternalIdentityMap) -> Self {
        Self {
            id: map.base.id,
            source_system_id: map.source_system_id.to_string(),
            object_type: map.object_type,
            external_id: map.external_id,
            mapping_status: map.mapping_status,
            mapped_at: map.mapped_at,
            mapped_by: map.mapped_by,
            created_at: map.base.created_at,
        }
    }
}

/// 外部身份映射列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExternalIdentityMapListParams {
    /// 来源系统 ID 筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态筛选。
    pub mapping_status: Option<MappingStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的外部身份映射列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExternalIdentityMapListQuery {
    /// 来源系统 ID 筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态筛选。
    pub mapping_status: Option<MappingStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ExternalIdentityMapListParams {
    /// 归一化外部身份映射列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ExternalIdentityMapListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, EXTERNAL_IDENTITY_MAP_SORT_FIELDS)?;
        Ok(ExternalIdentityMapListQuery {
            source_system_id: self.source_system_id.clone(),
            mapping_status: self.mapping_status,
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
    use super::{normalize_sort, ExternalIdentityMapListParams, SortDir, SourceSystemListParams};
    use entities::source_registry::{
        MappingStatus, SourceSystemId, SourceSystemStatus, SourceSystemType,
    };
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" created_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at"],
        )
        .unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = SourceSystemListParams {
            code: Some(" ERP ".to_string()),
            system_type: Some(SourceSystemType::Mall),
            status: Some(SourceSystemStatus::Active),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.code.as_deref(), Some("ERP"));
        assert_eq!(query.system_type, Some(SourceSystemType::Mall));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = SourceSystemListParams {
            code: None,
            system_type: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn map_list_params_normalize_flat_filters() {
        let params = ExternalIdentityMapListParams {
            source_system_id: Some(SourceSystemId::new("sys-1")),
            mapping_status: Some(MappingStatus::Pending),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.source_system_id.as_deref(), Some("sys-1"));
        assert_eq!(query.mapping_status, Some(MappingStatus::Pending));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
    }

    #[test]
    fn create_source_system_request_defaults_status_to_active() {
        let request: super::CreateSourceSystemRequest = serde_json::from_value(json!({
            "code": "ERP",
            "name": "ERP",
            "system_type": "ERP"
        }))
        .unwrap();
        let data = request.into_data();
        assert_eq!(data.status, SourceSystemStatus::Active);

        let request: super::CreateSourceSystemRequest = serde_json::from_value(json!({
            "code": "MALL",
            "name": "Mall",
            "system_type": "MALL",
            "status": "disabled"
        }))
        .unwrap();
        let data = request.into_data();
        assert_eq!(data.status, SourceSystemStatus::Disabled);
    }
}
