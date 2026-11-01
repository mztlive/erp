use erp_core::ids::PartyId;
use mongodb::bson::{Document, doc};

use crate::entity::party::EffectiveRecordStatus;

/// 构造指定日期生效的 Party 从属事实过滤条件。
///
/// # 参数
/// * `party_id` - 所属 Party ID
/// * `as_of` - 业务日期
///
/// # 返回
/// 返回启用状态且日期落在左闭右开有效期内的查询文档。
///
/// # 错误
/// 不返回错误。
pub(super) fn active_fact_filter(
    party_id: &PartyId,
    as_of: erp_core::common::time::BusinessDate,
) -> Document {
    let mut filter = active_fact_window_filter(as_of);
    filter.insert("party_id", party_id.to_string());
    filter
}

/// 构造指定日期生效的 Party 从属事实公共时间窗过滤条件。
///
/// # 参数
/// * `as_of` - 业务日期
///
/// # 返回
/// 返回启用状态且日期落在左闭右开有效期内的公共查询文档。
///
/// # 错误
/// 不返回错误。
pub(super) fn active_fact_window_filter(as_of: erp_core::common::time::BusinessDate) -> Document {
    let as_of = as_of.to_string();
    doc! {
        "status": EffectiveRecordStatus::Active.as_str(),
        "valid_from": { "$lte": &as_of },
        "$or": [
            { "valid_to": null },
            { "valid_to": { "$gt": &as_of } },
        ],
    }
}

/// 构建排序文档（仓储白名单）。
///
/// `sort_by` 不在 `allowed` 白名单内时回落默认 `created_at`，禁止透传任意
/// 字段名（P2 §2.3）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
///
/// # 错误
/// 不返回错误。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|candidate| allowed.contains(candidate)).unwrap_or("created_at");
    doc! { field: direction }
}

/// 构造从属事实默认优先的稳定排序（erp-party-012）。
///
/// 四个从属事实的 `list_current_on`/`list_by_party` 共用同一排序语义；
/// 排序文档唯一来源，避免默认优先与创建时间顺序漂移。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回默认标记优先、同组内最新创建优先的排序文档。
///
/// # 错误
/// 不返回错误。
pub(super) fn default_first_sort() -> Document {
    doc! { "is_default": -1, "created_at": -1 }
}

/// 为四类主体从属事实生成同一默认标记清理方法。
///
/// 保留各仓储已有的实体类型、逐条 CAS 更新和调用方执行器；任一次读取或
/// 更新失败立即终止。调用模块须导入 `PartyId`、`Executor`、`Result` 与
/// [`party_default_marks_filter`]，实体须具有 `base` 与 `is_default` 字段。
macro_rules! impl_clear_other_default_marks {
    () => {
        async fn clear_other_default_marks(
            &self,
            party_id: &PartyId,
            exclude_id: Option<&str>,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            let rows = self.find_many(party_default_marks_filter(party_id), executor).await?;
            for mut row in rows {
                if exclude_id.is_some_and(|id| id == row.base.id) {
                    continue;
                }
                row.is_default = false;
                self.update(&mut row, executor).await?;
            }
            Ok(())
        }
    };
}

pub(super) use impl_clear_other_default_marks;

