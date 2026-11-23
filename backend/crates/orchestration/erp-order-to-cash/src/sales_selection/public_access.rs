//! 公开密码授权与管理端密码维护的组合入口。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_sales::dto::sales_selection::{
    PublicSelectionAccessView, PublicSelectionPageView, SalesSelectionBookletView,
    SalesSelectionPasswordRequest, SaveSelectionSessionRequest, SubmitSelectionSessionRequest,
    UnlockSelectionRequest,
};
use erp_sales::entity::sales_selection::{
    IdempotencyOperation, SelectionGrant, hash_selection_password, token_hash, verify_selection_password,
};
use erp_sales::repository::SalesSelectionExt;
use erp_sales::service::sales_selection::SalesSelectionService;
use persistence_core::{ReadExecutor, Transactional};
use tokio::sync::Semaphore;

use super::SalesSelectionProcess;
use super::password_audit::update_password;
use super::public_retry::{SelectionAttemptError, retry_public_write};
use crate::{Error, Result};

static PASSWORD_WORK: Semaphore = Semaphore::const_new(8);

impl SalesSelectionProcess {
    /// 读取公开选品页，缺少或失效授权时返回锁定视图。
    ///
    /// `token` 为链接，`access` 为密文授权，`ip` 为来源。返回公开页面。无效链接、限流或读取失败时拒绝。
    pub async fn public_page(
        &self,
        token: &str,
        access: Option<&str>,
        ip: &str,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("read", ip, Some(token), 1200).await?;
        let grant = access.and_then(|value| SelectionGrant::decode(value, &self.crypto).ok());
        if let Some(grant) = &grant {
            self.admit_session("read", &grant.session_id, 120).await?;
        }

        Ok(SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .public_page_by_token(token, grant.as_ref(), Instant::now(), &mut ReadExecutor::default())
            .await?)
    }

    /// 公开选品访客提交访问密码时，先限流，再在事务外校验密码，成功后返回访问授权和页面。
    /// 密码错误、历史选品册未设密码、超限时拒绝。
    pub async fn public_unlock(
        &self,
        token: String,
        req: UnlockSelectionRequest,
        ip: String,
    ) -> Result<PublicSelectionAccessView> {
        self.admit_public("unlock", &ip, Some(&token), 2000).await?;
        let service = SalesSelectionService::new(self.db.clone(), self.services.authorization.clone());
        let scope = service.public_unlock_scope(&token, &mut ReadExecutor::default()).await?;
        self.admit_session("unlock", &scope, 10).await?;

        let hash = service
            .public_password_hash(&token, &mut ReadExecutor::default())
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
            return Err(Error::Forbidden("访问密码有误，请核对后重新输入".into()));
        }

        Ok(service
            .public_unlock(&token, &hash, Instant::now(), &self.crypto, &mut ReadExecutor::default())
            .await?)
    }

    /// 访客暂存选择时，校验链接和密文授权、按来源和会话限流，再在保存事务内重验资格。
    /// 成功返回更新后的页面；授权失效或版本冲突时拒绝。
    pub async fn public_save(
        &self,
        token: String,
        access: String,
        req: SaveSelectionSessionRequest,
        ip: String,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("write", &ip, Some(&token), 6000).await?;
        let grant = self.decode_grant(&access)?;
        self.admit_session("write", &grant.session_id, 30).await?;

        let db = self.db.clone();
        let authorization = self.services.authorization.clone();
        retry_public_write(|| {
            let db = db.clone();
            let authorization = authorization.clone();
            let token = token.clone();
            let grant = grant.clone();
            let req = req.clone();
            async move {
                db.client()
                    .clone()
                    .with_transaction(move |executor| {
                        Box::pin(async move {
                            SalesSelectionService::new(db, authorization)
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

    /// 访客最终确认方案时，校验链接和授权并限流，在同一事务提交选择和销售方案，返回回执。
    /// 授权失效或版本冲突时不完成提交。
    pub async fn public_submit(
        &self,
        token: String,
        access: String,
        req: SubmitSelectionSessionRequest,
        ip: String,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("submit", &ip, Some(&token), 2000).await?;
        let grant = self.decode_grant(&access)?;
        self.admit_session("submit", &grant.session_id, 10).await?;

        let db = self.db.clone();
        let authorization = self.services.authorization.clone();
        retry_public_write(|| {
            let db = db.clone();
            let authorization = authorization.clone();
            let token = token.clone();
            let grant = grant.clone();
            let req = req.clone();
            async move {
                db.client()
                    .clone()
                    .with_transaction(move |executor| {
                        Box::pin(async move {
                            SalesSelectionService::new(db, authorization)
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

    /// 校验授权后返回图片键。
    ///
    /// `token` 为链接，`asset_id` 为图片，`access` 为授权，`ip` 为来源。返回本册快照图片键。授权失效、图片越权或限流时拒绝。
    pub async fn public_image_key(
        &self,
        token: &str,
        asset_id: &str,
        access: &str,
        ip: &str,
    ) -> Result<String> {
        self.admit_public("image", ip, Some(token), 6000).await?;
        let grant = self.decode_grant(access)?;
        self.admit_session("image", &grant.session_id, 600).await?;
        Ok(SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .public_image_key(token, asset_id, &grant, Instant::now(), &mut ReadExecutor::default())
            .await?)
    }

    /// 在同一授权事务内修改册访问密码。
    ///
    /// `id` 为册，`req` 为密码命令，`actor` 为管理操作人。返回新册详情，旧公开凭证失效。权限、范围、版本或密码非法时拒绝。
    pub async fn set_access_password(
        &self,
        id: String,
        req: SalesSelectionPasswordRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        let access = self.services.selections.clone();
        let fingerprint =
            self.crypto.request_fingerprint(IdempotencyOperation::SetAccessPassword, &(&id, &req))?;
        let hash = password_hash(req.access_password.clone()).await?;
        update_password(self.db.clone(), access, id, req, hash, fingerprint, actor).await
    }

    async fn admit_session(&self, kind: &str, session_id: &str, limit: i64) -> Result<()> {
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
