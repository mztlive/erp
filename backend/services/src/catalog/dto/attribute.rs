use entities::catalog::{EnableStatus, SkuAttribute, SkuAttributeValue};
use entities::ids::SkuAttributeId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 规格属性列表允许的排序字段白名单。
pub(crate) const SKU_ATTRIBUTE_SORT_FIELDS: &[&str] = &["created_at", "attribute_code", "name"];
/// 规格属性值列表允许的排序字段白名单。
pub(crate) const SKU_ATTRIBUTE_VALUE_SORT_FIELDS: &[&str] =
    &["created_at", "value_code", "display_value", "sort_order"];
/// 规格属性创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSkuAttributeRequest {
    /// 稳定属性代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "属性代码不能为空"))]
    pub attribute_code: String,
    /// 属性名称。
    #[validate(custom(function = "non_blank", message = "属性名称不能为空"))]
    pub name: String,
    /// 属性值类型（受控枚举或规范文本）。
    pub value_type: entities::catalog::sku_attribute::AttributeValueType,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 规格属性更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSkuAttributeRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 属性名称；缺省表示不修改。
    pub name: Option<String>,
    /// 属性值类型；缺省表示不修改。
    pub value_type: Option<entities::catalog::sku_attribute::AttributeValueType>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 规格属性响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuAttributeView {
    /// 实体主键。
    pub id: String,
    /// 稳定属性代码。
    pub attribute_code: String,
    /// 属性名称。
    pub name: String,
    /// 属性值类型。
    pub value_type: entities::catalog::sku_attribute::AttributeValueType,
    /// 启停状态。
    pub status: EnableStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<SkuAttribute> for SkuAttributeView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `attribute` - 规格属性实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(attribute: SkuAttribute) -> Self {
        Self {
            id: attribute.base.id,
            attribute_code: attribute.attribute_code,
            name: attribute.name,
            value_type: attribute.value_type,
            status: attribute.stable.status,
            created_at: attribute.base.created_at,
            version: attribute.base.version,
        }
    }
}

/// 规格属性列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuAttributeListParams {
    /// 属性代码精确筛选。
    pub attribute_code: Option<String>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 属性值类型筛选。
    pub value_type: Option<entities::catalog::sku_attribute::AttributeValueType>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`attribute_code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的规格属性列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuAttributeListQuery {
    /// 属性代码精确筛选。
    pub attribute_code: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 属性值类型筛选。
    pub value_type: Option<entities::catalog::sku_attribute::AttributeValueType>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuAttributeListParams {
    /// 归一化规格属性列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuAttributeListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SKU_ATTRIBUTE_SORT_FIELDS)?;
        Ok(SkuAttributeListQuery {
            attribute_code: normalized_text(self.attribute_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
            value_type: self.value_type,
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

/// 规格属性值创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSkuAttributeValueRequest {
    /// 所属规格属性。
    pub attribute_id: SkuAttributeId,
    /// 稳定属性值代码（同一属性下唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "属性值代码不能为空"))]
    pub value_code: String,
    /// 展示值。
    #[validate(custom(function = "non_blank", message = "展示值不能为空"))]
    pub display_value: String,
    /// 展示排序（非负）。
    #[validate(range(min = 0, message = "展示排序不能为负数"))]
    pub sort_order: i32,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 规格属性值更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSkuAttributeValueRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 展示值；缺省表示不修改。
    pub display_value: Option<String>,
    /// 展示排序；缺省表示不修改。
    pub sort_order: Option<i32>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 规格属性值响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuAttributeValueView {
    /// 实体主键。
    pub id: String,
    /// 所属规格属性。
    pub attribute_id: String,
    /// 稳定属性值代码。
    pub value_code: String,
    /// 展示值。
    pub display_value: String,
    /// 展示排序。
    pub sort_order: i32,
    /// 启停状态。
    pub status: EnableStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<SkuAttributeValue> for SkuAttributeValueView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `value` - 规格属性值实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(value: SkuAttributeValue) -> Self {
        Self {
            id: value.base.id,
            attribute_id: value.attribute_id.to_string(),
            value_code: value.value_code,
            display_value: value.display_value,
            sort_order: value.sort_order,
            status: value.stable.status,
            created_at: value.base.created_at,
            version: value.base.version,
        }
    }
}

/// 规格属性值列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuAttributeValueListParams {
    /// 所属规格属性筛选。
    pub attribute_id: Option<SkuAttributeId>,
    /// 属性值代码精确筛选。
    pub value_code: Option<String>,
    /// 展示值字面量筛选（忽略大小写）。
    pub display_value: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`value_code`/`display_value`/`sort_order`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的规格属性值列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuAttributeValueListQuery {
    /// 所属规格属性筛选。
    pub attribute_id: Option<String>,
    /// 属性值代码精确筛选。
    pub value_code: Option<String>,
    /// 展示值筛选。
    pub display_value: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuAttributeValueListParams {
    /// 归一化规格属性值列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuAttributeValueListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SKU_ATTRIBUTE_VALUE_SORT_FIELDS)?;
        Ok(SkuAttributeValueListQuery {
            attribute_id: self.attribute_id.as_ref().map(|id| id.to_string()),
            value_code: normalized_text(self.value_code.as_deref()),
            display_value: normalized_text(self.display_value.as_deref()),
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
