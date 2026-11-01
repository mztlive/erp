//! 授权批次的查询累加器；计数、命中说明和分页共用同一筛选结果。
use application_core::AuditActor;
use erp_workflow::WorkflowAuthorizationPort;
use persistence_core::Executor;

use super::super::access::ActorAccess;
use super::super::query::{
    AUTHORIZED_SCAN_BATCH_SIZE, AuthorizedPage, AuthorizedPageCollector, matches_handler,
    matches_order_sources, queue_scope_version,
};
use super::super::{WorkItemFilter, WorkbenchObjectFactMap, WorkbenchReadService, dto};
use super::{SearchOptions, collect_options};
use crate::Result;

impl<A: WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 扫描全部仓储候选，逐批授权后仅保留请求页。
    /// # 参数
    /// `filter`、`query` 为服务端查询，`actor`、`access` 为真实账号范围，`executor` 为同一事务。
    /// # 返回
    /// 返回匹配总数、请求页、结果版本与授权候选项。
    /// # 错误
    /// 授权、资料或仓储读取失败时中止查询。
    pub(in crate::workbench) async fn authorized_page_fields(
        &self,
        filter: &WorkItemFilter,
        query: &dto::WorkItemListQuery,
        actor: &AuditActor,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedPage<dto::WorkItemFields>, String, SearchOptions)> {
        let mut candidates = filter.clone();
        candidates.query = None;
        let collector = AuthorizedPageCollector::new(query.page, query.page_size)?;
        let mut result =
            SearchPage { collector, result_version: String::new(), options: SearchOptions::new() };
        let mut scan = self.candidate_scan(&candidates, executor).await?;
        loop {
            let rows = scan.next_batch(AUTHORIZED_SCAN_BATCH_SIZE, executor).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            let mut facts = self
                .candidate_object_facts(
                    &rows,
                    filter.query.is_some() || query.filters != Default::default(),
                    executor,
                )
                .await?;
            let mut fields = self.authorize_queue_rows(rows, access, &mut facts, executor).await?;
            if filter.query.is_some() || query.filters != Default::default() {
                self.search_documents(&mut fields, executor).await?;
            }
            if query.filters.blocked {
                self.search_blockers(&mut fields, query.scope, actor, access, executor).await?;
            }
            result.extend(fields, &facts, query, filter.query.as_deref());
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        for values in result.options.values_mut() {
            values.sort_by(|a, b| a.label.cmp(&b.label));
        }
        Ok((result.collector.finish(), result.result_version, result.options))
    }
}
struct SearchPage {
    collector: AuthorizedPageCollector<dto::WorkItemFields>,
    result_version: String,
    options: SearchOptions,
}
impl SearchPage {
    fn extend(
        &mut self,
        fields: Vec<dto::WorkItemFields>,
        facts: &WorkbenchObjectFactMap,
        query: &dto::WorkItemListQuery,
        keyword: Option<&str>,
    ) {
        for mut field in fields {
            collect_options(&mut self.options, &field.search, &query.filters);
            let Some(hits) = field.search.matches(keyword) else {
                continue;
            };
            if !(query.filters.matches(&field.search)
                && matches_handler(&field, &query.handler_user_ids)
                && matches_order_sources(&field, facts, &query.sales_order_ids, &query.purchase_order_ids))
            {
                continue;
            }
            field.search_matches = hits;
            self.result_version =
                queue_scope_version(&self.result_version, &field.id, &field.task_version.to_string());
            self.collector.extend([field]);
        }
    }
}
