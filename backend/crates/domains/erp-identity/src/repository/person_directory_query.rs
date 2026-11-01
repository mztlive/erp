//! 人员目录数据库查询：授权 ID 索引、资格关联、搜索及分页均先于响应。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::AccountKind;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Error as PersistenceError, Executor};
use serde::Deserialize;

use crate::dto::PersonDirectoryItem;
use crate::entity::person_directory::PersonDirectoryCategory;
use crate::{AccessControlExt, Error, Result};

/// 单次候选搜索的可验证版本索引上限；超出时整体拒绝，不返回截断目录。
pub const DIRECTORY_LIMIT: usize = 10_000;

/// 服务层已解析的目录读取条件。
pub struct DirectoryRead<'a> {
    pub category: PersonDirectoryCategory,
    pub authorized_ids: Option<&'a [String]>,
    pub selected_ids: Option<&'a [String]>,
    pub search: Option<&'a str>,
    pub page: u64,
    pub page_size: u32,
}

/// 一个数据库快照内的页、总数与有限内容版本索引。
#[derive(Default, Deserialize)]
pub struct DirectoryResult {
    pub items: Vec<PersonDirectoryItem>,
    pub totals: Vec<DirectoryTotal>,
    pub versions: Vec<Document>,
}

/// 与同一查询条件对应的总数。
#[derive(Deserialize)]
pub struct DirectoryTotal {
    pub count: i64,
}

/// 同事务执行目录聚合。
///
/// # 参数
/// `read` 为已经完成范围解析的条件，`executor` 为用例事务。
/// # 返回
/// 分页、总数及不含账号凭证的内容版本索引。
/// # 错误
/// 数据库读取或版本索引超过上限时拒绝，不返回部分结果。
pub async fn directory_page(
    db: &Database,
    read: &DirectoryRead<'_>,
    executor: &mut dyn Executor,
) -> Result<DirectoryResult> {
    let collection = db.collection::<Document>("accounts");
    let pipeline = directory_pipeline(read)?;
    let result = if let Some(session) = executor.session() {
        let mut cursor = collection
            .aggregate(pipeline)
            .with_type::<DirectoryResult>()
            .session(&mut *session)
            .await
            .map_err(PersistenceError::from)?;
        if cursor.advance(session).await.map_err(PersistenceError::from)? {
            cursor.deserialize_current().map_err(PersistenceError::from)?
        } else {
            DirectoryResult::default()
        }
    } else {
        let mut cursor = collection
            .aggregate(pipeline)
            .with_type::<DirectoryResult>()
            .await
            .map_err(PersistenceError::from)?;
        if cursor.advance().await.map_err(PersistenceError::from)? {
            cursor.deserialize_current().map_err(PersistenceError::from)?
        } else {
            DirectoryResult::default()
        }
    };
    if result.versions.len() > DIRECTORY_LIMIT {
        return Err(Error::ValidationError("人员目录超过查询上限，请收窄组织或搜索条件".into()));
    }
    Ok(result)
}

/// 生成授权、资格和分页流水线，回显 ID 在资格关联前限定。
fn directory_pipeline(read: &DirectoryRead<'_>) -> Result<Vec<Document>> {
    let mut filters =
        vec![doc! { "kind": AccountKind::Admin.as_str(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON }];
    if let Some(ids) = read.authorized_ids {
        filters.push(doc! { "id": { "$in": ids } });
    }
    if let Some(ids) = read.selected_ids {
        filters.push(doc! { "id": { "$in": ids } });
    }
    if let Some(search) = read.search {
        let escaped = regex::escape(search);
        filters.push(doc! { "$or": [{ "name": { "$regex": &escaped, "$options": "i" } }, { "account": { "$regex": escaped, "$options": "i" } }] });
    }
    let mut pipeline = vec![doc! { "$match": { "$and": filters } }];
    if read.category != PersonDirectoryCategory::Business {
        pipeline.extend(qualification_stages(read.category));
    }
    let skip = read
        .page
        .saturating_sub(1)
        .checked_mul(u64::from(read.page_size))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| Error::ValidationError("页码超过查询上限".into()))?;
    pipeline.push(doc! { "$facet": {
        "items": [{ "$sort": { "name": 1, "id": 1 } }, { "$skip": skip }, { "$limit": i64::from(read.page_size) },
            { "$project": { "_id": 0, "id": 1, "name": 1, "account": 1, "status": 1 } }],
        "totals": [{ "$count": "count" }],
        "versions": [{ "$sort": { "id": 1 } }, { "$limit": 10001 },
            { "$project": { "_id": 0, "id": 1, "version": 1, "name": 1, "account": 1, "status": 1, "query_qualification.version": 1 } }]
    } });
    Ok(pipeline)
}

/// 在身份域内部按账号索引关联资格；后台目录无需此关联。
fn qualification_stages(category: PersonDirectoryCategory) -> Vec<Document> {
    vec![
        doc! { "$lookup": { "from": <Database as AccessControlExt>::PERSON_QUERY_QUALIFICATIONS,
        "localField": "id", "foreignField": "account_id", "pipeline": [
            { "$match": { "category": category.as_str(), "status": "active", "deleted_at": NOT_DELETED_TIMESTAMP_BSON } },
            { "$project": { "_id": 0, "version": 1 } }
        ], "as": "query_qualification" } },
        doc! { "$match": { "query_qualification.0": { "$exists": true } } },
    ]
}

/// 构造内容版本测试数据；Mongo 文档构造保留在仓储边界内。
#[cfg(test)]
pub(crate) fn version_fixture(name: &str, status: &str) -> DirectoryResult {
    DirectoryResult {
        versions: vec![doc! { "id": "a", "name": name, "status": status }],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_ids_precede_qualification_lookup_and_business_needs_no_role() {
        let ids = vec!["u1".into()];
        let mut read = DirectoryRead {
            category: PersonDirectoryCategory::Sales,
            authorized_ids: Some(&ids),
            selected_ids: Some(&ids),
            search: Some("a.*"),
            page: 1,
            page_size: 20,
        };
        let sales = directory_pipeline(&read).unwrap();
        let matches = sales[0].get_document("$match").unwrap().get_array("$and").unwrap();
        assert_eq!(matches[1], doc! { "id": { "$in": &ids } }.into());
        assert_eq!(matches[2], doc! { "id": { "$in": &ids } }.into());
        assert!(sales[1].contains_key("$lookup"));
        read.category = PersonDirectoryCategory::Business;
        let business = directory_pipeline(&read).unwrap();
        assert_eq!(business.len(), 2);
        assert!(business[1].contains_key("$facet"));
        assert!(!business[0].to_string().contains("status"));
    }
}
