//! 合同识别按法定全称查当前主体；有界结果保留歧义，禁止别名猜测。
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};

use crate::{Party, PartyExt};

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
