use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::ids::StockAdjustmentId;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::InventoryService;
use crate::dto::scope::{ADJUSTMENT_OWNERSHIP_BASIS, ADJUSTMENT_SCOPE_SUMMARY};
use crate::dto::{
    InventoryListPage, PageView, StockAdjustmentLineView, StockAdjustmentListParams,
    StockAdjustmentListQuery, StockAdjustmentView, ensure_scope_version,
};
use crate::entity::inventory::{StockAdjustment, StockAdjustmentLine, StockMovement};
use crate::error::{Error, Result};
use crate::ports::{AdjustmentPeopleFact, InventoryAuthorization, intersect_object_ids};
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
    /// * `ValidationError` - 分页或人员参数非法
    /// * `ConflictError` - 跨页 `scope_version` 不一致
    /// * `RepositoryError` - 数据库查询失败
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
        let (page, authorization, people) = self.search_stock_adjustments(&query, actor).await?;
        let meta = authorization.adjustment_list_meta();
        ensure_scope_version(query.paging.page, query.scope_version.as_deref(), meta.scope_version())?;
        let items = page.items.into_iter().map(|row| adjustment_row_view(row, &people)).collect();
        Ok(InventoryListPage::from_page(
            PageView { items, total: page.total, page: query.paging.page, page_size: query.paging.page_size },
            meta,
            authorization.adjustment_list_scope().is_empty(),
            ADJUSTMENT_SCOPE_SUMMARY,
            ADJUSTMENT_OWNERSHIP_BASIS,
        ))
    }

    async fn search_stock_adjustments(
        &self,
        query: &StockAdjustmentListQuery,
        actor: &AuditActor,
    ) -> Result<(
        persistence_core::PageResult<StockAdjustmentRow>,
        InventoryAuthorization,
        HashMap<String, AdjustmentPeopleFact>,
    )> {
        let db = self.db.clone();
        let authorization_port = std::sync::Arc::clone(&self.authorization);
        let catalog = std::sync::Arc::clone(&self.catalog_facts);
        let people_facts = std::sync::Arc::clone(&self.people_facts);
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let authorization = authorization_port.authorize(&actor, executor).await?;
                    if !authorization.actor_is_active() {
                        return Err(Error::Forbidden("当前账号无库存调整读取权限".to_string()));
                    }
                    let id_in = people_object_ids(people_facts.as_ref(), &query, executor).await?;
                    let search = super::search::sku_filter(
                        catalog.as_ref(),
                        query.q.as_deref(),
                        query.sku_id.as_ref(),
                        executor,
                    )
                    .await?;
                    let search = super::search::adjustment_filter(
                        &db,
                        search,
                        query.adjustment_id.as_deref(),
                        executor,
                    )
                    .await?;
                    let filter = adjustment_filter(&query, &authorization, search, id_in);
                    let page = db.stock_adjustments().search_stock_adjustments(&filter, executor).await?;
                    let ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
                    let people = people_facts.people_by_adjustment_ids(&ids, executor).await?;
                    Ok::<_, Error>((page, authorization, people))
                })
            })
            .await
    }

    /// 按主键批量读取调整单申请人与当前审批人，详情与列表同一口径。
    ///
    /// # 参数
    /// * `ids` - 调整单主键
    ///
    /// # 返回
    /// 返回人员事实映射。
    ///
    /// # 错误
    /// 端口未接线或读取失败。
    pub async fn adjustment_people_by_ids(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, AdjustmentPeopleFact>> {
        self.people_facts.people_by_adjustment_ids(ids, &mut NoTransaction).await
    }

    /// 在同一快照内加载表头并验证对象读取范围；拒绝结果隐藏资源存在性。
    pub async fn readable_stock_adjustment(&self, id: &str, actor: &AuditActor) -> Result<StockAdjustment> {
        let db = self.db.clone();
        let authorization_port = std::sync::Arc::clone(&self.authorization);
        let id = id.to_string();
        let actor = actor.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let authorization = authorization_port.authorize(&actor, executor).await?;
                    let adjustment = db
                        .inventory()
                        .stock_adjustment(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                    if !authorization.actor_is_active()
                        || !authorization.read_scope().covers(adjustment.warehouse_id.as_ref())
                    {
                        return Err(Error::NotFound("库存调整单不存在".to_string()));
                    }
                    Ok::<_, Error>(adjustment)
                })
            })
            .await
    }

    /// 按主键读取库存调整单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    pub async fn load_stock_adjustment(&self, id: &str) -> Result<StockAdjustment> {
        self.db
            .inventory()
            .stock_adjustment(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))
    }

    /// 按调整单主键读取全部明细。
    ///
    /// # 参数
    /// * `adjustment_id` - 调整单主键
    ///
    /// # 返回
    /// 返回该调整单的明细集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    pub async fn load_adjustment_lines(&self, adjustment_id: &str) -> Result<Vec<StockAdjustmentLine>> {
        let id = StockAdjustmentId::new(adjustment_id.to_string());
        Ok(self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&id), &mut NoTransaction)
            .await?)
    }

    /// 按来源单据读取过账流水。
    ///
    /// # 参数
    /// * `document_id` - 来源单据主键
    ///
    /// # 返回
    /// 返回该来源单据产生的库存流水。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    pub async fn load_movements_for_source_document(&self, document_id: &str) -> Result<Vec<StockMovement>> {
        Ok(self.db.inventory().movements_for_source_document(document_id, &mut NoTransaction).await?)
    }
}

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

fn adjustment_filter(
    query: &StockAdjustmentListQuery,
    authorization: &InventoryAuthorization,
    search: crate::repository::InventorySearch,
    id_in: Option<Vec<String>>,
) -> StockAdjustmentFilter {
    let (sort_by, sort_ascending) = query.paging.sort_selection();
    StockAdjustmentFilter {
        search,
        warehouse_ids: authorization
            .adjustment_list_scope()
            .repository_warehouse_ids(query.warehouse_id.clone()),
        status: query.status,
        prepared_by_ids: query.operator_user_ids.clone(),
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
        submitted_by: fact.and_then(|fact| fact.submitted_by.clone()),
        current_assignee: fact.and_then(|fact| fact.current_assignee.clone()),
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
            submitted_by: None,
            current_assignee: None,
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
    /// * `fact` - 人员事实；缺字段保持为空，不得填入 `created_by`
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
            quantity: line.quantity,
            direction: line.direction,
        }
    }
}

#[cfg(test)]
mod people_projection_tests {
    use super::*;

    #[test]
    fn list_projection_uses_snapshot_applicant_not_created_by() {
        let mut view = StockAdjustmentView {
            id: "adj-1".into(),
            adjustment_no: "ADJ-1".into(),
            warehouse_id: "wh-1".into(),
            reason_type: crate::entity::inventory::AdjustmentReasonType::StockGain,
            status: crate::entity::inventory::StockAdjustmentState::Draft,
            prepared_by: "operator-1".into(),
            submitted_by: None,
            current_assignee: None,
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
