//! 授权批次的查询累加器；计数、命中说明和分页共用同一筛选结果。
use persistence_core::Executor;

use super::super::access::QueueAccess;
use super::super::context::{AuthorizedPage, AuthorizedPageCollector, queue_scope_version};
use super::super::query::{AUTHORIZED_SCAN_BATCH_SIZE, matches_handler};
use super::super::{WorkItemFilter, WorkItemReadService, dto};
use super::{SearchOptions, collect_options};
use crate::Result;

impl WorkItemReadService {
    /// 扫描数据库授权后的任务，完成业务筛选后仅保留请求页。
    /// # 参数
    /// `filter`、`query` 为服务端查询，`actor`、`access` 为真实账号范围，`executor` 为同一事务。
    /// # 返回
    /// 返回匹配总数、请求页、结果版本与授权候选项。
    /// # 错误
    /// 授权、资料或仓储读取失败时中止查询。
    pub(in crate::service::work_item::read) async fn authorized_page_fields(
        &self,
        filter: &WorkItemFilter,
        query: &dto::WorkItemListQuery,
        access: &QueueAccess,
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

            let mut fields = rows.into_iter().map(dto::WorkItemFields::from).collect::<Vec<_>>();
            for field in &mut fields {
                field.search = super::task_document(field);
            }
            if query.filters.blocked {
                self.search_blockers(&mut fields, query.scope, access, executor).await?;
            }
            result.extend(fields, query, filter.query.as_deref());
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }

        for key in ["handlers", "sales_orders", "purchase_orders"] {
            result.options.remove(key);
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
        query: &dto::WorkItemListQuery,
        keyword: Option<&str>,
    ) {
        for mut field in fields {
            collect_options(&mut self.options, &field.search, &query.filters);
            let Some(hits) = field.search.matches(keyword) else {
                continue;
            };
            if !(query.filters.matches(&field.search) && matches_handler(&field, &query.handler_user_ids)) {
                continue;
            }
            field.search_matches = hits;
            self.result_version =
                queue_scope_version(&self.result_version, &field.id, &field.task_version.to_string());
            self.collector.extend([field]);
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::super::{WorkspaceFilters, task_document};
    use super::*;
    use crate::dto::work_item::{WorkItemListParams, WorkItemScope};
    use crate::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };

    fn task() -> WorkItem {
        WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "account".into(),
                subject_version: "1".into(),
                owner_role: "finance".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "user".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn multiple_source_ids_and_current_handler_filter_before_total_and_pagination() {
        let params: WorkItemListParams =
            serde_json::from_value(serde_json::json!({"scope":"mine", "page":2, "page_size":1})).unwrap();
        let mut query = params.normalized().unwrap();
        query.filters = WorkspaceFilters::parse(Some(
            r#"{"sales_orders":["same","other"],"purchase_orders":["same"],"handlers":["user"]}"#,
        ))
        .unwrap();
        query.merge_filters().unwrap();
        assert_eq!(query.scope, WorkItemScope::Mine);

        let mut page = SearchPage {
            collector: AuthorizedPageCollector::new(2, 1).unwrap(),
            result_version: String::new(),
            options: SearchOptions::new(),
        };
        let rows = (0..6)
            .map(|index| {
                let mut item =
                    task().with_link("sales_order", if index == 0 { "wrong" } else { "same" }).unwrap();
                if index != 1 {
                    item = item.with_link("purchase_order", "same").unwrap();
                }
                if index == 2 {
                    item.owner_user_id = Some("other-user".into());
                }
                item.base.id = format!("task-{index}");
                let mut fields = dto::WorkItemFields::from(item);
                fields.search = task_document(&fields);
                fields
            })
            .collect();

        page.extend(rows, &query, None);
        let result = page.collector.finish();
        assert_eq!(result.total, 3);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "task-4");
    }
    #[test]
    fn owner_ids_do_not_become_person_name_keyword_search() {
        let fields = dto::WorkItemFields::from(task());
        let document = task_document(&fields);
        assert!(document.matches(Some("user")).is_none());
        assert!(document.matches(Some("张三")).is_none());
        assert!(document.matches(Some("account")).is_some());
        assert!(WorkspaceFilters { handlers: vec!["user".into()], ..Default::default() }.matches(&document));
        assert!(!WorkspaceFilters { handlers: vec!["张三".into()], ..Default::default() }.matches(&document));
    }
}
