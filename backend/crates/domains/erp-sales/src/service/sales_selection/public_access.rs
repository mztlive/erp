//! 公开密码解锁、管理端密码维护与个人明细读取。

use erp_core::common::time::Instant;
use erp_core::ids::SalesSelectionBookletId;
use persistence_core::Executor;

use super::mapper::public_kind;
use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    PublicSelectionAccessView, PublicSelectionPageKind, SalesSelectionBookletView,
    SalesSelectionPasswordRequest, SelectionDetailView, SelectionVoucherView,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, LinkTokenCrypto, SelectionGrant, SelectionRequestFingerprint, SubmitMode,
    normalize_idempotency_key, token_hash,
};
use crate::repository::SalesSelectionExt;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 读取慢哈希验证所需的当前密码事实。
    ///
    /// # 参数
    /// `token` 为公开链接，`executor` 为读取边界。
    ///
    /// # 返回
    /// 返回当前哈希；历史未配置密码返回空。
    ///
    /// # 错误
    /// 无效链接或读取失败时拒绝。
    pub async fn public_password_hash(
        &self,
        token: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Ok(self.load_by_token(token, executor).await?.access_password_hash)
    }

    /// 已通过密码校验后，再重验当前密码并绑定个人会话。
    ///
    /// # 参数
    /// `token` 为链接，`verified_hash` 为刚验证的哈希，`voucher_code` 为提货码，`now` 为当前时间，`crypto` 为编解码器，`executor` 为读取边界。
    ///
    /// # 返回
    /// 返回个人授权凭证与页面。
    ///
    /// # 错误
    /// 密码变化、结束、缺提货码或提货码不属于本册时拒绝。
    pub async fn public_unlock(
        &self,
        token: &str,
        verified_hash: &str,
        voucher_code: Option<&str>,
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
        let session = self.unlock_session(&book, voucher_code, executor).await?;
        let access_token = SelectionGrant::issue(&book, &session, now, crypto)?;
        let page = self.participant_page(&book, &session, now, executor).await?;
        Ok(PublicSelectionAccessView { access_token, page })
    }

    /// 在密码慢验证前确定限流作用域；不返回任何个人视图。
    ///
    /// # 参数
    /// `token` 为链接，`voucher_code` 为个人提货码，`executor` 为读取边界。
    ///
    /// # 返回
    /// 提货券返回个人会话身份，普通册返回共享会话身份。
    ///
    /// # 错误
    /// 链接或提货码无效时拒绝。
    pub async fn public_unlock_scope(
        &self,
        token: &str,
        voucher_code: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let book = self.load_by_token(token, executor).await?;
        Ok(self.unlock_session(&book, voucher_code, executor).await?.base.id)
    }

    async fn unlock_session(
        &self,
        book: &crate::entity::sales_selection::SalesSelectionBooklet,
        voucher_code: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<crate::entity::sales_selection::SalesSelectionSession> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let found = if book.submit_mode == SubmitMode::PickupVoucher {
            let code = voucher_code
                .filter(|value| value.len() <= 128)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::Forbidden("请输入有效提货码".into()))?;
            domain.session_by_voucher_hash(&book.base.id, &token_hash(code), executor).await?
        } else {
            domain.session_by_participant(&book.base.id, "", executor).await?
        };
        found.ok_or_else(|| Error::Forbidden("密码或提货码不正确".into()))
    }

    /// 在调用方授权事务内更新访问密码并保存幂等结果。
    ///
    /// # 参数
    /// `id` 为册，`req` 为密码命令，`password_hash` 为慢哈希，`fingerprint` 为完整原请求的带密钥指纹，`actor` 为操作人，`executor` 必须为事务。
    ///
    /// # 返回
    /// 返回详情和是否首次执行标记；旧公开授权立即失效。
    ///
    /// # 错误
    /// 旧版本、幂等异载荷或非法状态时原子拒绝。
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

    /// 读取当前册所有提货码及提交状态。
    ///
    /// # 参数
    /// `id` 为已授权册，`crypto` 为内部码编解码器，`executor` 为读取边界。
    ///
    /// # 返回
    /// 返回个人提货码；普通册返回空列表。
    ///
    /// # 错误
    /// 密文损坏或读取失败时拒绝。
    pub async fn vouchers(
        &self,
        id: &str,
        crypto: &LinkTokenCrypto,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SelectionVoucherView>> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let sessions = domain.sessions_for_booklet(id, executor).await?;
        sessions
            .into_iter()
            .filter(|session| session.voucher_code_ciphertext.is_some())
            .map(|session| {
                let code = crypto.decrypt(session.voucher_code_ciphertext.as_deref().unwrap_or_default())?;
                Ok(SelectionVoucherView {
                    voucher_code: code,
                    participant_id: session.participant_id,
                    submitted: session.frozen,
                    proposal_id: session.proposal_id.map(|id| id.to_string()),
                })
            })
            .collect()
    }

    /// 读取已授权册的全部个人提交明细和收件信息。
    ///
    /// # 参数
    /// `id` 为已授权册，`crypto` 为内部码编解码器，`executor` 为读取边界。
    ///
    /// # 返回
    /// 返回每人完整 SKU 行及冻结地址，供管理端导出。
    ///
    /// # 错误
    /// 明细读取或密文解码失败时拒绝。
    pub async fn selection_details(
        &self,
        id: &str,
        crypto: &LinkTokenCrypto,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SelectionDetailView>> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let sessions = domain.sessions_for_booklet(id, executor).await?;
        let proposals = domain.proposals_for_booklet(id, executor).await?;
        let mut result = Vec::with_capacity(proposals.len());
        for proposal in proposals {
            let view = self.proposal_view_of(&proposal, executor).await?;
            let voucher_code = sessions
                .iter()
                .find(|session| session.participant_id == proposal.participant_id)
                .and_then(|session| session.voucher_code_ciphertext.as_deref())
                .map(|cipher| crypto.decrypt(cipher))
                .transpose()?;
            result.push(SelectionDetailView {
                participant_id: proposal.participant_id,
                voucher_code,
                proposal_id: proposal.base.id,
                proposal_no: proposal.proposal_no,
                submitted_at: proposal.submitted_at,
                recipient: proposal.recipient,
                total_amount: proposal.total_amount,
                items: view.sku_lines,
            });
        }
        Ok(result)
    }
}
