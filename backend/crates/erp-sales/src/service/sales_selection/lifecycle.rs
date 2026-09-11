//! 链接生命周期命令：版本竞争与幂等结果共同提交。
use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{SalesSelectionBookletView, SalesSelectionCommandRequest};
use crate::entity::sales_selection::{
    normalize_idempotency_key, request_hash, IdempotencyOperation, LinkTokenCrypto,
};
use crate::{repository::SalesSelectionExt, Error, Result};
use erp_core::{common::time::Instant, ids::SalesSelectionBookletId};
use persistence_core::{Executor, Transactional};

impl SalesSelectionService {
    /// 更换、关闭、撤销和作废的共同原子入口。
    /// # 错误
    /// 异载荷重试、旧版本、非法状态或任一步写入失败时整次拒绝。
    pub(super) async fn lifecycle_command(
        &self,
        id: &str,
        req: SalesSelectionCommandRequest,
        actor: &str,
        operation: IdempotencyOperation,
        crypto: Option<LinkTokenCrypto>,
    ) -> Result<SalesSelectionBookletView> {
        let service = Self::new(self.db.clone());
        let (id, actor) = (id.to_string(), actor.to_string());
        self.db
            .client()
            .with_transaction(move |tx| {
                Box::pin(async move {
                    service
                        .lifecycle_in(&id, req, &actor, operation, crypto, tx)
                        .await
                })
            })
            .await
    }

    /// 先识别原请求，再竞争册版本；密文与幂等事实不得分开提交。
    async fn lifecycle_in(
        &self,
        id: &str,
        req: SalesSelectionCommandRequest,
        actor: &str,
        operation: IdempotencyOperation,
        crypto: Option<LinkTokenCrypto>,
        tx: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash =
            request_hash(&serde_json::to_string(&(id, &req)).map_err(|e| Error::Internal(e.to_string()))?);
        if let Some(view) = self.replay_idempotency(operation, actor, &key, &hash, tx).await? {
            return Ok(view);
        }
        let mut book = self.load_booklet(id, tx).await?;
        book.ensure_version(req.expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        match operation {
            IdempotencyOperation::RotateLink => {
                book.ensure_public_write(book.link_token_hash.as_deref().unwrap_or(""), Instant::now())?;
                let (_, hash, cipher) = crypto
                    .ok_or_else(|| Error::Internal("缺少链接密钥".into()))?
                    .issue()
                    .map_err(|e| Error::Internal(e.to_string()))?;
                book.rotate_link(hash, cipher, actor)?;
            }
            IdempotencyOperation::Close => book.close(Instant::now(), actor)?,
            IdempotencyOperation::RevokeAccess => book.revoke_access(actor)?,
            IdempotencyOperation::Void => book.void(Instant::now(), actor)?,
            _ => return Err(Error::Internal("非法链接命令".into())),
        }
        self.db.sales_selection_booklets().update(&mut book, tx).await?;
        let view = self.detail_view(&book, None, tx).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation,
                scope_id: actor,
                key: &key,
                hash: &hash,
                result: &view,
                token_version: Some(book.link_token_version),
                booklet_id: Some(SalesSelectionBookletId::new(id)),
            },
            tx,
        )
        .await?;
        Ok(view)
    }
}
