//! 当前名称搜索的两阶段 ID 投影与分批关联。

use std::collections::HashSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::PartyId;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

use super::{PARTIES, PARTY_REVISIONS};

/// 当前修订关联查询的每批 ID 上限；只限制命令大小，不截断搜索结果。
const REVISION_BATCH_SIZE: usize = 500;

/// 搜索关联只反序列化必要的业务主键，拒绝缺失或非文本 ID。
#[derive(Debug, Deserialize)]
struct IdRow {
    id: String,
}

/// 先完整读取名称命中的修订 ID，再沿同一执行器分批解析当前主体指针。
///
/// # 参数
/// * `db` - 主体领域数据库
/// * `keyword` - 字面量名称关键词
/// * `executor` - 调用方执行器；两个阶段之间不另开事务或合并快照
///
/// # 返回
/// 返回所有命中当前指针的未删除主体，按稳定 ID 排序去重。
///
/// # 错误
/// 任一查询、游标读取或必要 ID 反序列化失败时整体返回错误。
pub(super) async fn current_name_ids(
    db: &Database,
    keyword: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PartyId>> {
    let revisions = mongo_ops::find_many(
        &db.collection::<IdRow>(PARTY_REVISIONS),
        literal_name_filter(keyword),
        id_options(),
        executor,
    )
    .await?;
    let revision_ids = unique_revision_ids(revisions);
    let mut party_ids = Vec::new();
    for batch in revision_ids.chunks(REVISION_BATCH_SIZE) {
        let parties = mongo_ops::find_many(
            &db.collection::<IdRow>(PARTIES),
            current_revision_filter(batch),
            id_options(),
            executor,
        )
        .await?;
        party_ids.extend(parties.into_iter().map(|party| party.id));
    }
    Ok(sorted_party_ids(party_ids))
}

/// 保留名称或简称的字面量、大小写不敏感包含匹配与活跃修订过滤。
fn literal_name_filter(keyword: &str) -> Document {
    let escaped = regex::escape(keyword);
    doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$or": [
            { "legal_name": { "$regex": &escaped, "$options": "i" } },
            { "short_name": { "$regex": &escaped, "$options": "i" } },
        ]
    }
}

/// 当前主体只按当前修订指针命中，不追加原查询没有的修订归属条件。
fn current_revision_filter(revision_ids: &[String]) -> Document {
    doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "current_revision_id": { "$in": revision_ids },
    }
}

/// 两个阶段只读取业务主键，保留无排序、无限制的原游标查询方式。
fn id_options() -> FindOptions {
    FindOptions::builder().projection(doc! { "id": 1, "_id": 0 }).build()
}

/// 在分批前按首出现顺序去重历史修订 ID，不保留完整修订实体。
fn unique_revision_ids(revisions: Vec<IdRow>) -> Vec<String> {
    let mut seen = HashSet::with_capacity(revisions.len());
    revisions.into_iter().map(|revision| revision.id).filter(|id| seen.insert(id.clone())).collect()
}

/// 跨批次合并主体 ID，保留原稳定字符串排序及去重规则。
fn sorted_party_ids(mut party_ids: Vec<String>) -> Vec<PartyId> {
    party_ids.sort_unstable();
    party_ids.dedup();
    party_ids.into_iter().map(PartyId::new).collect()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, deserialize_from_document};
    use regex::RegexBuilder;

    use super::*;

    /// 从真实查询条件读取正则，验证元字符按字面量匹配且允许中间包含。
    #[test]
    fn current_name_search_preserves_literal_case_insensitive_contains() {
        let filter = literal_name_filter("A+b[1].(北)");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
        let predicates = filter.get_array("$or").unwrap();
        for (predicate, field) in predicates.iter().zip(["legal_name", "short_name"]) {
            let expression = predicate.as_document().unwrap().get_document(field).unwrap();
            assert_eq!(expression.get_str("$options").unwrap(), "i");
            let pattern = RegexBuilder::new(expression.get_str("$regex").unwrap())
                .case_insensitive(true)
                .build()
                .unwrap();
            assert!(pattern.is_match("前缀 a+B[1].(北) 后缀"));
            assert!(!pattern.is_match("前缀 aaab1X北 后缀"));
        }
    }

    /// 当前关联只保留活跃主体与当前指针，历史修订所属主体不参与筛选。
    #[test]
    fn current_name_search_filters_only_current_pointer_and_soft_delete() {
        assert_eq!(
            current_revision_filter(&["revision-b".into(), "revision-a".into()]),
            doc! {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "current_revision_id": { "$in": ["revision-b", "revision-a"] },
            }
        );
        assert_eq!(id_options().projection, Some(doc! { "id": 1, "_id": 0 }));
        assert_eq!(id_options().limit, None);
        assert_eq!(id_options().sort, None);
    }

    /// ID 投影允许省去业务资料，同时缺失或非文本主键仍失败。
    #[test]
    fn current_name_search_id_projection_rejects_invalid_identity() {
        assert_eq!(deserialize_from_document::<IdRow>(doc! { "id": "revision" }).unwrap().id, "revision");
        assert_eq!(deserialize_from_document::<IdRow>(doc! { "id": "" }).unwrap().id, "");
        assert!(deserialize_from_document::<IdRow>(doc! { "_id": "mongo-only" }).is_err());
        assert!(deserialize_from_document::<IdRow>(doc! { "id": Bson::Null }).is_err());
        assert!(deserialize_from_document::<IdRow>(doc! { "id": 3 }).is_err());
    }

    /// 超过一批的完整修订集合不会截断，重复项保留第一次出现的位置。
    #[test]
    fn current_name_search_batches_all_unique_revision_ids_without_truncation() {
        let rows = (0..=REVISION_BATCH_SIZE)
            .flat_map(|index| [IdRow { id: format!("revision-{index}") }, IdRow { id: "revision-0".into() }])
            .collect();
        let ids = unique_revision_ids(rows);
        assert_eq!(ids.len(), REVISION_BATCH_SIZE + 1);
        assert_eq!(ids.first().unwrap(), "revision-0");
        assert_eq!(ids.last().unwrap(), &format!("revision-{REVISION_BATCH_SIZE}"));
        assert_eq!(ids.chunks(REVISION_BATCH_SIZE).map(<[String]>::len).collect::<Vec<_>>(), [500, 1]);
        assert!(unique_revision_ids(Vec::new()).is_empty());
    }

    /// 主体结果跨批次去重，并按原字符串顺序输出完整结果。
    #[test]
    fn current_name_search_party_results_keep_sorted_union_and_empty_input() {
        let ids = sorted_party_ids(vec!["中".into(), "b".into(), "a".into(), "b".into()]);
        assert_eq!(ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>(), ["a", "b", "中"]);
        assert!(sorted_party_ids(Vec::new()).is_empty());
    }
}
