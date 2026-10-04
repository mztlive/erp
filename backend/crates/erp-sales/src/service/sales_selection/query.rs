//! 选品查询：列表、详情、预览与公开页。

use erp_core::common::time::Instant;
use persistence_core::{Executor, NoTransaction};

use super::SalesSelectionService;
use crate::dto::sales_selection::{
    PublicChoiceView, SalesSelectionBookletListParams, SalesSelectionBookletPage, SalesSelectionBookletView,
    SalesSelectionProposalView, SalesSelectionSessionView,
};
use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::repository::sales_selection::{SelectionBookFilter, validate_book_sort};
use crate::{Error, Result};

impl SalesSelectionService {
    /// 查询方案列表（执行器与已解析范围注入）。
    ///
    /// 调用方必须先经 DataScope v2 解析动作范围；计数与取数同一条件。
    ///
    /// # 参数
    /// * `params` - 筛选
    /// * `authorized_scope` - 已解析的方案责任范围
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回分页。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn proposal_list_with(
        &self,
        params: &crate::dto::sales_selection::SalesSelectionProposalListParams,
        authorized_scope: &crate::repository::sales_selection::SelectionReadScope,
        executor: &mut dyn Executor,
    ) -> Result<crate::dto::sales_selection::SalesSelectionProposalPage> {
        use application_core::{page_or_default, page_size_or_default};
        let filter = crate::repository::sales_selection::SelectionProposalFilter {
            authorized_customer_ids: params.authorized_customer_ids.clone(),
            authorized_scope: authorized_scope.clone(),
            owner_user_ids: params.owner_user_ids.clone(),
            org_unit_ids: params.org_unit_ids.clone(),
            customer_id: params.customer_id.clone(),
            booklet_id: params.booklet_id.clone(),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
        };
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let result = domain.search_proposals(&filter, executor).await?;
        Ok(crate::dto::sales_selection::SalesSelectionProposalPage {
            items: result.items.iter().map(Self::proposal_list_item).collect(),
            total: result.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页列出选品册（执行器与已解析范围注入）。
    ///
    /// 调用方必须先经 DataScope v2 解析动作范围；本方法只执行条件。
    ///
    /// # 参数
    /// * `params` - 筛选与分页参数
    /// * `authorized_scope` - 已解析的选品责任范围
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回列表页。
    ///
    /// # 错误
    /// 排序非法或查询失败时拒绝。
    pub async fn list_booklets_with(
        &self,
        params: &SalesSelectionBookletListParams,
        authorized_scope: &crate::repository::sales_selection::SelectionReadScope,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletPage> {
        let filter = booklet_filter(params, authorized_scope)?;
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let result = domain.search_booklets(&filter, executor).await?;
        Ok(SalesSelectionBookletPage {
            items: result.items.iter().map(Self::booklet_list_item).collect(),
            total: result.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 读取选品册详情。
    ///
    /// # 参数
    /// * `id` - 选品册身份
    ///
    /// # 返回
    /// 返回管理端详情，不含令牌明文。
    ///
    /// # 错误
    /// 不存在时返回未找到。
    pub async fn booklet_detail(&self, id: &str) -> Result<SalesSelectionBookletView> {
        let mut executor = NoTransaction;
        let booklet = self.load_booklet(id, &mut executor).await?;
        self.detail_view(&booklet, None, &mut executor).await
    }

    /// 组装详情视图。
    ///
    /// 组合层授权快照与写命令经本入口复用同一映射；公开页不走本入口。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `batch_id` - 指定批次；为空使用当前批次
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// 无可预览批次时拒绝。
    pub async fn detail_view(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: Option<String>,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let batch = batch_id.as_deref().or(booklet.current_batch_id.as_deref());
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
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

    /// 读取活动任务。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 有活动任务时返回。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub(super) async fn active_task_of(
        &self,
        booklet: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<Option<crate::entity::sales_selection::SalesSelectionPrepareTask>> {
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

    /// 读取方案详情。
    ///
    /// # 参数
    /// * `id` - 方案身份；若未命中则按选品册身份再查一次
    ///
    /// # 返回
    /// 返回内部方案视图。
    ///
    /// # 错误
    /// 尚无方案时返回未找到。
    pub async fn proposal_detail(&self, id: &str) -> Result<SalesSelectionProposalView> {
        let mut executor = NoTransaction;
        let proposal = self.db.sales_selection_proposals().find_by_id(id, &mut executor).await?;
        let proposal = match proposal {
            Some(item) => item,
            None => {
                let domain =
                    crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
                domain
                    .find_proposal_by_booklet(id, &mut executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("销售方案不存在".into()))?
            },
        };
        self.proposal_view_of(&proposal, &mut executor).await
    }

    /// 读取内部会话快照。
    ///
    /// 组合层已在调用方事务内完成对象重验；本方法只组装视图。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回当前会话选择。
    ///
    /// # 错误
    /// 无会话时返回未找到。
    pub async fn session_of(
        &self,
        booklet_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionSessionView> {
        self.session_view(booklet_id, executor).await
    }

    /// 读取内部会话快照。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    ///
    /// # 返回
    /// 返回当前会话选择。
    ///
    /// # 错误
    /// 无会话时返回未找到。
    pub async fn admin_session(&self, booklet_id: &str) -> Result<SalesSelectionSessionView> {
        let mut executor = NoTransaction;
        self.session_view(booklet_id, &mut executor).await
    }

    /// 组装内部会话视图。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回当前会话选择。
    ///
    /// # 错误
    /// 无会话时返回未找到。
    async fn session_view(
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

    /// 组装方案视图。
    ///
    /// 组合层授权快照与写命令经本入口复用同一映射。
    ///
    /// # 参数
    /// * `proposal` - 方案表头
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回方案视图。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
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

/// 由列表参数与已解析范围构造仓储过滤。
///
/// # 参数
/// * `params` - 列表参数
/// * `authorized_scope` - 调用方事务内已证明的责任范围
///
/// # 返回
/// 返回仓储过滤，排序默认创建时间倒序。
///
/// # 错误
/// 排序字段非法时拒绝。
fn booklet_filter(
    params: &SalesSelectionBookletListParams,
    authorized_scope: &crate::repository::sales_selection::SelectionReadScope,
) -> Result<SelectionBookFilter> {
    use application_core::{page_or_default, page_size_or_default};
    let (sort_by, ascending) = validate_book_sort(None, false)?;
    Ok(SelectionBookFilter {
        authorized_scope: authorized_scope.clone(),
        owner_user_ids: params.owner_user_ids.clone(),
        org_unit_ids: params.org_unit_ids.clone(),
        authorized_customer_ids: params.authorized_customer_ids.clone(),
        customer_id: params.customer_id.clone(),
        form: params.form,
        status: params.status,
        submit_mode: params.submit_mode,
        page: page_or_default(params.page),
        page_size: page_size_or_default(params.page_size),
        sort_by: Some(sort_by),
        sort_ascending: ascending,
        q: params.q.clone(),
    })
}

/// 当前批次按每档最近成功任务汇总报告，按档重生成不丢失其他档结果。
fn merge_task_reports(
    mut tasks: Vec<crate::entity::sales_selection::SalesSelectionPrepareTask>,
) -> Option<crate::entity::sales_selection::SalesSelectionPrepareTask> {
    tasks.sort_by(|a, b| b.finished_at.cmp(&a.finished_at).then_with(|| b.base.id.cmp(&a.base.id)));
    let mut tasks = tasks.into_iter();
    let mut latest = tasks.next()?;
    let mut seen = std::collections::BTreeSet::new();
    latest.tier_reports = std::mem::take(&mut latest.tier_reports)
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
            authorized_customer_ids: None,
            scope_version: None,
            owner_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            customer_id: Some("cust-1".into()),
            form: None,
            status: None,
            submit_mode: None,
            q: None,
            page: None,
            page_size: None,
        };
        let scope = crate::repository::sales_selection::SelectionReadScope {
            roles: vec![crate::repository::sales_selection::SelectionScopeClause {
                company: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let filter = booklet_filter(&params, &scope).unwrap();
        assert_eq!(filter.customer_id.as_deref(), Some("cust-1"));
        assert_eq!(filter.page, 1);
        assert_eq!(filter.sort_by.as_deref(), Some("created_at"));
        assert!(!filter.sort_ascending);
    }
}
