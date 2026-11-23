//! 审批工作台（`started` / `mine` / `managed` / `blocked`）按完整授权集检索，结果按页码返回，保留旧游标接口。
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use application_core::AuditActor;
use erp_workflow::service::approval::execution::runtime_service::RuntimeInstanceListQuery;
use erp_workflow::service::work_item::read::context::AuthorizedPageCollector;
use futures_util::future::BoxFuture;
use persistence_core::{Executor, Transactional};

use super::super::brief::BriefSection;
use super::super::{ApprovalListItem, ApprovalListPage, WorkbenchReadService};
use super::sources::SubjectKey;
use super::{SearchDocument, SearchOptions, WorkspaceFilters, approval_label, collect_options};
use crate::ports::approval_instance::ApprovalInstanceReadPort;
use crate::{Error, Result};

impl WorkbenchReadService {
    /// 准备审批工作台检索时，在同一快照内完成指定视图（`started` / `mine` / `managed` / `blocked`）的完整检索、筛选和分页。
    ///
    /// # 参数
    /// `runtime` 为审批授权服务，`actor` 为当前账号，`query` 为关键词查询；`filters` 为结构化筛选，`page` 从1开始。
    ///
    /// # 返回
    /// 返回完整筛选总数、当前页和授权候选选项。
    ///
    /// # 错误
    /// 非法页码、授权或查询失败时返回错误。
    #[inline(never)]
    pub fn approval_workspace_list(
        &self,
        runtime: Arc<dyn ApprovalInstanceReadPort>,
        actor: AuditActor,
        query: RuntimeInstanceListQuery,
        mut filters: WorkspaceFilters,
        page: u64,
    ) -> BoxFuture<'_, Result<ApprovalListPage>> {
        Box::pin(async move {
            if page == 0 {
                return Err(Error::ValidationError("页码必须大于0".into()));
            }
            if filters.date_field.as_deref().is_some_and(|v| v != "created")
                && (filters.date_from.is_some() || filters.date_to.is_some() || filters.undated)
            {
                return Err(Error::ValidationError("审批列表只支持按提交时间筛选".into()));
            }
            if filters.due.is_some() {
                return Err(Error::ValidationError("审批列表不支持截止快捷条件".into()));
            }
            if filters.date_field.is_none() {
                filters.date_field = Some("created".into());
            }

            let this = self.clone();
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        this.approval_search_page(runtime.as_ref(), &actor, query, &filters, page, executor)
                            .await
                    })
                })
                .await
        })
    }

    async fn approval_search_page(
        &self,
        runtime: &dyn ApprovalInstanceReadPort,
        actor: &AuditActor,
        mut query: RuntimeInstanceListQuery,
        filters: &WorkspaceFilters,
        page: u64,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalListPage> {
        let keyword = query.query.take();
        let limit = query.limit;
        let mut collector = AuthorizedPageCollector::new(page, limit)?;
        query.cursor = None;
        query.limit = 100;
        let mut options = SearchOptions::new();
        let mut counts = BTreeMap::new();

        loop {
            let (batch, next_cursor) = runtime.workspace_instance_page(actor, &query, executor).await?;
            let items = self.approval_list_summaries(batch, executor).await?;
            let documents = self.approval_search_documents(&items, executor).await?;

            for mut item in items {
                let doc = enriched_document(&item, &documents);
                collect_options(&mut options, &doc, filters);
                *counts.entry(item.instance.document_type.clone().unwrap_or_default()).or_insert(0) += 1;
                let Some(hits) = doc.matches(keyword.as_deref()) else {
                    continue;
                };
                if !filters.matches(&doc) {
                    continue;
                }
                item.search_matches = hits;
                collector.extend([item]);
            }

            let Some(next) = next_cursor else {
                break;
            };
            query.cursor = Some(next);
        }

        let result = collector.finish();
        Ok(ApprovalListPage {
            items: result.items,
            total: u64::try_from(result.total).unwrap_or_default(),
            next_cursor: None,
            document_type_counts: Some(counts),
            filter_options: options,
        })
    }
    async fn approval_search_documents(
        &self,
        items: &[ApprovalListItem],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<SubjectKey, SearchDocument>> {
        let keys = items
            .iter()
            .filter_map(|item| {
                Some((
                    item.instance.document_type.clone()?,
                    item.instance.document_id.clone()?,
                    item.instance.subject_version?.to_string(),
                ))
            })
            .collect::<Vec<_>>();
        self.submission_search(&keys, executor).await
    }
}

fn approval_document(item: &ApprovalListItem) -> SearchDocument {
    let row = &item.instance;
    let mut doc =
        SearchDocument { created_at: row.started_at, blocked: row.status == "BLOCKED", ..Default::default() };
    if let Some(kind) = &row.document_type {
        doc.facet("item_types", &format!("approval:{kind}"), &approval_label(kind));
    }
    let status = match row.status.as_str() {
        "RUNNING" => "审批中",
        "APPROVED" => "已通过",
        "REJECTED" => "已驳回",
        "CANCELLED" => "已撤回",
        "BLOCKED" => "审批受阻",
        _ => "待处理",
    };
    doc.facet("statuses", &row.status, status);
    if let Some(label) = &row.document_label {
        doc.text("单号", label);
    }
    if let Some(name) = &row.current_assignee_name {
        doc.text("当前审批人", name);
        if let Some(id) = &row.current_assignee_participant_id {
            doc.facet("handlers", id, name);
        }
    }

    if let Some(summary) = &item.document_summary {
        if let Some(party) = &summary.counterparty_label {
            doc.facet("counterparties", party, party);
            doc.text("往来方", party);
        }
        for line in &summary.brief_lines {
            doc.text("商品", &line.title);
        }
        let sections = summary
            .summary_sections
            .iter()
            .map(|section| BriefSection {
                label: section.label.clone(),
                value: section.value.clone(),
                numeric: section.numeric.unwrap_or_default(),
                object_id: section.object_id.clone(),
            })
            .collect::<Vec<_>>();
        super::append_sections(&mut doc, &sections);
    }
    doc
}

fn enriched_document(
    item: &ApprovalListItem,
    documents: &HashMap<SubjectKey, SearchDocument>,
) -> SearchDocument {
    let mut doc = approval_document(item);
    if let (Some(kind), Some(id), Some(version)) =
        (&item.instance.document_type, &item.instance.document_id, item.instance.subject_version)
        && let Some(source) = documents.get(&(kind.clone(), id.clone(), version.to_string()))
    {
        doc.merge(source);
    }
    if let (Some(kind), Some(id), Some(label)) =
        (&item.instance.document_type, &item.instance.document_id, &item.instance.document_label)
    {
        match kind.as_str() {
            "sales_order" | "voucher_sales_order" => doc.facet("sales_orders", id, label),
            "purchase_order" => doc.facet("purchase_orders", id, label),
            _ => {},
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use erp_workflow::service::approval::execution::runtime_service::RuntimeInstanceListItem;

    use super::*;
    use crate::workbench::ApprovalDocumentSummary;

    #[test]
    fn approval_document_indexes_blocked_status_counterparty_and_handler_facets() {
        let instance = RuntimeInstanceListItem {
            instance_id: "inst-1".into(),
            status: "BLOCKED".into(),
            current_round_no: 1,
            current_node_key: Some("finance".into()),
            current_node_name: Some("财务复核".into()),
            current_assignee_participant_id: Some("u-fin".into()),
            current_assignee_name: Some("李财务".into()),
            document_type: Some("sales_order".into()),
            document_id: Some("so-1".into()),
            document_label: Some("SO-2026-001".into()),
            subject_version: Some(2),
            total_amount: None,
            process_version: Some(1),
            started_at: Some(1_790_784_000),
            latest_rejection_summary: None,
        };
        let item = ApprovalListItem {
            search_matches: Vec::new(),
            instance,
            document_summary: Some(ApprovalDocumentSummary {
                root_business_object_id: "so-1".into(),
                counterparty_label: Some("星辰科技".into()),
                impact_summary: None,
                list_summary: "星辰科技".into(),
                summary_sections: Vec::new(),
                brief_lines: Vec::new(),
                brief_more_count: 0,
            }),
        };

        let doc = enriched_document(&item, &HashMap::new());
        assert!(doc.blocked);
        assert!(doc.matches(Some("星辰")).is_some());
        assert!(doc.matches(Some("SO-2026-001")).is_some());
        let filters = WorkspaceFilters::parse(Some(
            r#"{"blocked":true,"item_types":["approval:sales_order"],"counterparties":["星辰科技"],"handlers":["u-fin"]}"#,
        ))
        .unwrap();
        assert!(filters.matches(&doc));
        let other = WorkspaceFilters::parse(Some(r#"{"item_types":["approval:purchase_order"]}"#)).unwrap();
        assert!(!other.matches(&doc));
    }
}
