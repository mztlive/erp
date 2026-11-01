//! 公开密码授权与管理端个人资料读取的组合入口。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_sales::dto::sales_selection::{
    PublicSelectionAccessView, PublicSelectionPageView, SalesSelectionBookletView,
    SalesSelectionPasswordRequest, SaveSelectionSessionRequest, SelectionDetailView, SelectionVoucherView,
    SubmitSelectionSessionRequest, UnlockSelectionRequest,
};
use erp_sales::entity::sales_selection::{
    IdempotencyOperation, SelectionGrant, hash_selection_password, token_hash, verify_selection_password,
};
use erp_sales::repository::SalesSelectionExt;
use erp_sales::service::sales_selection::SalesSelectionService;
use persistence_core::{NoTransaction, Transactional};
use tokio::sync::Semaphore;

use super::SalesSelectionProcess;
use super::password_audit::update_password;
use super::public_retry::{SelectionAttemptError, retry_public_write};
use crate::{Error, Result};

static PASSWORD_WORK: Semaphore = Semaphore::const_new(8);

impl SalesSelectionProcess {
    /// 读取个人公开页，缺少或失效授权时返回锁定视图。
    ///
    /// # 参数
    /// `token` 为链接，`access` 为密文授权，`ip` 为来源。
    ///
    /// # 返回
    /// 返回隔离后的个人页面。
    ///
    /// # 错误
    /// 无效链接、限流或读取失败时拒绝。
    pub async fn public_page(
        &self,
        token: &str,
        access: Option<&str>,
        ip: &str,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("read", ip, Some(token), 1200).await?;
        let grant = access.and_then(|value| SelectionGrant::decode(value, &self.crypto).ok());
        if let Some(grant) = &grant {
            self.admit_participant("read", &grant.session_id, 120).await?;
        }
        Ok(SalesSelectionService::new(self.db.clone())
            .public_page_by_token(token, grant.as_ref(), Instant::now(), &mut NoTransaction)
            .await?)
    }

