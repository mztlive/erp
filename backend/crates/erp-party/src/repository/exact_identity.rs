//! 合同识别按法定全称或信用代码查当前主体；有界结果保留歧义及停用身份。
use std::collections::HashSet;

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

use crate::{Party, PartyExt};

/// 足以区分唯一身份与歧义的候选上限；不作为完整主体目录。
const IDENTITY_CANDIDATE_LIMIT: i64 = 3;

/// 合同身份候选保留主体角色、启停及软删除事实，禁止据此自动创建重复主体。
#[derive(Deserialize)]
pub struct ContractIdentityCandidate {
    /// 主体稳定身份，包含我方角色及信用代码。
    pub party: Party,
    /// 我方角色法定名称，或归属正确的当前修订法定名称；缺失时须人工修复。
    pub legal_name: Option<String>,
}

/// 查找名称或信用代码命中的企业主体，包括停用、软删除及我方角色。
///
/// 两条索引查询沿同一执行器读取；保留冲突候选，不以可用性过滤冒充不存在。
/// # 参数
/// * `db` / `name` - 主体数据库及已规范化的法定全称。
/// * `credit` - 已去首尾空白并转大写的非空信用代码；缺失时只匹配名称。
/// * `executor` - 调用方执行器，确认导入时应使用写事务的同一执行器。
/// # 返回
/// 最多三个不同的企业主体；只有零候选才能按不存在处理。
/// # 错误
/// 任一查询、游标读取或必要身份反序列化失败时返回错误。
pub async fn contract_identity_candidates(
    db: &Database,
    name: &str,
    credit: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<ContractIdentityCandidate>> {
    let revisions = db.collection::<Document>(<Database as PartyExt>::PARTY_REVISIONS);
    let result = candidate_rows(&revisions, current_name_pipeline(name), executor).await?;
    let parties = db.collection::<Document>(<Database as PartyExt>::PARTIES);
    let direct = candidate_rows(&parties, stable_identity_pipeline(name, credit), executor).await?;
    Ok(merge_candidates(result, direct))
}

/// 同一主体名称、信用代码双重命中只计一次，保留不同主体形成的歧义。
fn merge_candidates(
    named: Vec<ContractIdentityCandidate>,
    direct: Vec<ContractIdentityCandidate>,
) -> Vec<ContractIdentityCandidate> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for candidate in named.into_iter().chain(direct) {
        if seen.insert(candidate.party.base.id.clone()) {
            result.push(candidate);
            if result.len() == 3 {
                break;
            }
        }
    }
    result
}

/// 当前法定名称从修订名称索引进入，只保留当前指针及真实主体归属。
fn current_name_pipeline(name: &str) -> Vec<Document> {
    vec![
        doc! { "$match": { "legal_name": name } },
        doc! { "$lookup": { "from": <Database as PartyExt>::PARTIES, "localField": "id", "foreignField": "current_revision_id", "as": "party" } },
        doc! { "$unwind": "$party" },
        doc! { "$match": { "party.party_kind": "enterprise", "$expr": { "$eq": ["$party_id", "$party.id"] } } },
        doc! { "$project": { "_id": 0, "party": 1, "legal_name": { "$ifNull": [
            "$party.company_profile.legal_name",
            { "$cond": [{ "$eq": ["$deleted_at", 0_i64] }, "$legal_name", null] }
        ] } } },
        doc! { "$limit": IDENTITY_CANDIDATE_LIMIT },
    ]
}

/// 我方名称或信用代码从稳定身份索引进入，再按主体归属读取当前修订。
fn stable_identity_pipeline(name: &str, credit: Option<&str>) -> Vec<Document> {
    vec![
        doc! { "$match": stable_identity_filter(name, credit) },
        doc! { "$limit": IDENTITY_CANDIDATE_LIMIT },
        doc! { "$lookup": {
            "from": <Database as PartyExt>::PARTY_REVISIONS,
            "let": { "party_id": "$id", "revision_id": "$current_revision_id" },
            "pipeline": [{ "$match": { "$expr": { "$and": [
                { "$eq": ["$party_id", "$$party_id"] }, { "$eq": ["$id", "$$revision_id"] }
            ] } } }],
            "as": "revision"
        } },
        doc! { "$unwind": { "path": "$revision", "preserveNullAndEmptyArrays": true } },
        doc! { "$project": { "_id": 0, "party": "$$ROOT", "legal_name": { "$ifNull": [
            "$company_profile.legal_name",
            { "$cond": [{ "$eq": ["$revision.deleted_at", 0_i64] }, "$revision.legal_name", null] }
        ] } } },
    ]
}

/// 信用代码与名称采用或条件，保留同名异码和同码异名冲突。
fn stable_identity_filter(name: &str, credit: Option<&str>) -> Document {
    let mut identities = vec![doc! { "company_profile.legal_name": name }];
    if let Some(code) = credit {
        identities.push(doc! { "unified_credit_code": code });
    }
    doc! { "party_kind": "enterprise", "$or": identities }
}

