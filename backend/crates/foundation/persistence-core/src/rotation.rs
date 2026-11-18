//! 调用方拥有集合的版本化轮转游标；仅提供事务读取和比较写入机制。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use serde::{Deserialize, Serialize};

use crate::{Error, Executor, Result, mongo_ops};

/// 单条规则负责人池的持久化位置；没有历史记录时从名单首位开始。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DispatchRotation {
    /// 兼容既有轮转记录的游标位置。
    pub last_owner_user_id: Option<String>,
    /// 用于比较写入的并发版本。
    pub revision: i64,
}

impl DispatchRotation {
    fn update_document(&self, position: Option<&str>) -> Result<Document> {
        let revision =
            self.revision.checked_add(1).ok_or(Error::EntityMetadataOutOfRange("游标版本超出范围"))?;
        Ok(doc! { "$set": { "last_owner_user_id": position, "revision": revision } })
    }
}

/// 分派任务或维护规则前读取调用方集合中的轮询位置。无历史记录视为首次分派，返回空位置和版本零；
/// 读取既不创建记录也不推进轮次。
pub async fn rotation(
    db: &Database,
    collection: &str,
    rule_id: &str,
    executor: &mut dyn Executor,
) -> Result<DispatchRotation> {
    Ok(mongo_ops::find_one(&db.collection::<DispatchRotation>(collection), doc! { "_id": rule_id }, executor)
        .await?
        .unwrap_or_default())
}

/// 在原快照事务内按读到的版本比较写入分派位置；维护规则时可传原位置，只推进版本。
/// 调用即结束旧读取阶段，缺少快照事务、版本越界或发生并发冲突时拒绝，由外层统一提交或回滚。
pub async fn advance(
    db: &Database,
    collection: &str,
    rule_id: &str,
    expected: &DispatchRotation,
    last_owner_user_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    executor.next_read();
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::UnsupportedDeployment("游标写入必须加入调用方事务"));
    }

    let update = expected.update_document(last_owner_user_id)?;
    mongo_ops::update_one(
        &db.collection::<DispatchRotation>(collection),
        doc! { "_id": rule_id, "revision": expected.revision },
        update,
        true,
        executor,
    )
    .await
    .map_err(|error| match error {
        Error::DuplicateKey(_) => Error::OptimisticLockingError,
        other => other,
    })?;
    Ok(())
}

/// 领域启动迁移时，把仍是字符串且目标字段不存在的旧值复制为单元素数组。已迁移记录不再修改，保留旧字段供后续处理。
/// 迁移期间须停掉旧版写入，失败应阻止启动。
pub async fn migrate_scalar_array(db: &Database, collection: &str, scalar: &str, array: &str) -> Result<()> {
    db.collection::<Document>(collection)
        .update_many(
            doc! { array: { "$exists": false }, scalar: { "$type": "string" } },
            vec![doc! { "$set": { array: [format!("${scalar}")] } }],
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use mongodb::options::{ClientOptions, ServerAddress};
    use mongodb::{Client, ClientSession};

    use super::*;
    use crate::ExecutionBoundary;
    use crate::executor::{ExecutionScope, sealed};

    struct PreflightExecutor(ExecutionScope);

    impl sealed::Sealed for PreflightExecutor {}

    impl Executor for PreflightExecutor {
        fn boundary(&self) -> Option<ExecutionBoundary> {
            Some(self.0.boundary())
        }

        fn next_read(&mut self) {
            self.0.advance();
        }

        fn session(&mut self) -> Option<&mut ClientSession> {
            panic!("rotation preconditions must fail before database access")
        }
    }

    #[tokio::test]
    async fn advance_precondition_failures_invalidate_old_reads_and_preserve_execution_mode() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let db = Client::with_options(options).unwrap().database("unit_test");

        for scope in [ExecutionScope::default(), ExecutionScope::snapshot()] {
            let mut executor = PreflightExecutor(scope);
            let before = executor.boundary().unwrap();
            let snapshot = before.is_snapshot();
            let expected = DispatchRotation { revision: i64::MAX, ..Default::default() };

            let error = advance(&db, "rotation", "rule", &expected, None, &mut executor).await.unwrap_err();

            if snapshot {
                assert!(matches!(error, Error::EntityMetadataOutOfRange("游标版本超出范围")));
            } else {
                assert!(matches!(error, Error::UnsupportedDeployment("游标写入必须加入调用方事务")));
            }

            assert!(!before.is_current(&executor));
            let current = executor.boundary().unwrap();
            assert!(current.is_current(&executor));
            assert_eq!(current.is_snapshot(), snapshot);
            assert_eq!(expected.revision, i64::MAX);
        }
    }

    #[test]
    fn advancing_and_configuration_guard_share_revision_without_resetting_position() {
        let initial = DispatchRotation::default();
        let assigned = initial.update_document(Some("member-a")).unwrap();
        let assigned = assigned.get_document("$set").unwrap();
        assert_eq!(assigned.get_str("last_owner_user_id").unwrap(), "member-a");
        assert_eq!(assigned.get_i64("revision").unwrap(), 1);

        let current = DispatchRotation { last_owner_user_id: Some("member-a".into()), revision: 7 };
        let guarded = current.update_document(current.last_owner_user_id.as_deref()).unwrap();
        let guarded = guarded.get_document("$set").unwrap();
        assert_eq!(guarded.get_str("last_owner_user_id").unwrap(), "member-a");
        assert_eq!(guarded.get_i64("revision").unwrap(), 8);
        assert_eq!(current.revision, 7);

        let invalid = DispatchRotation { revision: i64::MAX, ..Default::default() };
        assert!(matches!(invalid.update_document(Some("member-b")), Err(Error::EntityMetadataOutOfRange(_))));
    }
}
