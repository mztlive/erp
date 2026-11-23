//! 选品查询：列表、详情、预览与公开页。

use std::collections::BTreeSet;
use std::mem;

use application_core::{page_or_default, page_size_or_default};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::SalesSelectionService;
use crate::dto::sales_selection::{
    PublicChoiceView, SalesSelectionBookletListParams, SalesSelectionBookletPage, SalesSelectionBookletView,
    SalesSelectionProposalListParams, SalesSelectionProposalPage, SalesSelectionProposalView,
    SalesSelectionSessionView,
};
use crate::entity::sales_selection::{
    SalesSelectionBooklet, SalesSelectionPrepareTask, SalesSelectionProposal,
};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::repository::sales_selection::{
    SalesSelectionDomainRepository, SelectionBookFilter, SelectionProposalFilter, SelectionReadScope,
    validate_book_sort,
};
use crate::{Error, Result};

impl SalesSelectionService {
    /// 在调用方已解析的授权范围和读取边界内查询方案列表，分页不绕开来源限制。
    ///
    /// 调用方先通过统一授权查询生成范围；计数与取数使用同一条件，交付前由调用方复验。
    /// 返回分页。
    /// 查询失败。
    ///
    /// 输入约定：
    /// - `params`：筛选
    /// - `authorized_scope`：已解析的方案责任范围
    pub async fn proposal_list_with(
        &self,
        params: &SalesSelectionProposalListParams,
        authorized_scope: &SelectionReadScope,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposalPage> {
        let filter = SelectionProposalFilter {
            owner_user_ids: params.owner_user_ids.clone(),
            org_unit_ids: params.org_unit_ids.clone(),
            customer_id: params.customer_id.clone(),
            booklet_id: params.booklet_id.clone(),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
        };

        let domain = SalesSelectionDomainRepository::new(&self.db);
        let result = domain.search_proposals(&filter, authorized_scope, executor).await?;
        authorized_scope.ensure_current(Database::SALES_SELECTION_PROPOSALS, executor)?;
        Ok(SalesSelectionProposalPage {
            items: result.items.iter().map(Self::proposal_list_item).collect(),
            total: result.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 用注入的执行器和已解析责任范围分页读取选品册，保持授权与业务筛选在同一次查询。
    ///
    /// 调用方先通过统一授权查询生成范围；本方法只执行条件，交付前由调用方复验。
    /// 返回列表页。
    /// 排序非法或查询失败时拒绝。
    ///
    /// 输入约定：
    /// - `params`：筛选与分页参数
    /// - `authorized_scope`：已解析的选品责任范围
    pub async fn list_booklets_with(
        &self,
        params: &SalesSelectionBookletListParams,
        authorized_scope: &SelectionReadScope,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletPage> {
        let filter = booklet_filter(params)?;
        let domain = SalesSelectionDomainRepository::new(&self.db);
        let result = domain.search_booklets(&filter, authorized_scope, executor).await?;
        let ids = result.items.iter().map(|book| book.base.id.clone()).collect::<Vec<_>>();
        let summaries = domain.booklet_summaries(&ids, executor).await?;

        let items = result
            .items
            .iter()
            .map(|book| {
                let mut view = Self::booklet_list_item(book);
                if let Some(summary) = summaries.iter().find(|row| row.id == view.id) {
                    view.display_count = summary.display_count;
                    view.proposal_no = summary.proposal_no.clone();
                }
                view
            })
            .collect();

        authorized_scope.ensure_current(Database::SALES_SELECTION_BOOKLETS, executor)?;
        Ok(SalesSelectionBookletPage {
            items,
            total: result.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 组装册详情前加载有效陈列和对应准备任务，以便在同一执行器返回当前结果和准备状态。
    ///
    /// 组合层授权快照与写命令经本入口复用同一映射；公开页不走本入口。
    /// 无可预览批次时拒绝。
    ///
    /// 输入约定：
    /// - `booklet`：选品册
    /// - `batch_id`：指定批次；为空使用当前批次
    pub async fn detail_view(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: Option<String>,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let batch = batch_id.as_deref().or(booklet.current_batch_id.as_deref());
        let domain = SalesSelectionDomainRepository::new(&self.db);
        let items = if let Some(batch) = batch {
            domain.list_effective_items(&booklet.base.id, batch, executor).await?
        } else {
            Vec::new()
        };
        let task = self.active_task_of(booklet, executor).await?;

        let mut view = Self::booklet_view(booklet, &items, task.as_ref(), None);
        if let Some(proposal_id) = booklet.proposal_id.as_ref()
            && let Some(proposal) =
                self.db.sales_selection_proposals().find_by_id(proposal_id.as_ref(), executor).await?
        {
            view.proposal_no = Some(proposal.proposal_no);
        }
        Ok(view)
    }

    // 查询册详情时定位正在运行或排队的准备任务，供前台轮询进度。
    // 结果：有活动任务时返回该任务。没有活动任务且当前结果批次没有可汇总任务，或活动任务已不存在时返回 `None`。
    // 查询失败时返回仓储错误。
    //
    // 输入约定：
    // - `booklet`：选品册
    pub(super) async fn active_task_of(
        &self,
        booklet: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesSelectionPrepareTask>> {
        let Some(task_id) = booklet.active_task_id.as_deref() else {
            let tasks = self
                .db
                .sales_selection_prepare_tasks()
                .list_by_result_batch(&booklet.base.id, booklet.current_batch_id.as_deref(), executor)
                .await?;
            return Ok(merge_task_reports(tasks));
        };
        Ok(self.db.sales_selection_prepare_tasks().find_by_id(task_id, executor).await?)
    }

    /// 管理端需要内部选择快照时，用当前执行器读取该册会话及其已保存选择。
    ///
    /// 组合层已在调用方事务内完成对象重验；本方法只组装视图。
    /// 返回当前会话选择。
    /// 失败条件：无会话时返回未找到。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册
    pub async fn session_of(
        &self,
        booklet_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionSessionView> {
        let booklet = self.load_booklet(booklet_id, executor).await?;
        let session = self
            .db
            .sales_selection_sessions()
            .find_by_booklet(booklet_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("该选品册尚无选品会话".into()))?;

        let selections: Vec<PublicChoiceView> = session
            .choices
            .iter()
            .map(|choice| PublicChoiceView {
                item_id: choice.display_item_id.to_string(),
                quantity: choice.quantity,
                line_amount: None,
            })
            .collect();
        Ok(SalesSelectionSessionView {
            book_id: booklet.base.id,
            expected_version: session.session_version,
            selections,
            total_amount: None,
            updated_at: Instant::from_unix_secs(session.base.updated_at as i64),
        })
    }

    /// 查询单份选品方案时加载冻结陈列行和 SKU 行，再投影为详情响应。
    ///
    /// 组合层授权快照与写命令经本入口复用同一映射。
    /// 查询失败时返回仓储错误。
    ///
    /// 输入约定：
    /// - `proposal`：方案表头
    pub async fn proposal_view_of(
        &self,
        proposal: &SalesSelectionProposal,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposalView> {
        let proposal_id = proposal.base.id.as_str();
        let display_lines =
            self.db.sales_selection_proposal_display_lines().list_by_proposal(proposal_id, executor).await?;
        let sku_lines =
            self.db.sales_selection_proposal_sku_lines().list_by_proposal(proposal_id, executor).await?;
        Ok(Self::proposal_view(proposal, &display_lines, &sku_lines))
    }
}

// 由列表参数与已解析范围构造仓储过滤。
// 返回仓储过滤，排序默认创建时间倒序。
// 排序字段非法时拒绝。
//
// 输入约定：
// - `params`：列表参数
// - `authorized_scope`：调用方事务内已证明的责任范围
fn booklet_filter(params: &SalesSelectionBookletListParams) -> Result<SelectionBookFilter> {
    let (sort_by, ascending) = validate_book_sort(None, false)?;
    Ok(SelectionBookFilter {
        owner_user_ids: params.owner_user_ids.clone(),
        org_unit_ids: params.org_unit_ids.clone(),
        customer_id: params.customer_id.clone(),
        form: params.form,
        status: params.status,
        page: page_or_default(params.page),
        page_size: page_size_or_default(params.page_size),
        sort_by: Some(sort_by),
        sort_ascending: ascending,
        q: params.q.clone(),
    })
}

// 组装当前批次档位报告时逐档取最近成功结果，保证某档重生成不抹掉其他档位的报告。
fn merge_task_reports(mut tasks: Vec<SalesSelectionPrepareTask>) -> Option<SalesSelectionPrepareTask> {
    tasks.sort_by(|a, b| b.finished_at.cmp(&a.finished_at).then_with(|| b.base.id.cmp(&a.base.id)));
    let mut tasks = tasks.into_iter();
    let mut latest = tasks.next()?;
    let mut seen = BTreeSet::new();
    latest.tier_reports = mem::take(&mut latest.tier_reports)
        .into_iter()
        .chain(tasks.flat_map(|task| task.tier_reports))
        .filter(|report| seen.insert(report.tier_id.clone()))
        .collect();
    Some(latest)
}

#[cfg(test)]
mod tests {
    use super::booklet_filter;
    use crate::dto::sales_selection::SalesSelectionBookletListParams;

    #[test]
    fn list_filter_defaults_to_created_desc() {
        let params = SalesSelectionBookletListParams {
            scope_version: None,
            owner_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            customer_id: Some("cust-1".into()),
            form: None,
            status: None,
            q: None,
            page: None,
            page_size: None,
        };
        let filter = booklet_filter(&params).unwrap();
        assert_eq!(filter.customer_id.as_deref(), Some("cust-1"));
        assert_eq!(filter.page, 1);
        assert_eq!(filter.sort_by.as_deref(), Some("created_at"));
        assert!(!filter.sort_ascending);
    }
}
