use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::sku_attribute::AttributeValueType;
use entities::catalog::{EnableStatus, SkuAttribute, SkuAttributeValue};
use entities::ids::SkuAttributeId;

use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::sort_doc;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuAttributeRow {
    /// 实体主键。
    pub id: String,
    /// 稳定属性代码。
    pub attribute_code: String,
    /// 属性名称。
    pub name: String,
    /// 属性值类型。
    pub value_type: AttributeValueType,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 规格属性列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuAttributeFilter {
    /// 属性代码精确匹配；`None` 表示不筛选。
    pub attribute_code: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 属性值类型；`None` 表示不筛选。
    pub value_type: Option<AttributeValueType>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`attribute_code`/`name`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuAttributeFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.attribute_code {
            filter.insert("attribute_code", code);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(value_type) = self.value_type {
            filter.insert("value_type", value_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuAttributeFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuAttribute> {
    /// 分页检索规格属性列表（投影查询）。
    ///
    /// 只返回 [`SkuAttributeRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`attribute_code`/`name`）。
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
    pub async fn search_sku_attributes(
        &self,
        filter: &SkuAttributeFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuAttributeRow>> {
        let options = FindOptions::builder()
            .sort(sku_attribute_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_attribute_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuAttributeRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 规格属性值列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuAttributeValueRow {
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
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 规格属性值列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuAttributeValueFilter {
    /// 所属规格属性；`None` 表示不筛选。
    pub attribute_id: Option<String>,
    /// 属性值代码精确匹配；`None` 表示不筛选。
    pub value_code: Option<String>,
    /// 展示值字面量正则（忽略大小写）；`None` 表示不筛选。
    pub display_value: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`value_code`/`display_value`/`sort_order`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuAttributeValueFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(attribute_id) = &self.attribute_id {
            filter.insert("attribute_id", attribute_id);
        }
        if let Some(code) = &self.value_code {
            filter.insert("value_code", code);
        }
        insert_literal_regex_filter(&mut filter, "display_value", self.display_value.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuAttributeValueFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuAttributeValue> {
    /// 分页检索规格属性值列表（投影查询）。
    ///
    /// 只返回 [`SkuAttributeValueRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`value_code`/`display_value`/`sort_order`）。
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
    pub async fn search_sku_attribute_values(
        &self,
        filter: &SkuAttributeValueFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuAttributeValueRow>> {
        let options = FindOptions::builder()
            .sort(sku_attribute_value_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_attribute_value_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuAttributeValueRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量查询一组规格属性下的全部属性值（`$in`，一次取回）。
    ///
    /// 用于字典下拉与跨属性取值组装，避免逐属性 N+1。
    ///
    /// # 参数
    /// * `attribute_ids` - 规格属性 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除属性值实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_attribute_ids(
        &self,
        attribute_ids: &[SkuAttributeId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuAttributeValue>> {
        if attribute_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = attribute_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "attribute_id": { "$in": ids } }, executor)
            .await
    }
}

/// 构建规格属性排序文档（白名单：`created_at`/`attribute_code`/`name`）。
fn sku_attribute_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("attribute_code") => "attribute_code",
        Some("name") => "name",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建规格属性值排序文档（白名单：`created_at`/`value_code`/`display_value`/`sort_order`）。
fn sku_attribute_value_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("value_code") => "value_code",
        Some("display_value") => "display_value",
        Some("sort_order") => "sort_order",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 规格属性列表投影字段。
fn sku_attribute_projection() -> Document {
    doc! {
        "id": 1,
        "attribute_code": 1,
        "name": 1,
        "value_type": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 规格属性值列表投影字段。
fn sku_attribute_value_projection() -> Document {
    doc! {
        "id": 1,
        "attribute_id": 1,
        "value_code": 1,
        "display_value": 1,
        "sort_order": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::catalog::sku_attribute::AttributeValueType;
    use entities::catalog::EnableStatus;

    #[test]
    fn sku_attribute_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SkuAttributeFilter {
            attribute_code: Some("SIZE".to_string()),
            name: Some("尺".to_string()),
            value_type: Some(AttributeValueType::Enum),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("attribute_code").unwrap(), "SIZE");
        assert_eq!(document.get_str("value_type").unwrap(), "enum");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sku_attribute_value_filter_applies_regex_and_attribute_scoping() {
        let filter = SkuAttributeValueFilter {
            attribute_id: Some("attr-1".to_string()),
            value_code: Some("L".to_string()),
            display_value: Some("大号".to_string()),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("attribute_id").unwrap(), "attr-1");
        assert_eq!(document.get_str("value_code").unwrap(), "L");
        assert!(document.get("display_value").is_some());
        assert!(document.get("status").is_none());
    }
}
