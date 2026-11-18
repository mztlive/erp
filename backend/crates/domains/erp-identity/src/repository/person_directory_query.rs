//! 人员目录数据库查询：授权 ID 索引、资格关联、搜索及分页均先于响应。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, aggregate};
use serde::Deserialize;

use crate::dto::PersonDirectoryItem;
use crate::entity::authorization::{QueryModel, QueryOutput};
use crate::entity::person_directory::PersonDirectoryCategory;
use crate::ports::QueryFragment;
use crate::repository::organization::primary_membership_query;
use crate::{AccessControlExt, CompiledAccess, Error, Result};

/// 单次候选搜索的可验证版本索引上限；超出时整体拒绝，不返回截断目录。
pub const DIRECTORY_LIMIT: usize = 10_000;

/// 服务层已解析的目录读取条件。
pub struct DirectoryRead<'a> {
    pub category: PersonDirectoryCategory,
    pub org_unit_ids: Option<&'a [String]>,
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
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<DirectoryResult> {
    ensure_model(access.model(), read.category)?;
    access.ensure_current(executor)?;

    let collection = db.collection::<DirectoryResult>("accounts");
    let mut pipeline = access.stages().to_vec();
    pipeline.extend(directory_pipeline(read, access.as_of())?);
    let result = aggregate::rows(&collection, pipeline, executor).await?.pop().unwrap_or_default();

    if result.versions.len() > DIRECTORY_LIMIT {
        return Err(Error::ValidationError("人员目录超过查询上限，请收窄组织或搜索条件".into()));
    }
    Ok(result)
}

/// 目录只能消费对应固定资源的完整账号查询，不能将份额凭证用于人员详情。
fn ensure_model(model: &QueryModel, category: PersonDirectoryCategory) -> Result<()> {
    if model.resource != category.resource()
        || model.collection != "accounts"
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(Error::ValidationError("人员目录授权模型不匹配".into()));
    }
    Ok(())
}

/// 生成授权、资格和分页流水线，回显 ID 在资格关联前限定。
fn directory_pipeline(read: &DirectoryRead<'_>, as_of: Instant) -> Result<Vec<Document>> {
    let mut filters =
        vec![doc! { "kind": AccountKind::Admin.as_str(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON }];
    if let Some(ids) = read.selected_ids {
        filters.push(doc! { "id": { "$in": ids } });
    }
    if let Some(search) = read.search {
        let escaped = regex::escape(search);
        filters.push(doc! { "$or": [{ "name": { "$regex": &escaped, "$options": "i" } }, { "account": { "$regex": escaped, "$options": "i" } }] });
    }
    let mut pipeline = vec![doc! { "$match": { "$and": filters } }];
    if let Some(ids) = read.org_unit_ids {
        pipeline.extend(organization_filter(ids, as_of)?);
    }
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

/// 请求组织条件只收窄中央授权管道，在分页和计数前匹配真实有效主属关系。
fn organization_filter(ids: &[String], as_of: Instant) -> Result<Vec<Document>> {
    let ids = ids.iter().cloned().collect();
    let QueryFragment::Exists { collection, bindings, pipeline } =
        primary_membership_query("$id".into(), Some(&ids), as_of)
    else {
        return Err(Error::ValidationError("目录组织筛选缺少主属关系查询".into()));
    };
    Ok(vec![
        doc! { "$lookup": { "from": collection, "let": bindings, "pipeline": pipeline, "as": "__directory_filter" } },
        doc! { "$match": { "__directory_filter.0": { "$exists": true } } },
        doc! { "$unset": "__directory_filter" },
    ])
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
    use crate::entity::authorization::QueryModelKey;

    #[test]
    fn directory_rejects_other_resources_collections_and_partial_output() {
        let model = QueryModel {
            key: QueryModelKey::parse("directory_sales_person").unwrap(),
            resource: "sales_person".into(),
            collection: "accounts".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        };
        assert!(ensure_model(&model, PersonDirectoryCategory::Sales).is_ok());
        assert!(ensure_model(&model, PersonDirectoryCategory::Procurement).is_err());

        let wrong_collection = QueryModel { collection: "other_accounts".into(), ..model.clone() };
        assert!(ensure_model(&wrong_collection, PersonDirectoryCategory::Sales).is_err());

        let partial = QueryModel { output: QueryOutput::Existence { fields: ["id".into()].into() }, ..model };
        assert!(ensure_model(&partial, PersonDirectoryCategory::Sales).is_err());
    }

    #[test]
    fn selected_ids_precede_qualification_lookup_and_business_needs_no_role() {
        let ids = vec!["u1".into()];
        let mut read = DirectoryRead {
            category: PersonDirectoryCategory::Sales,
            org_unit_ids: None,
            selected_ids: Some(&ids),
            search: Some("a.*"),
            page: 1,
            page_size: 20,
        };
        let sales = directory_pipeline(&read, Instant::from_unix_secs(1)).unwrap();
        let matches = sales[0].get_document("$match").unwrap().get_array("$and").unwrap();
        assert_eq!(matches[1], doc! { "id": { "$in": &ids } }.into());
        assert!(sales[1].contains_key("$lookup"));

        read.category = PersonDirectoryCategory::Business;
        let business = directory_pipeline(&read, Instant::from_unix_secs(1)).unwrap();
        assert_eq!(business.len(), 2);
        assert!(business[1].contains_key("$facet"));
        assert!(!business[0].to_string().contains("status"));
    }
}
