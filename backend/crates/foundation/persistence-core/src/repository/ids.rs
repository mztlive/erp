//! 已绑定领域集合的关联 ID 查询。

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
    /// 需要扫描关联标识而不加载整个实体时，从领域仓储派生绑定同一集合的轻量查询入口。
    pub fn ids(&self) -> IdRepository {
        IdRepository { collection: self.collection().clone_with_type() }
    }
}

impl IdRepository {
    /// 清理等需追踪历史关联的场景，分批按外键精确匹配并保留软删除记录，只投影业务 ID 和请求的父键、分类键。
    /// 空输入不查询；主键无效、要求的父键缺失或投影值格式非法时拒绝，不悄悄跳过记录。
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

// 把投影结果还原为主键、父键和分类键供清理流程使用；不跳过缺少有效业务 ID 的记录。
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

// 请求了父引用就要求有有效值；缺失时停止清理，避免留下无法追踪的父单据。
fn required_text(document: &Document, field: &str) -> Result<String> {
    optional_text(document, field)?.ok_or(Error::EntityMetadataOutOfRange("关联记录缺少父 id"))
}

// 读取投影的可空分类或父键时允许缺省及 `null`，但字段存在且不是有效非空白字符串则报错。
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
