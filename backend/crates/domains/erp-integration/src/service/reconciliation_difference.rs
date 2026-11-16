//! 集成本域查询、实体准备与调用方事务内写入。
use std::collections::HashMap;

use application_core::AuditActor;
use erp_permissions::integration_ops;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::IntegrationOpsService;
use super::scope::{ScopedIntegrationList, ensure_page, resolve_list_scope};
use crate::dto::{self, *};
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::repository::integration_ops::ReconciliationDifferenceRow;
use crate::repository::integration_ops::reader::IntegrationReader;
use crate::{Error, Result};

mod delivery;

/// 对账差异列表筛选条件类型。
type DifferenceFilter = <Database as IntegrationOpsExt>::ReconciliationDifferenceFilter;
impl IntegrationOpsService {
    /// 分页查询对账差异，并按最新决定派生状态与版本。
    ///
    /// # 参数
    /// * `params` - 列表查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围元数据的分页视图；无决定的差异状态为 `None`、版本为 0。
    /// 完整授权条件无匹配记录时返回空页。
    ///
    /// # 错误
    /// 查询参数非法、范围版本变化、授权或组织展开失败、仓储查询或事务失败时返回对应错误。
    pub async fn difference_list(
        &self,
        params: &DifferenceListParams,
        actor: &AuditActor,
    ) -> Result<DifferenceListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, query.scope_version.as_deref())?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.difference_page(query, &actor, executor, &this.db).await })
            })
            .await
    }

    /// 在原事务内完成授权、根分页、最新决定补齐与交付复验。
    async fn difference_page(
        &self,
        query: dto::DifferenceListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
        reader: &dyn IntegrationReader,
    ) -> Result<DifferenceListView> {
        let access = self.access();
        let scope = resolve_list_scope(
            &access,
            actor,
            &integration_ops::DIFFERENCE_LIST,
            &query.org_unit_ids,
            query.include_descendants,
            query.scope_version.as_deref(),
            executor,
        )
        .await?;
        let operator_difference_ids =
            self.operator_difference_ids(&query.operator_user_ids, executor, reader).await?;
        let filter = difference_filter(query, scope.owner_org_unit_ids, operator_difference_ids);
        let page = reader.difference_page(&filter, &scope.read_scope, executor).await?;
        delivery::deliver_difference_page(
            self,
            page,
            &scope.read_scope,
            &filter,
            scope.meta,
            executor,
            reader,
        )
        .await
    }

    /// 把历史处理人筛选解析为差异 ID 集合。
    ///
    /// # 参数
    /// * `operator_user_ids` - 已规范化的历史处理人 ID
    /// * `executor` - 与列表查询相同的执行器
    ///
    /// # 返回
    /// 未筛选时返回 `None`；否则返回处理过的差异 ID。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    async fn operator_difference_ids(
        &self,
        operator_user_ids: &[String],
        executor: &mut dyn Executor,
        reader: &dyn IntegrationReader,
    ) -> Result<Option<Vec<String>>> {
        if operator_user_ids.is_empty() {
            return Ok(None);
        }
        Ok(Some(reader.handled_difference_ids(operator_user_ids, executor).await?))
    }
}
/// 构造差异并把实体不变量失败映射为 `ValidationError`。
///
/// # 参数
/// * `req` - 差异登记请求
/// * `owner_org_unit_id` - 处理人当前内部组织
///
/// # 返回
/// 返回新建的对账差异。
///
/// # 错误
/// 对象身份、证据引用或处理人不满足实体规则时返回 `ValidationError`，文案来自领域错误。
pub fn prepare_difference(
    req: &CreateDifferenceRequest,
    owner_org_unit_id: String,
) -> Result<ReconciliationDifference> {
    let difference = ReconciliationDifference::new(
        ReconciliationDifferenceId::new(next_id()),
        ReconciliationDifferenceData {
            business_object_type: req.business_object_type.clone(),
            business_object_id: req.business_object_id.clone(),
            difference_type: req.difference_type.clone(),
            left_fact_reference: req.left_fact_reference.clone(),
            right_fact_reference: req.right_fact_reference.clone(),
            owner_user_id: req.owner_user_id.clone(),
            owner_org_unit_id,
        },
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;

    Ok(difference)
}

/// 把已规范化查询与授权条件装配为差异仓储筛选。
fn difference_filter(
    query: dto::DifferenceListQuery,
    owner_org_unit_ids: Vec<String>,
    operator_difference_ids: Option<Vec<String>>,
) -> DifferenceFilter {
    DifferenceFilter {
        q: query.q,
        business_object_type: query.business_object_type,
        business_object_id: query.business_object_id,
        difference_type: query.difference_type,
        created_at_from: query.created_at_from,
        created_at_to: query.created_at_to,
        handler_user_ids: query.handler_user_ids,
        operator_difference_ids,
        owner_org_unit_ids,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 装配带范围元数据的差异列表响应。
fn difference_list_view(
    total: i64,
    items: Vec<DifferenceView>,
    filter: &DifferenceFilter,
    meta: ScopedIntegrationList,
) -> DifferenceListView {
    DifferenceListView {
        data: PageView { items, total, page: filter.page, page_size: filter.page_size },
        scope_version: meta.scope_version,
        as_of: meta.as_of,
        empty_reason: None,
        scope_summary: meta.scope_summary,
        ownership_basis: meta.ownership_basis,
    }
}

/// 无最新决定的差异状态为 `None`、版本为 0。
fn project_difference_rows(
    rows: Vec<ReconciliationDifferenceRow>,
    latest_by_difference: &HashMap<String, ReconciliationDifferenceResolution>,
) -> Vec<DifferenceView> {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let (status, version) = latest_by_difference
            .get(&row.id)
            .map_or((None, 0), |record| (Some(record.resulting_status), u64::from(record.resolution_no)));
        items.push(DifferenceView {
            id: row.id,
            business_object_type: row.business_object_type,
            business_object_id: row.business_object_id,
            business_object_label: None,
            difference_type: row.difference_type,
            left_fact_reference: row.left_fact_reference,
            left_fact_label: None,
            right_fact_reference: row.right_fact_reference,
            right_fact_label: None,
            status,
            version,
            created_at: row.created_at,
            owner_user_id: row.owner_user_id.unwrap_or_default(),
            owner_user_name: None,
        });
    }
    items
}

/// 在调用方事务内保存不可变差异事实。
///
/// # 参数
/// * `db` - 目标数据库
/// * `difference` - 待写入的对账差异
/// * `executor` - 调用方执行器
///
/// # 返回
/// 成功时差异已写入。
///
/// # 错误
/// 仓储写入失败时返回对应错误。
pub async fn persist_difference(
    db: &Database,
    difference: &ReconciliationDifference,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.reconciliation_differences().create(difference, executor).await?;
    Ok(())
}

#[cfg(test)]
mod authorization_tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::{unavailable_authorization, unbound_authorization};
    use persistence_core::Error as PersistenceError;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::repository::integration_ops::reader::testing::{
        Behavior, MemoryReader, actor, own_scope, plan_denies, plan_owned_by, service,
    };
    use crate::service::IntegrationAccess;

    #[tokio::test]
    async fn difference_page_runs_real_query_projection_and_final_validate_with_list_permission() {
        let reader = Arc::new(MemoryReader::default());
        let permission = integration_ops::DIFFERENCE_LIST;
        let service = service(vec![permission.clone()], &[permission], own_scope(), reader.clone()).await;
        let params: DifferenceListParams =
            serde_json::from_value(serde_json::json!({"operator_user_ids":"operator"})).unwrap();
        let mut executor = SnapshotExecutor::default();
        let view = service
            .difference_page(params.normalized().unwrap(), &actor(), &mut executor, reader.as_ref())
            .await
            .unwrap();
        assert_eq!((view.data.total, view.data.page, view.data.items.len()), (5, 1, 1));
        assert_eq!(view.data.items[0].id, "difference");
        assert_eq!((view.data.items[0].status, view.data.items[0].version), (None, 0));
        let accesses = reader.accesses.lock().unwrap();
        let access = &accesses[0];
        assert_eq!(access.model().resource, "reconciliation_difference");
        assert!(plan_owned_by(access, "reader"));
        assert_eq!(view.scope_version, access.version());
        assert!(access.ensure_current(&mut executor).is_err());
        assert_eq!(*reader.events.lock().unwrap(), ["handled", "difference_page", "latest"]);
        assert_eq!(
            reader.difference_filters.lock().unwrap()[0].operator_difference_ids,
            Some(vec!["difference".into()])
        );
    }

    #[tokio::test]
    async fn difference_list_rejects_detail_only_missing_permission_and_other_identity_before_reads() {
        for (grants, user) in [
            (vec![], actor()),
            (vec![integration_ops::DIFFERENCE_DETAIL], actor()),
            (
                vec![integration_ops::DIFFERENCE_LIST],
                AuditActor::new("other".into(), "other".into(), AccountKind::Admin),
            ),
        ] {
            let reader = Arc::new(MemoryReader::default());
            let service =
                service(grants, &[integration_ops::DIFFERENCE_LIST], own_scope(), reader.clone()).await;
            let params: DifferenceListParams =
                serde_json::from_value(serde_json::json!({"operator_user_ids":"operator"})).unwrap();
            assert!(matches!(
                service
                    .difference_page(
                        params.normalized().unwrap(),
                        &user,
                        &mut SnapshotExecutor::default(),
                        reader.as_ref()
                    )
                    .await,
                Err(Error::Forbidden(_))
            ));
            assert!(reader.events.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn difference_list_empty_scope_keeps_deny_filter_and_final_validation() {
        let reader = Arc::new(MemoryReader::with_behavior(Behavior::Empty));
        let permission = integration_ops::DIFFERENCE_LIST;
        let scope = ResolvedScope {
            role_clauses: vec![],
            user_limit: Some(ScopeClause { company: true, ..Default::default() }),
        };
        let service = service(vec![permission.clone()], &[permission], scope, reader.clone()).await;
        let mut executor = SnapshotExecutor::default();
        let view = service
            .difference_page(
                DifferenceListParams::default().normalized().unwrap(),
                &actor(),
                &mut executor,
                reader.as_ref(),
            )
            .await
            .unwrap();
        assert!(view.data.items.is_empty());
        assert_eq!(view.data.total, 0);
        let accesses = reader.accesses.lock().unwrap();
        assert!(!accesses[0].has_role_scope());
        assert!(plan_denies(&accesses[0]));
        assert!(accesses[0].ensure_current(&mut executor).is_err());
        assert_eq!(*reader.events.lock().unwrap(), ["difference_page", "latest"]);
    }

    #[tokio::test]
    async fn difference_list_discards_pages_on_page_or_latest_failure_and_changed_stage() {
        for behavior in [Behavior::PageFailure, Behavior::LatestFailure, Behavior::LatestStage] {
            let reader = Arc::new(MemoryReader::with_behavior(behavior));
            let permission = integration_ops::DIFFERENCE_LIST;
            let service = service(vec![permission.clone()], &[permission], own_scope(), reader.clone()).await;
            let error = service
                .difference_page(
                    DifferenceListParams::default().normalized().unwrap(),
                    &actor(),
                    &mut SnapshotExecutor::default(),
                    reader.as_ref(),
                )
                .await
                .unwrap_err();
            if behavior == Behavior::LatestStage {
                assert!(
                    matches!(error, Error::ValidationError(message) if message == "授权结果不能跨事务或读取阶段使用")
                );
            } else {
                let reason = if behavior == Behavior::PageFailure {
                    "测试分页读取失败"
                } else {
                    "测试决定读取失败"
                };
                assert_eq!(
                    error.to_string(),
                    Error::from(PersistenceError::EntityMetadataOutOfRange(reason)).to_string()
                );
            }
            let expected = if behavior == Behavior::PageFailure {
                vec!["difference_page"]
            } else {
                vec!["difference_page", "latest"]
            };
            assert_eq!(*reader.events.lock().unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn difference_list_rejects_changed_scope_version_before_history_or_page_reads() {
        let reader = Arc::new(MemoryReader::default());
        let permission = integration_ops::DIFFERENCE_LIST;
        let service = service(vec![permission.clone()], &[permission], own_scope(), reader.clone()).await;
        let params = DifferenceListParams { scope_version: Some("stale".into()), ..Default::default() };
        assert!(
            matches!(service.difference_page(params.normalized().unwrap(), &actor(), &mut SnapshotExecutor::default(), reader.as_ref()).await, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
        );
        assert!(reader.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn difference_page_rejects_missing_mapping_and_identity_failure_before_business_reads() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("difference_authorization_test");
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        for (authorization, missing_mapping) in
            [(unbound_authorization(), true), (unavailable_authorization(), false)]
        {
            let access = IntegrationAccess::new(db.clone(), authorization);
            let service = IntegrationOpsService::new(db.clone(), access);
            let params = DifferenceListParams::default();
            let error = service
                .difference_page(params.normalized().unwrap(), &actor, &mut SnapshotExecutor::default(), &db)
                .await
                .expect_err("授权失败必须先于历史操作人、根分页及决定补齐读取");
            if missing_mapping {
                assert!(matches!(error, Error::ValidationError(reason) if reason == "缺少查询 Mapping 绑定"));
            } else {
                assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
            }
        }
    }
}
