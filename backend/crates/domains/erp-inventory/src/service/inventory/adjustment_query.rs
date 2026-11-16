use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::ids::StockAdjustmentId;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::InventoryService;
use crate::dto::scope::{ADJUSTMENT_OWNERSHIP_BASIS, ADJUSTMENT_SCOPE_SUMMARY};
use crate::dto::{
    InventoryListPage, PageView, StockAdjustmentLineView, StockAdjustmentListParams,
    StockAdjustmentListQuery, StockAdjustmentView, ensure_scope_version,
};
use crate::entity::inventory::{StockAdjustment, StockAdjustmentLine, StockMovement};
use crate::error::{Error, Result};
use crate::ports::{AdjustmentPeopleFact, InventoryAction, intersect_object_ids};
use crate::repository::prelude::*;
use crate::repository::{InventoryExt, StockAdjustmentFilter, StockAdjustmentRow};

impl InventoryService {
    /// 分页查询库存调整单列表（W10 调整记录视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/状态/经办/申请人/当前审批人筛选）
    /// * `actor` - 当前认证操作人，用于计算对象读取范围
    ///
    /// # 返回
    /// 返回带范围信封的分页视图，投影经办、申请人与当前审批人。
    ///
    /// # 错误
    /// 分页、排序、人员筛选或 `scope_version` 非法时返回 `ValidationError`。
    /// 跨页 `scope_version` 不一致时返回 `ConflictError`。
    /// 账号未激活时返回 `Forbidden`。
    /// 授权、商品、人员或仓储查询失败时返回对应错误。
    #[tracing::instrument(
        name = "inventory.stock_adjustment_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_list")
    )]
    pub async fn stock_adjustment_list(
        &self,
        params: &StockAdjustmentListParams,
        actor: &AuditActor,
    ) -> Result<InventoryListPage<StockAdjustmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.search_stock_adjustments(&query, &actor, executor).await })
            })
            .await
    }

    /// 在原快照完成过滤、分页、投影及姓名补齐，最后复验读取凭证。
    async fn search_stock_adjustments(
        &self,
        query: &StockAdjustmentListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<InventoryListPage<StockAdjustmentView>> {
        let authorization =
            self.authorization.authorize(actor, InventoryAction::AdjustmentList, executor).await?;
        if !authorization.actor_is_active() {
            return Err(Error::Forbidden("当前账号无库存调整读取权限".to_string()));
        }
        let id_in = people_object_ids(self.people_facts.as_ref(), query, executor).await?;
        let search = super::search::sku_filter(
            self.catalog_facts.as_ref(),
            query.q.as_deref(),
            query.sku_id.as_ref(),
            executor,
        )
        .await?;
        let search =
            self.db.inventory().adjustment_search(search, query.adjustment_id.as_deref(), executor).await?;
        let filter = adjustment_filter(query.clone(), search, id_in);
        let page = self
            .db
            .stock_adjustments()
            .search_authorized_stock_adjustments(
                &filter,
                authorization.query(InventoryAction::AdjustmentList),
                executor,
            )
            .await?;
        let ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let people = self.people_facts.people_by_adjustment_ids(&ids, executor).await?;
        let meta = authorization.meta(InventoryAction::AdjustmentList);
        ensure_scope_version(query.paging.page, query.scope_version.as_deref(), meta.scope_version())?;
        let mut items =
            page.items.into_iter().map(|row| adjustment_row_view(row, &people)).collect::<Vec<_>>();
        self.enrich_adjustment_people_names(&mut items, executor).await?;
        let result = InventoryListPage::from_page(
            PageView { items, total: page.total, page: query.paging.page, page_size: query.paging.page_size },
            meta,
            authorization.no_scope(InventoryAction::AdjustmentList),
            ADJUSTMENT_SCOPE_SUMMARY,
            ADJUSTMENT_OWNERSHIP_BASIS,
        );
        authorization.validate(executor).await?;
        Ok(result)
    }

    /// 按主键批量读取调整单申请人与当前审批人，详情与列表同一口径。
    ///
    /// # 参数
    /// * `ids` - 调整单主键
    /// * `executor` - 调用方读取快照。
    ///
    /// # 返回
    /// 返回人员事实映射。
    ///
    /// # 错误
    /// 端口未接线或读取失败。
    pub async fn adjustment_people_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, AdjustmentPeopleFact>> {
        self.people_facts.people_by_adjustment_ids(ids, executor).await
    }

    /// 在同一快照先验证完整对象授权再加载表头；拒绝结果隐藏资源存在性。
    ///
    /// # 参数
    /// * `id` - 调整单主键。
    /// * `actor` - 当前认证操作人。
    /// * `executor` - 调用方读取快照，不开启嵌套事务。
    ///
    /// # 返回
    /// 返回未删除且落在读取范围内的调整单。
    ///
    /// # 错误
    /// 调整单不存在、账号未激活或仓库不在读取范围时返回 `Error::NotFound`。
    /// 授权或仓储查询失败时返回对应错误。
    pub async fn readable_stock_adjustment(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustment> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("库存调整详情必须使用快照事务".into()));
        }
        self.authorization.require_adjustment(actor, id, executor).await.map_err(|error| match error {
            Error::Forbidden(_) | Error::NotFound(_) => Error::NotFound("库存调整单不存在".into()),
            other => other,
        })?;
        let adjustment = self
            .db
            .inventory()
            .stock_adjustment(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
        Ok(adjustment)
    }

    /// 按主键读取库存调整单，不做仓库范围校验。
    ///
    /// # 参数
    /// * `id` - 调整单主键。
    /// * `executor` - 调用方已授权的原事务或读取阶段执行器。
    ///
    /// # 返回
    /// 返回未删除的调整单。
    ///
    /// # 错误
    /// 不存在时返回 `Error::NotFound`。仓储查询失败时返回对应错误。
    pub async fn load_stock_adjustment(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustment> {
        self.db
            .inventory()
            .stock_adjustment(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))
    }

    /// 按调整单主键读取全部明细。
    ///
    /// # 参数
    /// * `adjustment_id` - 调整单主键
    /// * `executor` - 调用方读取快照。
    ///
    /// # 返回
    /// 返回该调整单的明细集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    pub async fn load_adjustment_lines(
        &self,
        adjustment_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockAdjustmentLine>> {
        let id = StockAdjustmentId::new(adjustment_id.to_string());
        Ok(self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&id), executor)
            .await?)
    }

    /// 按来源单据读取过账流水。
    ///
    /// # 参数
    /// * `document_id` - 来源单据主键
    /// * `executor` - 调用方读取快照。
    ///
    /// # 返回
    /// 返回该来源单据产生的库存流水。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    pub async fn load_movements_for_source_document(
        &self,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockMovement>> {
        Ok(self.db.inventory().movements_for_source_document(document_id, executor).await?)
    }
}

