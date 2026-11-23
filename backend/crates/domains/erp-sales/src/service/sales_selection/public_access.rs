//! 公开密码解锁与管理端密码维护。

use erp_core::common::time::Instant;
use erp_core::ids::SalesSelectionBookletId;
use persistence_core::Executor;

use super::mapper::public_kind;
use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    PublicSelectionAccessView, PublicSelectionPageKind, SalesSelectionBookletView,
    SalesSelectionPasswordRequest,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, LinkTokenCrypto, SelectionGrant, SelectionRequestFingerprint,
    normalize_idempotency_key,
};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 读取慢哈希验证所需的当前密码事实。
    /// 返回当前哈希；历史未配置密码返回空。
    /// 无效链接或读取失败时拒绝。
    ///
    /// 输入约定：`token` 为公开链接，`executor` 为读取边界。
    pub async fn public_password_hash(
        &self,
        token: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(self.load_by_token(token, executor).await?.access_password_hash)
    }

    /// 已通过密码校验后，再重验当前密码并绑定会话。
    /// 返回授权凭证与公开选品页。
    /// 密码变化、选品已结束或会话缺失时拒绝。
    ///
    /// 输入约定：`token` 为链接，`verified_hash` 为刚验证的哈希，`now` 为当前时间，`crypto` 为编解码器，`executor` 为读取边界。
    pub async fn public_unlock(
        &self,
        token: &str,
        verified_hash: &str,
        now: Instant,
        crypto: &LinkTokenCrypto,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionAccessView> {
        let book = self.load_by_token(token, executor).await?;
        if book.access_password_hash.as_deref() != Some(verified_hash) {
            return Err(Error::Forbidden("访问密码已更新，请重新输入".into()));
        }
        if public_kind(&book, now) == PublicSelectionPageKind::Ended {
            return Err(Error::Forbidden("选品已结束".into()));
        }

        let session = self.unlock_session(&book, executor).await?;
        let access_token = SelectionGrant::issue(&book, &session, now, crypto)?;
        let page = self.session_page(&book, &session, now, executor).await?;
        Ok(PublicSelectionAccessView { access_token, page })
    }

    /// 在密码慢验证前确定限流作用域；不返回任何业务视图。
    /// 结果：返回当前选品册的会话身份。
    /// 链接或会话无效时拒绝。
    ///
    /// 输入约定：`token` 为链接，`executor` 为读取边界。
    pub async fn public_unlock_scope(&self, token: &str, executor: &mut dyn Executor) -> Result<String> {
        let book = self.load_by_token(token, executor).await?;
        Ok(self.unlock_session(&book, executor).await?.base.id)
    }

    async fn unlock_session(
        &self,
        book: &crate::entity::sales_selection::SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<crate::entity::sales_selection::SalesSelectionSession> {
        self.db
            .sales_selection_sessions()
            .find_by_booklet(&book.base.id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("访问密码有误，请核对后重新输入".into()))
    }

    /// 在调用方授权事务内更新访问密码并保存幂等结果。
    /// 调用方须在同一执行器核验册维护资格；actor 只用于领域留痕和幂等作用域。
    /// 返回详情和是否首次执行标记；旧公开授权立即失效。
    /// 旧版本、幂等异载荷或非法状态时原子拒绝。
    ///
    /// 输入约定：`id` 为册，`req` 为密码命令，`password_hash` 为慢哈希，`fingerprint` 为完整原请求的带密钥指纹，`actor` 为操作人，`executor` 必须为事务。
    pub async fn set_access_password(
        &self,
        id: &str,
        req: SalesSelectionPasswordRequest,
        password_hash: String,
        fingerprint: SelectionRequestFingerprint,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SalesSelectionBookletView, bool)> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = fingerprint.as_str();
        if let Some(view) = self
            .replay_idempotency(IdempotencyOperation::SetAccessPassword, actor, &key, hash, executor)
            .await?
        {
            return Ok((view, false));
        }

        let mut book = self.load_booklet(id, executor).await?;
        book.ensure_version(req.expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        book.set_access_password_hash(password_hash, actor)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        self.db.sales_selection_booklets().update(&mut book, executor).await?;

        let view = self.detail_view(&book, None, executor).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation: IdempotencyOperation::SetAccessPassword,
                scope_id: actor,
                key: &key,
                hash,
                result: &view,
                token_version: Some(book.link_token_version),
                booklet_id: Some(SalesSelectionBookletId::new(id)),
            },
            executor,
        )
        .await?;
        Ok((view, true))
    }
}
