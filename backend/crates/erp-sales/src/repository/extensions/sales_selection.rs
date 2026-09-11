//! 销售选品集合名与仓储访问器。

use super::super::owned::sales_selection::{
    SalesSelectionBookletRepository, SalesSelectionDisplayItemRepository,
    SalesSelectionIdempotencyRepository, SalesSelectionPoolMemberRepository,
    SalesSelectionPrepareTaskRepository, SalesSelectionProposalDisplayLineRepository,
    SalesSelectionProposalRepository, SalesSelectionProposalSkuLineRepository,
    SalesSelectionSessionRepository,
};
use mongodb::Database;

/// 销售选品仓储访问器。
pub trait SalesSelectionExt {
    /// 选品册集合。
    const SALES_SELECTION_BOOKLETS: &'static str = "sales_selection_booklets";
    /// 陈列项集合。
    const SALES_SELECTION_DISPLAY_ITEMS: &'static str = "sales_selection_display_items";
    /// 商品池成员集合。
    const SALES_SELECTION_POOL_MEMBERS: &'static str = "sales_selection_pool_members";
    /// 准备任务集合。
    const SALES_SELECTION_PREPARE_TASKS: &'static str = "sales_selection_prepare_tasks";
    /// 会话集合。
    const SALES_SELECTION_SESSIONS: &'static str = "sales_selection_sessions";
    /// 方案集合。
    const SALES_SELECTION_PROPOSALS: &'static str = "sales_selection_proposals";
    /// 方案陈列行集合。
    const SALES_SELECTION_PROPOSAL_DISPLAY_LINES: &'static str = "sales_selection_proposal_display_lines";
    /// 方案 SKU 行集合。
    const SALES_SELECTION_PROPOSAL_SKU_LINES: &'static str = "sales_selection_proposal_sku_lines";
    /// 幂等集合。
    const SALES_SELECTION_IDEMPOTENCY: &'static str = "sales_selection_idempotency";
    /// 公开限流窗口集合。
    const SALES_SELECTION_RATE_WINDOWS: &'static str = "sales_selection_rate_windows";

    /// 选品册仓储。
    ///
    /// # 返回
    /// 返回选品册仓储。
    fn sales_selection_booklets(&self) -> SalesSelectionBookletRepository<'_>;
    /// 陈列项仓储。
    ///
    /// # 返回
    /// 返回陈列项仓储。
    fn sales_selection_display_items(&self) -> SalesSelectionDisplayItemRepository<'_>;
    /// 商品池成员仓储。
    ///
    /// # 返回
    /// 返回商品池成员仓储。
    fn sales_selection_pool_members(&self) -> SalesSelectionPoolMemberRepository<'_>;
    /// 准备任务仓储。
    ///
    /// # 返回
    /// 返回准备任务仓储。
    fn sales_selection_prepare_tasks(&self) -> SalesSelectionPrepareTaskRepository<'_>;
    /// 会话仓储。
    ///
    /// # 返回
    /// 返回会话仓储。
    fn sales_selection_sessions(&self) -> SalesSelectionSessionRepository<'_>;
    /// 方案仓储。
    ///
    /// # 返回
    /// 返回方案仓储。
    fn sales_selection_proposals(&self) -> SalesSelectionProposalRepository<'_>;
    /// 方案陈列行仓储。
    ///
    /// # 返回
    /// 返回方案陈列行仓储。
    fn sales_selection_proposal_display_lines(&self) -> SalesSelectionProposalDisplayLineRepository<'_>;
    /// 方案 SKU 行仓储。
    ///
    /// # 返回
    /// 返回方案 SKU 行仓储。
    fn sales_selection_proposal_sku_lines(&self) -> SalesSelectionProposalSkuLineRepository<'_>;
    /// 幂等仓储。
    ///
    /// # 返回
    /// 返回幂等仓储。
    fn sales_selection_idempotency(&self) -> SalesSelectionIdempotencyRepository<'_>;
    /// 公开限流仓储。
    ///
    /// # 返回
    /// 返回限流仓储。
    fn sales_selection_rate(
        &self,
    ) -> crate::repository::owned::sales_selection::SalesSelectionRateRepository<'_>;
}

impl SalesSelectionExt for Database {
    fn sales_selection_booklets(&self) -> SalesSelectionBookletRepository<'_> {
        SalesSelectionBookletRepository::new(self, Self::SALES_SELECTION_BOOKLETS)
    }

    fn sales_selection_display_items(&self) -> SalesSelectionDisplayItemRepository<'_> {
        SalesSelectionDisplayItemRepository::new(self, Self::SALES_SELECTION_DISPLAY_ITEMS)
    }

    fn sales_selection_pool_members(&self) -> SalesSelectionPoolMemberRepository<'_> {
        SalesSelectionPoolMemberRepository::new(self, Self::SALES_SELECTION_POOL_MEMBERS)
    }

    fn sales_selection_prepare_tasks(&self) -> SalesSelectionPrepareTaskRepository<'_> {
        SalesSelectionPrepareTaskRepository::new(self, Self::SALES_SELECTION_PREPARE_TASKS)
    }

    fn sales_selection_sessions(&self) -> SalesSelectionSessionRepository<'_> {
        SalesSelectionSessionRepository::new(self, Self::SALES_SELECTION_SESSIONS)
    }

    fn sales_selection_proposals(&self) -> SalesSelectionProposalRepository<'_> {
        SalesSelectionProposalRepository::new(self, Self::SALES_SELECTION_PROPOSALS)
    }

    fn sales_selection_proposal_display_lines(&self) -> SalesSelectionProposalDisplayLineRepository<'_> {
        SalesSelectionProposalDisplayLineRepository::new(self, Self::SALES_SELECTION_PROPOSAL_DISPLAY_LINES)
    }

    fn sales_selection_proposal_sku_lines(&self) -> SalesSelectionProposalSkuLineRepository<'_> {
        SalesSelectionProposalSkuLineRepository::new(self, Self::SALES_SELECTION_PROPOSAL_SKU_LINES)
    }

    fn sales_selection_idempotency(&self) -> SalesSelectionIdempotencyRepository<'_> {
        SalesSelectionIdempotencyRepository::new(self, Self::SALES_SELECTION_IDEMPOTENCY)
    }

    fn sales_selection_rate(
        &self,
    ) -> crate::repository::owned::sales_selection::SalesSelectionRateRepository<'_> {
        crate::repository::owned::sales_selection::SalesSelectionRateRepository::new(self)
    }
}
