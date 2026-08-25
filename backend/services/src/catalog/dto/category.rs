use entities::catalog::{EnableStatus, ProductCategory, ProductKind};
use entities::ids::ProductCategoryId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 商品分类列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const PRODUCT_CATEGORY_SORT_FIELDS: &[&str] = &["created_at", "category_code", "name"];
/// 商品分类创建请求（HTTP 契约：`{ category_code, parent_category_id?, name,
/// product_kind, status? }`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductCategoryRequest {
    /// 稳定分类代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "分类代码不能为空"))]
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<ProductCategoryId>,
    /// 分类名称。
    #[validate(custom(function = "non_blank", message = "分类名称不能为空"))]
    pub name: String,
    /// 分类允许的商品类型（只用于兼容性校验和筛选）。
    pub product_kind: ProductKind,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 商品分类更新请求（携带乐观锁版本；`category_code` 与 `parent_category_id`
/// 不可在通用更新中修改，父分类移动走专门接口）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateProductCategoryRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 分类名称；缺省表示不修改。
    pub name: Option<String>,
    /// 分类允许的商品类型；缺省表示不修改。
    pub product_kind: Option<ProductKind>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
    /// 可选父级变更；存在时与名称、类型、状态在同一事务提交。
    pub parent_change: Option<ProductCategoryParentChange>,
}

/// 商品分类更新命令中的父级变更。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductCategoryParentChange {
    /// 新父分类；空表示提升为根分类。
    pub parent_category_id: Option<ProductCategoryId>,
}

/// 移动商品分类到新父分类请求（树形维护：只允许移动叶节点或整棵子树）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MoveProductCategoryRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 新父分类；空表示提升为根分类。
    pub parent_category_id: Option<ProductCategoryId>,
}

/// 商品分类响应视图（契约形状：`id`/`category_code`/`parent_category_id`/
/// `name`/`product_kind`/`status`/`created_at`，另附 `version`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductCategoryView {
    /// 实体主键。
    pub id: String,
    /// 稳定分类代码。
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<String>,
    /// 分类名称。
    pub name: String,
    /// 分类允许的商品类型。
    pub product_kind: ProductKind,
    /// 启停状态。
    pub status: EnableStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
}

impl From<ProductCategory> for ProductCategoryView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `category` - 商品分类实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(category: ProductCategory) -> Self {
        Self {
            id: category.base.id,
            category_code: category.category_code,
            parent_category_id: category.parent_category_id.map(|id| id.to_string()),
            name: category.name,
            product_kind: category.product_kind,
            status: category.stable.status,
            created_at: category.base.created_at,
            version: category.base.version,
        }
    }
}

/// 商品分类列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductCategoryListParams {
    /// 分类代码精确筛选。
    pub category_code: Option<String>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 父分类筛选：`root` 只匹配根分类；传入分类 ID 匹配其直接子节点。
    pub parent_category_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`category_code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商品分类列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductCategoryListQuery {
    /// 分类代码精确筛选。
    pub category_code: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 父分类筛选（`None` 不筛选；`Some(None)` 根；`Some(Some(id))` 直接子节点）。
    pub parent_category_id: Option<Option<String>>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductCategoryListParams {
    /// 归一化商品分类列表查询参数。
    ///
    /// 文本筛选去首尾空白、`root` 特判根分类、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductCategoryListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PRODUCT_CATEGORY_SORT_FIELDS)?;
        let parent_category_id = match self
            .parent_category_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some("root") => Some(None),
            Some(id) => Some(Some(id.to_string())),
            None => None,
        };
        Ok(ProductCategoryListQuery {
            category_code: normalized_text(self.category_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
            parent_category_id,
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
