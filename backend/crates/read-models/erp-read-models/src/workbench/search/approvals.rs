//! 本人发起审批按完整授权集检索，结果按页码返回，保留旧游标接口。
use std::collections::{BTreeMap, HashMap};

use application_core::AuditActor;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::service::approval::execution::runtime_service::{
    ApprovalRuntimeService, RuntimeInstanceListQuery,
};
use persistence_core::{Executor, Transactional};

use super::super::brief::BriefSection;
use super::super::query::AuthorizedPageCollector;
use super::super::{ApprovalListItem, ApprovalListPage, WorkbenchReadService};
use super::sources::SubjectKey;
use super::{SearchDocument, SearchOptions, WorkspaceFilters, approval_label, collect_options};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 在同一快照内完成本人审批的完整检索、筛选和分页。
    /// # 参数
    /// `runtime` 为审批授权服务，`actor` 为当前账号，`query` 为关键词查询；`filters` 为结构化筛选，`page` 从1开始。
    /// # 返回
    /// 返回完整筛选总数、当前页和授权候选选项。
    /// # 错误
    /// 非法页码、授权或查询失败时返回错误。
    pub async fn approval_workspace_list(
        &self,
        runtime: ApprovalRuntimeService<A>,
        actor: AuditActor,
        query: RuntimeInstanceListQuery,
        mut filters: WorkspaceFilters,
        page: u64,
    ) -> Result<ApprovalListPage> {
        if page == 0 {
            return Err(Error::ValidationError("页码必须大于0".into()));
        }
        if filters.date_field.as_deref().is_some_and(|v| v != "created")
            && (filters.date_from.is_some() || filters.date_to.is_some() || filters.undated)
        {
            return Err(Error::ValidationError("我发起的审批只支持按提交时间筛选".into()));
        }
        if filters.due.is_some() {
            return Err(Error::ValidationError("我发起的审批不支持截止快捷条件".into()));
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
                    this.approval_search_page(&runtime, &actor, query, &filters, page, executor).await
                })
            })
            .await
    }

    async fn approval_search_page(
        &self,
        runtime: &ApprovalRuntimeService<A>,
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
            let (batch, next_cursor) = runtime.started_instance_page_with(actor, &query, executor).await?;
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
        doc.sections(&sections);
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
