//! 集成授权用例的持久化读取边界；权限分派、事实转换及交付检查留在原生产逻辑。
use std::collections::HashMap;

use async_trait::async_trait;
use erp_identity::CompiledAccess;
use mongodb::Database;
use persistence_core::{Executor, PageResult, Result};

use super::{
    IntegrationErrorTaskFilter, IntegrationErrorTaskRepositoryExt, IntegrationErrorTaskRow,
    ReconciliationDifferenceFilter, ReconciliationDifferenceRepositoryExt,
    ReconciliationDifferenceResolutionBatchExt, ReconciliationDifferenceResolutionRepositoryExt,
    ReconciliationDifferenceRow,
};
use crate::entity::integration_ops::{
    IntegrationErrorTask, ReconciliationDifference, ReconciliationDifferenceResolution,
};
use crate::repository::IntegrationOpsExt;

#[async_trait]
pub(crate) trait IntegrationReader: Send + Sync {
    /// 读取已授权错误任务页及计数。
    /// # 参数
    /// filter 为业务筛选，access 为原查询凭证，executor 为原执行器。
    /// # 返回
    /// 原快照内的页及总数。
    /// # 错误
    /// 凭证守卫或持久化失败时拒绝。
    async fn error_page(
        &self,
        filter: &IntegrationErrorTaskFilter,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<IntegrationErrorTaskRow>>;

    /// 读取已授权差异页及计数。
    /// # 参数
    /// filter、access、executor 为原业务筛选、凭证和执行器。
    /// # 返回
    /// 原快照内的页及总数。
    /// # 错误
    /// 凭证守卫或持久化失败时拒绝。
    async fn difference_page(
        &self,
        filter: &ReconciliationDifferenceFilter,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReconciliationDifferenceRow>>;

    /// 读取历史处理人对应的差异身份。
    /// # 参数
    /// users 为历史处理人，executor 为原执行器。
    /// # 返回
    /// 去重的差异 ID。
    /// # 错误
    /// 持久化失败时返回错误。
    async fn handled_difference_ids(
        &self,
        users: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 补齐当前页的最新决定。
    /// # 参数
    /// ids 为当前页 ID，executor 为原执行器。
    /// # 返回
    /// 按差异 ID 索引的最新决定。
    /// # 错误
    /// 持久化失败时返回错误。
    async fn latest_decisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReconciliationDifferenceResolution>>;

    /// 读取错误任务真实事实。
    /// # 参数
    /// id 为根身份，executor 为原执行器。
    /// # 返回
    /// 未删除的实体，缺失时为 None。
    /// # 错误
    /// 持久化失败时返回错误。
    async fn error_task(&self, id: &str, executor: &mut dyn Executor)
    -> Result<Option<IntegrationErrorTask>>;

    /// 读取差异真实事实及版本。
    /// # 参数
    /// id 为根身份，executor 为原执行器。
    /// # 返回
    /// 未删除的实体，缺失时为 None。
    /// # 错误
    /// 持久化失败时返回错误。
    async fn difference(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifference>>;
}

#[async_trait]
impl IntegrationReader for Database {
    async fn error_page(
        &self,
        filter: &IntegrationErrorTaskFilter,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<IntegrationErrorTaskRow>> {
        self.integration_error_tasks().search_error_tasks(filter, access, executor).await
    }

    async fn difference_page(
        &self,
        filter: &ReconciliationDifferenceFilter,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReconciliationDifferenceRow>> {
        self.reconciliation_differences().search_differences(filter, access, executor).await
    }

    async fn handled_difference_ids(
        &self,
        users: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        self.reconciliation_difference_resolutions().find_difference_ids_handled_by(users, executor).await
    }

    async fn latest_decisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReconciliationDifferenceResolution>> {
        self.reconciliation_difference_resolutions().find_latest_by_differences(ids, executor).await
    }

    async fn error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<IntegrationErrorTask>> {
        self.integration_error_tasks().find_by_id(id, executor).await
    }

    async fn difference(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReconciliationDifference>> {
        self.reconciliation_differences().find_by_id(id, executor).await
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_identity::Permission;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::authorization;
    use mongodb::Client;
    use mongodb::bson::{Bson, Document, doc};
    use persistence_core::Error;

    use super::*;
    use crate::entity::integration_ops::{
        ErrorClass, IntegrationErrorTaskData, IntegrationErrorTaskId, ReconciliationDifferenceId,
    };
    use crate::repository::authorization::{facts_using, mapping};
    use crate::service::{IntegrationAccess, IntegrationOpsService};

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) enum Behavior {
        #[default]
        Normal,
        Empty,
        PageFailure,
        PageStage,
        LatestFailure,
        LatestStage,
        MissingFacts,
        ChangedVersion,
        FactsFailure,
    }

    pub(crate) struct MemoryReader {
        pub events: Mutex<Vec<&'static str>>,
        pub accesses: Mutex<Vec<CompiledAccess>>,
        pub error_filters: Mutex<Vec<IntegrationErrorTaskFilter>>,
        pub difference_filters: Mutex<Vec<ReconciliationDifferenceFilter>>,
        pub difference: Mutex<Option<ReconciliationDifference>>,
        pub error: Mutex<Option<IntegrationErrorTask>>,
        pub behavior: Behavior,
        difference_reads: AtomicUsize,
    }

    impl Default for MemoryReader {
        fn default() -> Self {
            let mut error = IntegrationErrorTask::new(
                IntegrationErrorTaskId::new("error"),
                IntegrationErrorTaskData {
                    message_id: None,
                    business_object_id: Some("business".into()),
                    error_class: ErrorClass::TransientFailure,
                    owner_role: None,
                    owner_user_id: Some("reader".into()),
                    owner_org_unit_id: "org".into(),
                },
            )
            .unwrap();
            error.base = BaseModel { id: "error".into(), version: 5, ..BaseModel::fake() };
            Self {
                events: Mutex::new(Vec::new()),
                accesses: Mutex::new(Vec::new()),
                error_filters: Mutex::new(Vec::new()),
                difference_filters: Mutex::new(Vec::new()),
                difference: Mutex::new(Some(ReconciliationDifference {
                    base: BaseModel {
                        id: ReconciliationDifferenceId::new("difference").to_string(),
                        version: 7,
                        ..BaseModel::fake()
                    },
                    business_object_type: "sales_order".into(),
                    business_object_id: "business".into(),
                    difference_type: "amount_mismatch".into(),
                    left_fact_reference: Some("left".into()),
                    right_fact_reference: None,
                    owner_user_id: "reader".into(),
                    owner_org_unit_id: "org".into(),
                })),
                error: Mutex::new(Some(error)),
                behavior: Behavior::Normal,
                difference_reads: AtomicUsize::new(0),
            }
        }
    }

    impl MemoryReader {
        pub(crate) fn with_behavior(behavior: Behavior) -> Self {
            Self { behavior, ..Default::default() }
        }

        fn page_read(
            &self,
            name: &'static str,
            access: &CompiledAccess,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            access.ensure_current(executor).unwrap();
            self.events.lock().unwrap().push(name);
            self.accesses.lock().unwrap().push(access.clone());
            if self.behavior == Behavior::PageStage {
                executor.next_read();
            }
            if self.behavior == Behavior::PageFailure {
                return Err(Error::EntityMetadataOutOfRange("测试分页读取失败"));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl IntegrationReader for MemoryReader {
        async fn error_page(
            &self,
            filter: &IntegrationErrorTaskFilter,
            access: &CompiledAccess,
            executor: &mut dyn Executor,
        ) -> Result<PageResult<IntegrationErrorTaskRow>> {
            self.page_read("error_page", access, executor)?;
            self.error_filters.lock().unwrap().push(filter.clone());
            let items = if self.behavior == Behavior::Empty {
                vec![]
            } else {
                vec![
                    serde_json::from_value(serde_json::json!({
                        "id":"error", "business_object_id":"business", "error_class":"transient_failure",
                        "status":"pending", "owner_user_id":"reader", "owner_org_unit_id":"org",
                        "attempt_count":2, "version":5, "created_at":9
                    }))
                    .unwrap(),
                ]
            };
            Ok(PageResult { total: if items.is_empty() { 0 } else { 5 }, items })
        }

        async fn difference_page(
            &self,
            filter: &ReconciliationDifferenceFilter,
            access: &CompiledAccess,
            executor: &mut dyn Executor,
        ) -> Result<PageResult<ReconciliationDifferenceRow>> {
            self.page_read("difference_page", access, executor)?;
            self.difference_filters.lock().unwrap().push(filter.clone());
            let items = if self.behavior == Behavior::Empty {
                vec![]
            } else {
                vec![serde_json::from_value(serde_json::json!({
                    "id":"difference", "business_object_type":"sales_order", "business_object_id":"business",
                    "difference_type":"amount_mismatch", "left_fact_reference":"left",
                    "owner_user_id":"reader", "owner_org_unit_id":"org", "version":7, "created_at":9
                })).unwrap()]
            };
            Ok(PageResult { total: if items.is_empty() { 0 } else { 5 }, items })
        }

        async fn handled_difference_ids(
            &self,
            users: &[String],
            executor: &mut dyn Executor,
        ) -> Result<Vec<String>> {
            assert!(executor.boundary().unwrap().is_snapshot());
            assert_eq!(users, ["operator"]);
            self.events.lock().unwrap().push("handled");
            Ok(vec!["difference".into()])
        }

        async fn latest_decisions(
            &self,
            ids: &[String],
            executor: &mut dyn Executor,
        ) -> Result<HashMap<String, ReconciliationDifferenceResolution>> {
            self.accesses.lock().unwrap().last().unwrap().ensure_current(executor).unwrap();
            assert!(ids.is_empty() || ids == ["difference"]);
            self.events.lock().unwrap().push("latest");
            if self.behavior == Behavior::LatestStage {
                executor.next_read();
            }
            if self.behavior == Behavior::LatestFailure {
                return Err(Error::EntityMetadataOutOfRange("测试决定读取失败"));
            }
            Ok(HashMap::new())
        }

        async fn error_task(
            &self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<IntegrationErrorTask>> {
            assert!(executor.boundary().unwrap().is_snapshot());
            self.events.lock().unwrap().push("error_facts");
            Ok(self.error.lock().unwrap().clone().filter(|row| row.base.id == id))
        }

        async fn difference(
            &self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<ReconciliationDifference>> {
            assert!(executor.boundary().unwrap().is_snapshot());
            self.events.lock().unwrap().push("difference_facts");
            let call = self.difference_reads.fetch_add(1, Ordering::Relaxed);
            if self.behavior == Behavior::FactsFailure && call > 0 {
                return Err(Error::EntityMetadataOutOfRange("测试 Facts 读取失败"));
            }
            if self.behavior == Behavior::MissingFacts && call > 0 {
                return Ok(None);
            }
            let mut row = self.difference.lock().unwrap().clone().filter(|row| row.base.id == id);
            if self.behavior == Behavior::ChangedVersion && call > 0 {
                row.as_mut().unwrap().base.version += 1;
            }
            Ok(row)
        }
    }

    pub(crate) fn actor() -> AuditActor {
        AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin)
    }

    pub(crate) fn own_scope() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        }
    }

    pub(crate) async fn service(
        grants: Vec<Permission>,
        queries: &[Permission],
        scope: ResolvedScope,
        reader: Arc<MemoryReader>,
    ) -> IntegrationOpsService {
        let facts = facts_using(reader);
        let service = authorization(
            actor(),
            grants,
            scope,
            [("integration_error_task".into(), facts.clone()), ("reconciliation_difference".into(), facts)]
                .into(),
            queries.iter().map(|p| (p.clone(), mapping(p.resource()).unwrap())).collect(),
        )
        .unwrap();
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("integration_unit_test");
        let access = IntegrationAccess::new(db.clone(), service);
        IntegrationOpsService::new(db, access)
    }

    pub(crate) fn plan_contains(stages: &[Document], expected: Document) -> bool {
        fn contains(value: &Bson, expected: &Bson) -> bool {
            value == expected
                || match value {
                    Bson::Document(doc) => doc.values().any(|value| contains(value, expected)),
                    Bson::Array(items) => items.iter().any(|value| contains(value, expected)),
                    _ => false,
                }
        }
        let expected = Bson::Document(expected);
        stages.iter().any(|stage| contains(&Bson::Document(stage.clone()), &expected))
    }

    pub(crate) fn plan_owned_by(access: &CompiledAccess, user: &str) -> bool {
        plan_contains(access.stages(), doc! {"owner_user_id":user})
    }

    pub(crate) fn plan_denies(access: &CompiledAccess) -> bool {
        plan_contains(access.stages(), doc! {"$expr":false})
    }
}
