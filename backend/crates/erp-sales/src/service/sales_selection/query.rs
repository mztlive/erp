//! 选品查询：列表、详情、预览与公开页。

use erp_core::common::time::Instant;
use persistence_core::{Executor, NoTransaction};

use crate::dto::sales_selection::{
    PublicChoiceView, PublicReceiptView, PublicSelectionPageKind, PublicSelectionPageView,
    SalesSelectionBookletListParams, SalesSelectionBookletPage, SalesSelectionBookletView,
    SalesSelectionProposalView, SalesSelectionSessionView,
};
use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::repository::sales_selection::{validate_book_sort, SelectionBookFilter};
use crate::repository::SalesSelectionExt;
use crate::{Error, Result};

use super::SalesSelectionService;

impl SalesSelectionService {
    /// 分页列出选品册。
    ///
    /// # 参数
    /// * `params` - 筛选与分页参数
    ///
    /// # 返回
    /// 返回列表页。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn list_booklets(
        &self,
        params: SalesSelectionBookletListParams,
    ) -> Result<SalesSelectionBookletPage> {
        let mut executor = NoTransaction;
        self.list_booklets_with(&params, &mut executor).await
    }

    /// 查询方案列表。
    ///
    /// # 参数
    /// * `params` - 筛选
    ///
    /// # 返回
    /// 返回分页。
    ///
    /// # 错误
    /// 查询失败。
    pub async fn proposal_list(
        &self,
        params: crate::dto::sales_selection::SalesSelectionProposalListParams,
    ) -> Result<crate::dto::sales_selection::SalesSelectionProposalPage> {
        let mut executor = NoTransaction;
        let mut items = self
            .db
            .sales_selection_proposals()
            .list_authorized(
                params.customer_id.as_deref(),
                params.booklet_id.as_deref(),
                params.authorized_customer_ids.as_deref(),
                &mut executor,
            )
            .await?;
        items.sort_by(|a, b| {
            b.submitted_at
                .cmp(&a.submitted_at)
                .then_with(|| b.base.id.cmp(&a.base.id))
        });
        let total = i64::try_from(items.len()).unwrap_or(i64::MAX);
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20);
        let skip = ((page.saturating_sub(1)) * u64::from(page_size)) as usize;
        let page_items = items
            .into_iter()
            .skip(skip)
            .take(page_size as usize)
            .map(|item| Self::proposal_list_item(&item))
            .collect();
        Ok(crate::dto::sales_selection::SalesSelectionProposalPage {
            items: page_items,
            total,
            page,
            page_size,
        })
    }

    /// 分页列出选品册（执行器注入）。
    ///
    /// # 参数
    /// * `params` - 筛选与分页参数
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
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletPage> {
        let filter = booklet_filter(params)?;
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

    /// 预览某批次陈列。
    ///
    /// # 参数
    /// * `id` - 选品册身份
    /// * `batch_id` - 准备批次；为空使用当前批次
    ///
    /// # 返回
    /// 返回该批次的详情视图。
    ///
    /// # 错误
    /// 无可预览批次时拒绝。
    pub async fn preview_booklet(
        &self,
        id: &str,
        batch_id: Option<String>,
    ) -> Result<SalesSelectionBookletView> {
        let mut executor = NoTransaction;
        let booklet = self.load_booklet(id, &mut executor).await?;
        self.detail_view(&booklet, batch_id, &mut executor).await
    }

    /// 组装详情视图。
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
    pub(super) async fn detail_view(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: Option<String>,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let batch = batch_id.or_else(|| booklet.current_batch_id.clone());
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let items = if let Some(batch) = batch {
            domain
                .list_effective_items(&booklet.base.id, &batch, executor)
                .await?
        } else {
            Vec::new()
        };
        let task = self.active_task_of(booklet, executor).await?;
        let mut view = Self::booklet_view(booklet, &items, task.as_ref(), None);
        if let Some(proposal_id) = booklet.proposal_id.as_ref() {
            if let Some(proposal) = self
                .db
                .sales_selection_proposals()
                .find_by_id(proposal_id.as_ref(), executor)
                .await?
            {
                view.proposal_no = Some(proposal.proposal_no);
            }
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
        Ok(self
            .db
            .sales_selection_prepare_tasks()
            .find_by_id(task_id, executor)
            .await?)
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
        let proposal = self
            .db
            .sales_selection_proposals()
            .find_by_id(id, &mut executor)
            .await?;
        let proposal = match proposal {
            Some(item) => item,
            None => {
                let domain =
                    crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
                domain
                    .find_proposal_by_booklet(id, &mut executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("销售方案不存在".into()))?
            }
        };
        self.proposal_view_of(&proposal, &mut executor).await
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
        let booklet = self.load_booklet(booklet_id, &mut executor).await?;
        let session = self
            .db
            .sales_selection_sessions()
            .find_by_booklet(booklet_id, &mut executor)
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
    /// # 参数
    /// * `proposal` - 方案表头
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回方案视图。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub(super) async fn proposal_view_of(
        &self,
        proposal: &SalesSelectionProposal,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposalView> {
        let proposal_id = proposal.base.id.clone();
        let display_lines = self
            .db
            .sales_selection_proposal_display_lines()
            .list_by_proposal(&proposal_id, executor)
            .await?;
        let sku_lines = self
            .db
            .sales_selection_proposal_sku_lines()
            .list_by_proposal(&proposal_id, executor)
            .await?;
        Ok(Self::proposal_view(proposal, &display_lines, &sku_lines))
    }

    /// 按令牌哈希读取公开页。
    ///
    /// # 参数
    /// * `token_hash` - 令牌查找哈希
    /// * `now` - 服务端时间
    ///
    /// # 返回
    /// 返回选择页、只读回执或结束态，不泄漏内部字段。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误；未知令牌按结束态返回。
    pub async fn public_page_by_hash(
        &self,
        token_hash: &str,
        now: Instant,
    ) -> Result<PublicSelectionPageView> {
        let mut executor = NoTransaction;
        let booklet = self
            .db
            .sales_selection_booklets()
            .find_by_token_hash(token_hash, &mut executor)
            .await?;
        let Some(booklet) = booklet else {
            return Ok(Self::public_page(
                PublicSelectionPageKind::Ended,
                &ended_booklet(),
                &[],
                None,
                None,
            ));
        };
        self.public_page_for(&booklet, now, &mut executor).await
    }

    /// 按册状态组装公开页。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `now` - 服务端时间
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回选择页、回执或结束态。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn public_page_for(
        &self,
        booklet: &SalesSelectionBooklet,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        if booklet
            .status
            .public_is_ended(booklet.link_revoked, booklet.is_expired(now))
        {
            return Ok(Self::public_page(
                PublicSelectionPageKind::Ended,
                booklet,
                &[],
                None,
                None,
            ));
        }
        if booklet.status == crate::entity::sales_selection::BookletStatus::Submitted {
            return self.receipt_page(booklet, executor).await;
        }
        self.selecting_page(booklet, executor).await
    }

    /// 组装可选择页。
    ///
    /// # 参数
    /// * `booklet` - 已发布选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回陈列与当前选择。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn selecting_page(
        &self,
        booklet: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let batch = booklet.current_batch_id.clone().unwrap_or_default();
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let items = domain
            .list_effective_items(&booklet.base.id, &batch, executor)
            .await?;
        let session = domain.find_session_by_booklet(&booklet.base.id, executor).await?;
        Ok(Self::public_page(
            PublicSelectionPageKind::Selecting,
            booklet,
            &items,
            session.as_ref(),
            None,
        ))
    }

    /// 组装只读回执页。
    ///
    /// # 参数
    /// * `booklet` - 已提交选品册
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回回执页。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn receipt_page(
        &self,
        booklet: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let proposal = domain
            .find_proposal_by_booklet(&booklet.base.id, executor)
            .await?;
        let receipt = match proposal {
            Some(item) => {
                let choices = self.receipt_items(&item.base.id, executor).await?;
                Some(PublicReceiptView {
                    proposal_no: item.proposal_no.clone(),
                    submitted_at: item.submitted_at,
                    customer_name: item.customer_name.clone(),
                    items: choices,
                    total_amount: item.total_amount,
                })
            }
            None => None,
        };
        let batch = booklet.current_batch_id.clone().unwrap_or_default();
        let items = domain
            .list_effective_items(&booklet.base.id, &batch, executor)
            .await?;
        let session = domain.find_session_by_booklet(&booklet.base.id, executor).await?;
        Ok(Self::public_page(
            PublicSelectionPageKind::Receipt,
            booklet,
            &items,
            session.as_ref(),
            receipt,
        ))
    }

    /// 回执明细行。
    ///
    /// # 参数
    /// * `proposal_id` - 方案身份
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回回证明细。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub(super) async fn receipt_items(
        &self,
        proposal_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PublicChoiceView>> {
        let lines = self
            .db
            .sales_selection_proposal_display_lines()
            .list_by_proposal(proposal_id, executor)
            .await?;
        Ok(lines
            .into_iter()
            .map(|line| PublicChoiceView {
                item_id: line.display_item_id.to_string(),
                quantity: line.quantity,
                line_amount: line.line_amount,
            })
            .collect())
    }
}

/// 由列表参数构造仓储过滤。
///
/// # 参数
/// * `params` - 列表参数
///
/// # 返回
/// 返回仓储过滤，排序默认创建时间倒序。
///
/// # 错误
/// 排序字段非法时拒绝。
fn booklet_filter(params: &SalesSelectionBookletListParams) -> Result<SelectionBookFilter> {
    use application_core::{page_or_default, page_size_or_default};
    let (sort_by, ascending) = validate_book_sort(None, false)?;
    Ok(SelectionBookFilter {
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

/// 构造结束态占位册。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回最小占位册，仅用于结束态映射。
///
/// # 错误
/// 无。
fn ended_booklet() -> SalesSelectionBooklet {
    use crate::entity::sales_selection::{PoolFilterSnapshot, PoolSource, SelectionForm, SubmitMode};
    use crate::entity::sales_selection::{PoolSourceKind, SalesSelectionBookletData};
    use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId};
    SalesSelectionBooklet::new(
        SalesSelectionBookletId::new("ended"),
        SalesSelectionBookletData {
            customer_id: CustomerAccountId::new("ended"),
            customer_no: "ended".into(),
            customer_name: "ended".into(),
            form: SelectionForm::SingleSku,
            submit_mode: SubmitMode::MallRedeem,
            pool_source: PoolSource {
                kind: PoolSourceKind::Filter,
                filter: Some(PoolFilterSnapshot::default()),
                sku_ids: None,
            },
            tiers: Vec::new(),
            created_by: "system".into(),
        },
    )
    .expect("结束态占位册必然合法")
}

/// 当前批次按每档最近成功任务汇总报告，按档重生成不丢失其他档结果。
fn merge_task_reports(
    mut tasks: Vec<crate::entity::sales_selection::SalesSelectionPrepareTask>,
) -> Option<crate::entity::sales_selection::SalesSelectionPrepareTask> {
    tasks.sort_by(|a, b| {
        b.finished_at
            .cmp(&a.finished_at)
            .then_with(|| b.base.id.cmp(&a.base.id))
    });
    let mut latest = tasks.first()?.clone();
    let mut seen = std::collections::BTreeSet::new();
    latest.tier_reports = tasks
        .into_iter()
        .flat_map(|task| task.tier_reports)
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
            customer_id: Some("cust-1".into()),
            form: None,
            status: None,
            submit_mode: None,
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