    /// 使用访问密码与个人提货码解锁，密码验证不占用事务。
    ///
    /// # 参数
    /// `token` 为链接，`req` 为密码及提货码，`ip` 为来源。
    ///
    /// # 返回
    /// 返回个人授权与页面。
    ///
    /// # 错误
    /// 错误密码、错误提货码、历史未设置密码或限流时拒绝。
    pub async fn public_unlock(
        &self,
        token: String,
        req: UnlockSelectionRequest,
        ip: String,
    ) -> Result<PublicSelectionAccessView> {
        self.admit_public("unlock", &ip, Some(&token), 2000).await?;
        let service = SalesSelectionService::new(self.db.clone());
        let scope =
            service.public_unlock_scope(&token, req.voucher_code.as_deref(), &mut NoTransaction).await?;
        self.admit_participant("unlock", &scope, 10).await?;
        let hash = service
            .public_password_hash(&token, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Forbidden("选品册尚未设置访问密码，请联系业务员".into()))?;
        let verify_hash = hash.clone();
        let permit = PASSWORD_WORK.acquire().await.map_err(|_| Error::Internal("密码验证暂不可用".into()))?;
        let accepted = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verify_selection_password(&req.password, &verify_hash)
        })
        .await
        .map_err(|_| Error::Internal("密码验证失败".into()))?;
        if !accepted {
            return Err(Error::Forbidden("密码或提货码不正确".into()));
        }
        Ok(service
            .public_unlock(
                &token,
                &hash,
                req.voucher_code.as_deref(),
                Instant::now(),
                &self.crypto,
                &mut NoTransaction,
            )
            .await?)
    }

    /// 在事务内重验个人授权并保存选择。
    ///
    /// # 参数
    /// `token` 为链接，`access` 为授权，`req` 为选择，`ip` 为来源。
    ///
    /// # 返回
    /// 返回当前个人页面。
    ///
    /// # 错误
    /// 授权失效、额度或版本冲突时拒绝。
    pub async fn public_save(
        &self,
        token: String,
        access: String,
        req: SaveSelectionSessionRequest,
        ip: String,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("write", &ip, Some(&token), 6000).await?;
        let grant = self.decode_grant(&access)?;
        self.admit_participant("write", &grant.session_id, 30).await?;
        let db = self.db.clone();
        retry_public_write(|| {
            let db = db.clone();
            let token = token.clone();
            let grant = grant.clone();
            let req = req.clone();
            async move {
                db.client()
                    .clone()
                    .with_transaction(move |executor| {
                        Box::pin(async move {
                            SalesSelectionService::new(db)
                                .public_save(&token, &grant, req, Instant::now(), executor)
                                .await
                                .map_err(Error::from)
                                .map_err(SelectionAttemptError::AbortedBody)
                        })
                    })
                    .await
            }
        })
        .await
    }

    /// 原子提交个人选择、地址与个人方案。
    ///
    /// # 参数
    /// `token` 为链接，`access` 为授权，`req` 为最终确认，`ip` 为来源。
    ///
    /// # 返回
    /// 返回个人回执。
    ///
    /// # 错误
    /// 授权失效、额度、地址或版本冲突时拒绝。
    pub async fn public_submit(
        &self,
        token: String,
        access: String,
        req: SubmitSelectionSessionRequest,
        ip: String,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("submit", &ip, Some(&token), 2000).await?;
        let grant = self.decode_grant(&access)?;
        self.admit_participant("submit", &grant.session_id, 10).await?;
        let db = self.db.clone();
        retry_public_write(|| {
            let db = db.clone();
            let token = token.clone();
            let grant = grant.clone();
            let req = req.clone();
            async move {
                db.client()
                    .clone()
                    .with_transaction(move |executor| {
                        Box::pin(async move {
                            SalesSelectionService::new(db)
                                .public_submit(&token, &grant, req, Instant::now(), executor)
                                .await
                                .map_err(Error::from)
                                .map_err(SelectionAttemptError::AbortedBody)
                        })
                    })
                    .await
            }
        })
        .await
    }

    /// 校验个人授权后返回图片键。
    ///
    /// # 参数
    /// `token` 为链接，`asset_id` 为图片，`access` 为授权，`ip` 为来源。
    ///
    /// # 返回
    /// 返回本册快照图片键。
    ///
    /// # 错误
    /// 授权失效、图片越权或限流时拒绝。
    pub async fn public_image_key(
        &self,
        token: &str,
        asset_id: &str,
        access: &str,
        ip: &str,
    ) -> Result<String> {
        self.admit_public("image", ip, Some(token), 6000).await?;
        let grant = self.decode_grant(access)?;
        self.admit_participant("image", &grant.session_id, 600).await?;
        Ok(SalesSelectionService::new(self.db.clone())
            .public_image_key(token, asset_id, &grant, Instant::now(), &mut NoTransaction)
            .await?)
    }

    /// 在同一授权事务内修改册访问密码。
    ///
    /// # 参数
    /// `id` 为册，`req` 为密码命令，`actor` 为管理操作人。
    ///
    /// # 返回
    /// 返回新册详情，旧公开凭证失效。
    ///
    /// # 错误
    /// 权限、范围、版本或密码非法时拒绝。
    pub async fn set_access_password(
        &self,
        id: String,
        req: SalesSelectionPasswordRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        let access = self.selection_access()?;
        let fingerprint =
            self.crypto.request_fingerprint(IdempotencyOperation::SetAccessPassword, &(&id, &req))?;
        let hash = password_hash(req.access_password.clone()).await?;
        update_password(self.db.clone(), access, id, req, hash, fingerprint, actor).await
    }

    /// 按复制链接范围读取个人提货码。
    ///
    /// # 参数
    /// `id` 为册，`actor` 为管理操作人。
    ///
    /// # 返回
    /// 返回可发放提货码及提交状态。
    ///
    /// # 错误
    /// 权限或范围不足时拒绝。
    pub async fn vouchers(&self, id: &str, actor: &AuditActor) -> Result<Vec<SelectionVoucherView>> {
        let mut executor = NoTransaction;
        self.selection_access()?.require_booklet(actor, "copy_link", id, &mut executor).await?;
        Ok(SalesSelectionService::new(self.db.clone()).vouchers(id, &self.crypto, &mut executor).await?)
    }

    /// 按册详情范围读取全部个人选择与地址供导出。
    ///
    /// # 参数
    /// `id` 为册，`actor` 为管理操作人。
    ///
    /// # 返回
    /// 返回全部提交行与收件资料。
    ///
    /// # 错误
    /// 权限或范围不足时拒绝。
    pub async fn selection_details(&self, id: &str, actor: &AuditActor) -> Result<Vec<SelectionDetailView>> {
        let mut executor = NoTransaction;
        self.selection_access()?.require_booklet(actor, "get", id, &mut executor).await?;
        Ok(SalesSelectionService::new(self.db.clone())
            .selection_details(id, &self.crypto, &mut executor)
            .await?)
    }

    async fn admit_participant(&self, kind: &str, session_id: &str, limit: i64) -> Result<()> {
        let key = format!("participant:{kind}:{}", token_hash(session_id));
        let window = Instant::now().unix_secs() / 60;
        if !self.db.sales_selection_rate().admit(&key, limit, window).await? {
            return Err(Error::BusinessLogicError("请求过于频繁，请稍后重试".into()));
        }
        Ok(())
    }

    fn decode_grant(&self, encoded: &str) -> Result<SelectionGrant> {
        SelectionGrant::decode(encoded, &self.crypto)
            .map_err(|_| Error::Forbidden("请先输入选品访问密码".into()))
    }
}

pub(super) async fn password_hash(password: String) -> Result<String> {
    let permit = PASSWORD_WORK.acquire().await.map_err(|_| Error::Internal("密码设置暂不可用".into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        hash_selection_password(&password)
    })
    .await
    .map_err(|_| Error::Internal("密码设置失败".into()))?
    .map_err(|_| Error::ValidationError("访问密码须为8至64个字符".into()))
}
