//! 已绑定领域集合的 ID 查询和精确清理机械能力。

use mongodb::Collection;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::Repository;
use crate::{Error, Executor, Result, mongo_ops};

/// 关联查询只返回主键、可选父键和可选分类键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedId {
    pub id: String,
    pub parent: Option<String>,
    pub tag: Option<String>,
}

/// 由领域仓储派生的集合绑定，调用方不能指定任意集合。
pub struct IdRepository {
    collection: Collection<Document>,
}

impl<T: Serialize + DeserializeOwned + Send + Sync> Repository<'_, T> {
    /// 获取当前领域仓储的 ID 操作入口。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回绑定同一集合的操作入口。
    /// # 错误
    /// 无。
    pub fn ids(&self) -> IdRepository {
        IdRepository { collection: self.collection().clone_with_type() }
    }
}

impl IdRepository {
    /// 按指定字段精确匹配 ID 集合，保留软删除记录。
    ///
    /// # 参数
    /// * `field` - 外键字段
    /// * `values` - 精确匹配值；空集合不查询
    /// * `parent` - 可选父键投影字段
    /// * `tag` - 可选分类键投影字段
    /// * `executor` - 调用方事务执行器
    /// # 返回
    /// 返回匹配记录的 ID 及投影字段。
    /// # 错误
    /// 查询失败或文档缺少合法 ID 时返回错误。
    pub async fn linked(
        &self,
        field: &str,
        values: &[String],
        parent: Option<&str>,
        tag: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LinkedId>> {
        let mut projection = doc! {"id": 1, "_id": 0};
        for field in [parent, tag].into_iter().flatten() {
            projection.insert(field, 1);
        }
        let mut result = Vec::new();
        for chunk in values.chunks(200) {
            let docs = mongo_ops::find_many(
                &self.collection,
                doc! {field: {"$in": chunk}},
                FindOptions::builder().projection(projection.clone()).build(),
                executor,
            )
            .await?;
            for document in docs {
                result.push(linked_id(&document, parent, tag)?);
            }
        }
        Ok(result)
    }
}

/// 解析主键，异常数据不得被静默跳过。
fn linked_id(document: &Document, parent: Option<&str>, tag: Option<&str>) -> Result<LinkedId> {
    let id = document
        .get_str("id")
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::EntityMetadataOutOfRange("关联记录缺少有效 id"))?;
    let parent = parent.map(|field| required_text(document, field)).transpose()?;
    let tag = tag.map(|field| optional_text(document, field)).transpose()?.flatten();
    Ok(LinkedId { id: id.to_string(), parent, tag })
}

/// 父引用缺失时停止清理，避免遗留不可追踪的父单据。
fn required_text(document: &Document, field: &str) -> Result<String> {
    optional_text(document, field)?.ok_or(Error::EntityMetadataOutOfRange("关联记录缺少父 id"))
}

/// 可空分类字段允许缺省，但已存在的非法值必须报错。
fn optional_text(document: &Document, field: &str) -> Result<Option<String>> {
    match document.get(field) {
        None | Some(Bson::Null) => Ok(None),
        Some(Bson::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        _ => Err(Error::EntityMetadataOutOfRange("关联 id 字段类型或内容无效")),
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::linked_id;

    #[test]
    fn projection_preserves_identity_and_rejects_missing_ids() {
        let row = linked_id(&doc! {"id":"line", "parent":"order", "tag":"sku"}, Some("parent"), Some("tag"))
            .unwrap();
        assert_eq!(row.id, "line");
        assert_eq!(row.parent.as_deref(), Some("order"));
        assert_eq!(row.tag.as_deref(), Some("sku"));
        assert!(linked_id(&doc! {"_id":"mongo-only"}, None, None).is_err());
        assert!(linked_id(&doc! {"id":""}, None, None).is_err());
        assert!(linked_id(&doc! {"id":"line"}, Some("parent"), None).is_err());
        assert!(linked_id(&doc! {"id":"line", "sku_id":123}, None, Some("sku_id")).is_err());
        assert!(linked_id(&doc! {"id":"line", "sku_id":null}, None, Some("sku_id")).is_ok());
    }
}
