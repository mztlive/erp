//! 册内客户售价、套餐合计、册版本和幂等收据使用调用方同一事务。

use erp_core::ids::SalesSelectionBookletId;
use persistence_core::Executor;

use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{SalesSelectionBookletView, UpdateSelectionPricesRequest};
use crate::entity::sales_selection::{
    IdempotencyOperation, SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionPoolMember,
    normalize_idempotency_key, request_hash,
};
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::SalesSelectionDomainRepository;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 调整已授权册的当前批次客户售价。
    /// 调用方须在同一执行器核验册维护资格；actor 只用于领域留痕和幂等作用域。
    ///
    /// # 参数
    /// `id` 为册，`req` 为版本、批次与改价方式，`actor` 为操作人，`executor` 为调用方事务。
    ///
    /// # 返回
    /// 返回更新详情及是否首次执行；重放不再次加价。
    ///
    /// # 错误
    /// 状态、版本、批次、金额、幂等冲突或任一步写入失败时拒绝。
    pub async fn update_prices(
        &self,
        id: &str,
        req: UpdateSelectionPricesRequest,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SalesSelectionBookletView, bool)> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = request_hash(
            &serde_json::to_string(&(id, &req)).map_err(|error| Error::Internal(error.to_string()))?,
        );
        let operation = IdempotencyOperation::UpdatePrices;
        if let Some(view) = self.replay_idempotency(operation, actor, &key, &hash, executor).await? {
            return Ok((view, false));
        }
        let mut book = self.load_booklet(id, executor).await?;
        book.ensure_version(req.expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        if book.current_batch_id.as_deref() != Some(req.batch_id.as_str()) {
            return Err(Error::selection_conflict("准备批次已变化，请刷新后改价"));
        }
        let domain = SalesSelectionDomainRepository::new(&self.db);
        let mut pool = domain.list_pool_members(id, &req.batch_id, executor).await?;
        let mut items = domain.list_effective_items(id, &req.batch_id, executor).await?;
        req.change
            .apply(book.status, &mut pool, &mut items)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        book.record_display_edit(actor)?;
        self.persist_prices(&mut book, &mut pool, &mut items, executor).await?;
        let view = self.detail_view(&book, None, executor).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation,
                scope_id: actor,
                key: &key,
                hash: &hash,
                result: &view,
                token_version: None,
                booklet_id: Some(SalesSelectionBookletId::new(id)),
            },
            executor,
        )
        .await?;
        Ok((view, true))
    }

    async fn persist_prices(
        &self,
        book: &mut SalesSelectionBooklet,
        pool: &mut [SalesSelectionPoolMember],
        items: &mut [SalesSelectionDisplayItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.sales_selection_booklets().update(book, executor).await?;
        for member in pool {
            self.db.sales_selection_pool_members().update(member, executor).await?;
        }
        for item in items {
            self.db.sales_selection_display_items().update(item, executor).await?;
        }
        Ok(())
    }
}
