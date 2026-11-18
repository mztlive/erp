//! 历史参与按当前单据关联；分页版本按账号维护，不枚举账号的全部单据。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, mongo_ops};

use crate::entity::document_registry::DocumentParticipant;
use crate::repository::DocumentRegistryExt;
use crate::{Error, Result};

const REVISIONS: &str = "document_participant_revisions";

/// 编译当前单据的参与关系存在性查询。
/// # 参数
/// user 为已经完成入口权限校验的账号。
/// # 返回
/// 最多关联一行的查询阶段，须在授权条件及分页之前执行。
/// # 错误
/// 无；不执行 I/O。
pub fn lookup(user: &str) -> Vec<Document> {
    vec![doc! { "$lookup": {
        "from": Database::DOCUMENT_PARTICIPANTS,
        "let": { "document": "$id" },
        "pipeline": [
            { "$match": { "participant_user_id": user, "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": { "$eq": ["$document_id", "$$document"] } } },
            { "$limit": 1 }, { "$project": { "_id": 1 } }
        ],
        "as": "__document_participant"
    } }]
}

/// 点查账号的历史参与修订号。
/// # 参数
/// db 为 workflow 数据库，user 为当前账号，executor 沿用查询事务。
/// # 返回
/// 不存在时为零；存量参与关系直接通过 lookup 生效。
/// # 错误
/// 数据库失败或版本损坏时拒绝。
pub async fn revision(db: &Database, user: &str, executor: &mut dyn Executor) -> Result<i64> {
    let collection = db.collection::<Document>(REVISIONS);
    let row = mongo_ops::find_one(&collection, doc! { "_id": user }, executor).await?;
    let value = row
        .map(|row| row.get_i64("revision").map_err(|e| Error::Internal(e.to_string())))
        .transpose()?
        .unwrap_or(0);
    if value < 0 {
        return Err(Error::Internal("历史参与修订号损坏".into()));
    }
    Ok(value)
}

/// 追加参与关系并推进该账号的分页版本。
/// # 参数
/// db 为拥有集合的数据库，participant 为已校验实体，executor 为写入事务。
/// # 返回
/// 参与记录与版本原子提交。
/// # 错误
/// 无事务、重复身份或数据库失败时拒绝。
pub async fn append(
    db: &Database,
    participant: &DocumentParticipant,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::Internal("参与记录必须使用活动事务".into()));
    }

    mongo_ops::update_one(
        &db.collection::<Document>(REVISIONS),
        doc! { "_id": &participant.participant_user_id },
        doc! { "$inc": { "revision": 1_i64 } },
        true,
        executor,
    )
    .await?;
    db.document_participants().create(participant, executor).await?;
    Ok(())
}