/// 显式使用调用方执行器，禁止在有事务的导入中另行读取主体。
async fn candidate_rows(
    collection: &Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<ContractIdentityCandidate>> {
    let mut result = Vec::new();
    if let Some(session) = executor.session() {
        let mut cursor = collection
            .aggregate(pipeline)
            .with_type::<ContractIdentityCandidate>()
            .session(&mut *session)
            .await?;
        while cursor.advance(session).await? {
            result.push(cursor.deserialize_current()?);
        }
    } else {
        let mut cursor = collection.aggregate(pipeline).with_type::<ContractIdentityCandidate>().await?;
        while cursor.advance().await? {
            result.push(cursor.deserialize_current()?);
        }
    }
    Ok(result)
}

/// 精确查询启用的当前法定名称，最多返回两个候选以检测歧义。
/// # 参数
/// * `db` / `name` / `company` / `executor` - 数据库、法定全称、我方角色及事务。
/// # 返回
/// 零至两个当前有效主体。
/// # 错误
/// 数据库读取或反序列化失败。
pub async fn exact_parties(
    db: &Database,
    name: &str,
    company: bool,
    credit: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<Party>> {
    if company {
        let mut filter = doc! { "company_profile.legal_name": name, "status": "active", "deleted_at": 0_i64 };
        if let Some(code) = credit {
            filter.insert("unified_credit_code", code);
        }
        return mongo_ops::find_many(
            &db.parties().collection(),
            filter,
            FindOptions::builder().limit(2).build(),
            executor,
        )
        .await;
    }
    let mut pipeline = vec![
        doc! { "$match": { "legal_name": name, "deleted_at": 0_i64 } },
        doc! { "$lookup": { "from": <Database as PartyExt>::PARTIES, "localField": "id", "foreignField": "current_revision_id", "as": "party" } },
        doc! { "$unwind": "$party" },
        doc! { "$match": { "party.status": "active", "party.deleted_at": 0_i64, "$expr": { "$eq": ["$party_id", "$party.id"] } } },
        doc! { "$replaceRoot": { "newRoot": "$party" } },
    ];
    if let Some(code) = credit {
        pipeline.push(doc! { "$match": { "unified_credit_code": code } });
    }
    pipeline.push(doc! { "$limit": 2 });
    let collection = db.collection::<Document>(<Database as PartyExt>::PARTY_REVISIONS);
    let mut result = Vec::new();
    if let Some(session) = executor.session() {
        let mut cursor = collection.aggregate(pipeline).with_type::<Party>().session(&mut *session).await?;
        while cursor.advance(session).await? {
            result.push(cursor.deserialize_current()?);
        }
    } else {
        let mut cursor = collection.aggregate(pipeline).with_type::<Party>().await?;
        while cursor.advance().await? {
            result.push(cursor.deserialize_current()?);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{deserialize_from_document, serialize_to_document};

    use super::*;
    use crate::{PartyData, PartyId, PartyKind, PartyStatus};

    fn candidate(id: &str, name: Option<&str>) -> ContractIdentityCandidate {
        let party = Party::new(
            PartyId::new(id),
            PartyData {
                party_no: id.into(),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: Some("91310000123456789A".into()),
                status: PartyStatus::Disabled,
            },
            "admin",
        )
        .unwrap();
        ContractIdentityCandidate { party, legal_name: name.map(str::to_owned) }
    }

    #[test]
    fn identity_filter_matches_name_or_credit_without_availability_filter() {
        assert_eq!(
            stable_identity_filter("客户有限公司", Some("91310000123456789A")),
            doc! {
                "party_kind": "enterprise",
                "$or": [
                    { "company_profile.legal_name": "客户有限公司" },
                    { "unified_credit_code": "91310000123456789A" },
                ],
            }
        );
        assert_eq!(
            stable_identity_filter("客户有限公司", None),
            doc! { "party_kind": "enterprise", "$or": [{ "company_profile.legal_name": "客户有限公司" }] }
        );
    }

    #[test]
    fn merge_collapses_same_identity_and_retains_conflicting_identity() {
        let result = merge_candidates(
            vec![candidate("same", Some("客户有限公司")), candidate("same", Some("客户有限公司"))],
            vec![candidate("same", Some("客户有限公司")), candidate("other", Some("另一个有限公司"))],
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].party.base.id, "same");
        assert_eq!(result[1].legal_name.as_deref(), Some("另一个有限公司"));
        assert_eq!(result[1].party.stable.status, PartyStatus::Disabled);
        assert!(merge_candidates(Vec::new(), Vec::new()).is_empty());
    }

    #[test]
    fn merge_bounds_unique_results_without_hiding_ambiguity() {
        let result = merge_candidates(
            vec![candidate("one", Some("客户有限公司")), candidate("two", Some("客户有限公司"))],
            vec![candidate("three", None), candidate("four", None)],
        );
        assert_eq!(result.len(), 3);
        assert_eq!(result[2].party.base.id, "three");
        assert!(result[2].legal_name.is_none());
    }

    #[test]
    fn candidate_retains_soft_deleted_identity_and_missing_name() {
        let mut candidate = candidate("deleted", None);
        candidate.party.base.deleted_at = 1;
        let row = doc! { "party": serialize_to_document(&candidate.party).unwrap(), "legal_name": null };
        let decoded = deserialize_from_document::<ContractIdentityCandidate>(row).unwrap();
        assert_eq!(decoded.party.base.deleted_at, 1);
        assert_eq!(decoded.party.stable.status, PartyStatus::Disabled);
        assert_eq!(decoded.party.unified_credit_code.as_deref(), Some("91310000123456789A"));
        assert!(decoded.legal_name.is_none());
    }
}