/// 申请人与当前审批人按 AND 求交；任一侧未筛选则不收窄该侧。
async fn people_object_ids(
    people: &dyn crate::ports::AdjustmentPeopleFactsPort,
    query: &StockAdjustmentListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let applicants = match &query.applicant_user_ids {
        Some(ids) => Some(people.adjustment_ids_submitted_by(ids, executor).await?),
        None => None,
    };
    let handlers = match &query.handler_user_ids {
        Some(ids) => Some(people.adjustment_ids_assigned_to(ids, executor).await?),
        None => None,
    };
    Ok(intersect_object_ids(applicants, handlers))
}

/// 将已规范化的查询字段移动到库存调整筛选条件。
fn adjustment_filter(
    query: StockAdjustmentListQuery,
    search: crate::repository::InventorySearch,
    id_in: Option<Vec<String>>,
) -> StockAdjustmentFilter {
    let (sort_by, sort_ascending) = query.paging.sort_selection();
    StockAdjustmentFilter {
        search,
        warehouse_ids: query.warehouse_id.map(|id| vec![id]),
        status: query.status,
        prepared_by_ids: query.operator_user_ids,
        id_in,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by,
        sort_ascending,
    }
}

fn adjustment_row_view(
    row: StockAdjustmentRow,
    people: &HashMap<String, AdjustmentPeopleFact>,
) -> StockAdjustmentView {
    let fact = people.get(&row.id);
    StockAdjustmentView {
        id: row.id,
        adjustment_no: row.adjustment_no,
        warehouse_id: row.warehouse_id.to_string(),
        reason_type: row.reason_type,
        status: row.status,
        prepared_by: row.prepared_by,
        prepared_by_name: None,
        submitted_by: fact.and_then(|fact| fact.submitted_by.clone()),
        submitted_by_name: None,
        current_assignee: fact.and_then(|fact| fact.current_assignee.clone()),
        current_assignee_name: None,
        reviewed_by: row.reviewed_by,
        finance_reviewed_by: row.finance_reviewed_by,
        note: row.note,
        occurred_at: row.occurred_at.map(|instant| instant.unix_secs()),
        version: row.version.to_string(),
        created_at: row.created_at,
    }
}

impl From<StockAdjustment> for StockAdjustmentView {
    /// 从调整单实体构造视图；申请人与当前审批人须另按快照/开放任务投影。
    fn from(adjustment: StockAdjustment) -> Self {
        Self {
            id: adjustment.base.id,
            adjustment_no: adjustment.adjustment_no,
            warehouse_id: adjustment.warehouse_id.to_string(),
            reason_type: adjustment.reason_type,
            status: adjustment.status,
            prepared_by: adjustment.prepared_by,
            prepared_by_name: None,
            submitted_by: None,
            submitted_by_name: None,
            current_assignee: None,
            current_assignee_name: None,
            reviewed_by: adjustment.reviewed_by,
            finance_reviewed_by: adjustment.finance_reviewed_by,
            note: adjustment.note,
            occurred_at: adjustment.occurred_at.map(|instant| instant.unix_secs()),
            version: adjustment.base.version.to_string(),
            created_at: adjustment.base.created_at,
        }
    }
}

