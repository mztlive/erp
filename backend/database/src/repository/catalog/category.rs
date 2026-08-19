use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, ProductCategory, ProductCategoryAttribute, ProductKind};

use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::sort_doc;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 商品分类列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductCategoryRow {
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
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品分类列表筛选条件（树形字典：支持按父节点与名称筛选）。
#[derive(Debug, Clone)]
pub struct ProductCategoryFilter {
    /// 分类代码精确匹配；`None` 表示不筛选。
    pub category_code: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 父分类筛选：`Some(Some(id))` 匹配该父节点的直接子节点，
    /// `Some(None)` 只匹配根节点，`None` 不筛选。
    pub parent_category_id: Option<Option<String>>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`category_code`/`name`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductCategoryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.category_code {
            filter.insert("category_code", code);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(parent) = &self.parent_category_id {
            filter.insert(
                "parent_category_id",
                match parent {
                    Some(id) => Bson::String(id.to_string()),
                    None => Bson::Null,
                },
            );
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductCategoryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductCategory> {
    /// 分页检索商品分类列表（投影查询）。
    ///
    /// 只返回 [`ProductCategoryRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单化（`created_at`/`category_code`/`name`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_product_categories(
        &self,
        filter: &ProductCategoryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductCategoryRow>> {
        let options = FindOptions::builder()
            .sort(product_category_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_category_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductCategoryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 查询指定父分类的直接子节点（投影行，按分类代码升序）。
    ///
    /// # 参数
    /// * `parent_category_id` - 父分类；`None` 表示查询根分类
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回子分类投影行集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_children(
        &self,
        parent_category_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryRow>> {
        let parents = parent_category_id
            .map(|id| vec![id.to_string()])
            .unwrap_or_default();
        self.find_children_of(&parents, executor).await
    }

    /// 层序批量查询一批父分类的直接子节点（`$in`，一次取回）。
    async fn find_children_of(
        &self,
        parent_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryRow>> {
        let filter = if parent_ids.is_empty() {
            doc! { "parent_category_id": null, "deleted_at": NOT_DELETED_TIMESTAMP_BSON }
        } else {
            doc! {
                "parent_category_id": { "$in": parent_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        };
        let options = FindOptions::builder()
            .sort(doc! { "category_code": 1 })
            .projection(product_category_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductCategoryRow>();
        mongo_ops::find_many(&collection, filter, options, executor).await
    }
}

/// 分类-属性适用关系列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductCategoryAttributeFilter {
    /// 商品分类；`None` 表示不筛选。
    pub category_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`sort_order`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductCategoryAttributeFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(category_id) = &self.category_id {
            filter.insert("category_id", category_id);
        }
        filter
    }
}

impl Pagination for ProductCategoryAttributeFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductCategoryAttribute> {}

/// 构建商品分类排序文档（白名单：`created_at`/`category_code`/`name`）。
///
/// # 参数
/// * `sort_by` - 排序字段；非法或缺失时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn product_category_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("category_code") => "category_code",
        Some("name") => "name",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 商品分类列表投影字段。
fn product_category_projection() -> Document {
    doc! {
        "id": 1,
        "category_code": 1,
        "parent_category_id": 1,
        "name": 1,
        "product_kind": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}