/// 构造同一主体默认标记行的查询条件（erp-party-012）。
///
/// 四个从属事实的 `clear_other_default_marks` 共用同一过滤语义；调用方
/// 在同一事务执行器内逐行清除，与主写入组成原子边界。
///
/// # 参数
/// * `party_id` - 所属 Party ID
///
/// # 返回
/// 返回限定同一主体默认行的查询文档。
///
/// # 错误
/// 不返回错误。
pub(super) fn party_default_marks_filter(party_id: &PartyId) -> Document {
    doc! { "party_id": party_id.to_string(), "is_default": true }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use entity_core::BaseModel;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::PartyId;
    use mongodb::ClientSession;
    use mongodb::bson::{Document, doc};
    use persistence_core::{Error, Executor, Result};

    use super::{active_fact_window_filter, default_first_sort, party_default_marks_filter, sort_doc};

    #[test]
    fn active_fact_window_is_left_closed_and_right_open() {
        let as_of = BusinessDate::from_ymd(2026, 8, 27).expect("测试日期必须合法");

        let filter = active_fact_window_filter(as_of);

        assert_eq!(
            filter
                .get_document("valid_from")
                .expect("必须包含开始日期")
                .get_str("$lte")
                .expect("开始日期必须包含当天"),
            "2026-08-27"
        );
        let valid_to = filter.get_array("$or").expect("必须包含结束日期分支")[1]
            .as_document()
            .expect("结束日期分支必须是文档")
            .get_document("valid_to")
            .expect("必须包含结束日期");
        assert_eq!(valid_to.get_str("$gt").expect("结束日期必须排除当天"), "2026-08-27");
    }

    #[test]
    fn sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted() {
        assert_eq!(
            sort_doc(Some("revised_at"), false, &["created_at", "party_no"]),
            doc! { "created_at": -1 }
        );
        assert_eq!(sort_doc(Some("party_no"), true, &["created_at", "party_no"]), doc! { "party_no": 1 });
        assert_eq!(sort_doc(None, false, &["created_at"]), doc! { "created_at": -1 });
    }

    #[test]
    fn subordinate_fact_sort_and_default_filter_share_single_source() {
        assert_eq!(default_first_sort(), doc! { "is_default": -1, "created_at": -1 });
        let party_id = erp_core::ids::PartyId::new("party-1");
        assert_eq!(party_default_marks_filter(&party_id), doc! { "party_id": "party-1", "is_default": true });
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct DefaultFact {
        base: BaseModel,
        is_default: bool,
    }

    struct DefaultMarksRepository {
        rows: Vec<DefaultFact>,
        filters: RefCell<Vec<Document>>,
        attempted_ids: RefCell<Vec<String>>,
        saved: RefCell<Vec<DefaultFact>>,
        fail_read: bool,
        fail_update_id: Option<&'static str>,
    }

    impl DefaultMarksRepository {
        fn new(ids: &[&str]) -> Self {
            Self {
                rows: ids
                    .iter()
                    .map(|id| DefaultFact {
                        base: BaseModel { id: (*id).to_string(), ..BaseModel::fake() },
                        is_default: true,
                    })
                    .collect(),
                filters: RefCell::default(),
                attempted_ids: RefCell::default(),
                saved: RefCell::default(),
                fail_read: false,
                fail_update_id: None,
            }
        }

        async fn find_many(&self, filter: Document, executor: &mut dyn Executor) -> Result<Vec<DefaultFact>> {
            let _ = executor.session();
            self.filters.borrow_mut().push(filter);
            if self.fail_read {
                return Err(Error::DatabaseError(mongodb::error::Error::custom("read failed")));
            }
            Ok(self.rows.clone())
        }

        async fn update(&self, row: &mut DefaultFact, executor: &mut dyn Executor) -> Result<()> {
            let _ = executor.session();
            self.attempted_ids.borrow_mut().push(row.base.id.clone());
            if self.fail_update_id == Some(row.base.id.as_str()) {
                return Err(Error::OptimisticLockingError);
            }
            self.saved.borrow_mut().push(row.clone());
            Ok(())
        }

        impl_clear_other_default_marks!();
    }

    #[derive(Default)]
    struct RecordingExecutor {
        calls: usize,
    }

    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.calls += 1;
            None
        }
    }

    #[tokio::test]
    async fn default_marks_excludes_matching_record_and_preserves_metadata() {
        let repository = DefaultMarksRepository::new(&["first", "keep", "last"]);
        let mut executor = RecordingExecutor::default();

        repository
            .clear_other_default_marks(&PartyId::new("party-1"), Some("keep"), &mut executor)
            .await
            .unwrap();

        let saved = repository.saved.borrow();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].base, repository.rows[0].base);
        assert_eq!(saved[1].base, repository.rows[2].base);
        assert!(saved.iter().all(|row| !row.is_default));
        assert_eq!(*repository.attempted_ids.borrow(), ["first", "last"]);
        assert_eq!(*repository.filters.borrow(), [doc! { "party_id": "party-1", "is_default": true }]);
        assert_eq!(executor.calls, 3);
    }

    #[tokio::test]
    async fn default_marks_without_exclusion_updates_all_records_in_order() {
        let repository = DefaultMarksRepository::new(&["first", "last"]);
        let mut executor = RecordingExecutor::default();

        repository.clear_other_default_marks(&PartyId::new("party-1"), None, &mut executor).await.unwrap();

        assert_eq!(*repository.attempted_ids.borrow(), ["first", "last"]);
        assert_eq!(repository.saved.borrow().len(), 2);
        assert!(repository.saved.borrow().iter().all(|row| !row.is_default));
        assert_eq!(executor.calls, 3);
    }

    #[tokio::test]
    async fn default_marks_empty_records_performs_no_updates() {
        let repository = DefaultMarksRepository::new(&[]);
        let mut executor = RecordingExecutor::default();

        repository
            .clear_other_default_marks(&PartyId::new("party-1"), Some("keep"), &mut executor)
            .await
            .unwrap();

        assert!(repository.attempted_ids.borrow().is_empty());
        assert!(repository.saved.borrow().is_empty());
        assert_eq!(executor.calls, 1);
    }

    #[tokio::test]
    async fn default_marks_update_failure_stops_later_records() {
        let mut repository = DefaultMarksRepository::new(&["first", "conflict", "last"]);
        repository.fail_update_id = Some("conflict");
        let mut executor = RecordingExecutor::default();

        let result =
            repository.clear_other_default_marks(&PartyId::new("party-1"), None, &mut executor).await;

        assert!(matches!(result, Err(Error::OptimisticLockingError)));
        assert_eq!(*repository.attempted_ids.borrow(), ["first", "conflict"]);
        let saved = repository.saved.borrow();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].base.id, "first");
        assert!(!saved[0].is_default);
        assert_eq!(executor.calls, 3);
    }

    #[tokio::test]
    async fn default_marks_read_failure_performs_no_updates() {
        let mut repository = DefaultMarksRepository::new(&["first"]);
        repository.fail_read = true;
        let mut executor = RecordingExecutor::default();

        let result =
            repository.clear_other_default_marks(&PartyId::new("party-1"), None, &mut executor).await;

        match result {
            Err(Error::DatabaseError(error)) => {
                assert_eq!(error.get_custom::<&str>().copied(), Some("read failed"));
            },
            other => panic!("应透传读取失败的原始错误，实际为 {other:?}"),
        }
        assert!(repository.attempted_ids.borrow().is_empty());
        assert!(repository.saved.borrow().is_empty());
        assert_eq!(executor.calls, 1);
    }
}