impl StockAdjustmentView {
    /// 用审批快照申请人与当前开放审批人覆盖列表/详情投影。
    ///
    /// # 参数
    /// * `fact` - 人员事实；缺字段保持为空，不得填入 `created_by`。
    ///
    /// # 返回
    /// 无返回值。用事实覆盖 `submitted_by` 与 `current_assignee`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn apply_people(&mut self, fact: &AdjustmentPeopleFact) {
        self.submitted_by = fact.submitted_by.clone();
        self.current_assignee = fact.current_assignee.clone();
    }
}

impl From<StockAdjustmentLine> for StockAdjustmentLineView {
    /// 从调整明细实体构造视图。
    fn from(line: StockAdjustmentLine) -> Self {
        Self {
            id: line.base.id,
            sku_id: line.sku_id.to_string(),
            sku_code: None,
            sku_name: None,
            quantity: line.quantity,
            direction: line.direction,
        }
    }
}

#[cfg(test)]
mod people_projection_tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{
        AuthorizationPort, FailClosedAdjustmentPeopleFactsPort, FailClosedAuditPort, FailClosedCatalogFacts,
        FailClosedFulfillmentFacts, FailClosedWarehouseFacts, InventoryAuthorization,
    };

    struct DeniedAuthorization(bool);

    #[async_trait]
    impl AuthorizationPort for DeniedAuthorization {
        async fn require_adjustment_update(
            &self,
            _: &AuditActor,
            _: &str,
            _: &mut dyn Executor,
        ) -> Result<()> {
            panic!("详情读取不得检查修改资格")
        }

        async fn require_adjustment(
            &self,
            actor: &AuditActor,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert_eq!((actor.id(), id), ("reader", "adjustment"));
            assert!(executor.boundary().unwrap().is_snapshot());
            if self.0 {
                Err(Error::Internal("authorization-unavailable".into()))
            } else {
                Err(Error::Forbidden("denied".into()))
            }
        }

        async fn authorize(
            &self,
            _: &AuditActor,
            _: InventoryAction,
            _: &mut dyn Executor,
        ) -> Result<InventoryAuthorization> {
            panic!("对象读取不得另起列表资格解析")
        }

        async fn require_warehouse(
            &self,
            _: &AuditActor,
            _: InventoryAction,
            _: &str,
            _: &mut dyn Executor,
        ) -> Result<()> {
            panic!("对象授权不得重复解析仓库资格")
        }

        async fn allowed_warehouses(
            &self,
            _: &AuditActor,
            _: InventoryAction,
            _: &[String],
            _: &mut dyn Executor,
        ) -> Result<BTreeSet<String>> {
            panic!("详情读取不得枚举授权仓库")
        }
    }

    #[tokio::test]
    async fn adjustment_detail_authorizes_before_any_record_read_and_preserves_evidence_failures() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("adjustment_access_unit");
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        for unavailable in [false, true] {
            let service = InventoryService::new(
                db.clone(),
                Arc::new(DeniedAuthorization(unavailable)),
                Arc::new(FailClosedWarehouseFacts),
                Arc::new(FailClosedCatalogFacts),
                Arc::new(FailClosedFulfillmentFacts),
                Arc::new(FailClosedAuditPort),
                Arc::new(FailClosedAdjustmentPeopleFactsPort),
            );
            let result = service
                .readable_stock_adjustment("adjustment", &actor, &mut SnapshotExecutor::default())
                .await;
            if unavailable {
                assert!(
                    matches!(result, Err(Error::Internal(value)) if value == "authorization-unavailable")
                );
            } else {
                assert!(matches!(result, Err(Error::NotFound(_))));
            }
        }
    }

    #[test]
    fn list_projection_uses_snapshot_applicant_not_created_by() {
        let mut view = StockAdjustmentView {
            id: "adj-1".into(),
            adjustment_no: "ADJ-1".into(),
            warehouse_id: "wh-1".into(),
            reason_type: crate::entity::inventory::AdjustmentReasonType::StockGain,
            status: crate::entity::inventory::StockAdjustmentState::Draft,
            prepared_by: "operator-1".into(),
            prepared_by_name: None,
            submitted_by: None,
            submitted_by_name: None,
            current_assignee: None,
            current_assignee_name: None,
            reviewed_by: None,
            finance_reviewed_by: None,
            note: None,
            occurred_at: None,
            version: "1".into(),
            created_at: 1,
        };
        view.apply_people(&AdjustmentPeopleFact {
            submitted_by: Some("applicant-1".into()),
            current_assignee: Some("handler-1".into()),
        });
        assert_eq!(view.submitted_by.as_deref(), Some("applicant-1"));
        assert_ne!(view.submitted_by.as_deref(), Some("creator-1"));
        assert_eq!(view.current_assignee.as_deref(), Some("handler-1"));
        assert_eq!(view.prepared_by, "operator-1");
    }
}
