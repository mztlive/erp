//! 公开密码授权与管理端个人资料读取的组合入口。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::sales_selection;
use erp_sales::dto::sales_selection::{
    PublicSelectionAccessView, PublicSelectionPageView, SalesSelectionBookletView,
    SalesSelectionPasswordRequest, SaveSelectionSessionRequest, SelectionDetailView, SelectionVoucherView,
    SubmitSelectionSessionRequest, UnlockSelectionRequest,
};
use erp_sales::entity::sales_selection::{
    IdempotencyOperation, SelectionGrant, hash_selection_password, token_hash, verify_selection_password,
};
use erp_sales::repository::SalesSelectionExt;
use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
use erp_sales::service::sales_selection::SalesSelectionService;
use persistence_core::{ReadExecutor, Transactional};
use tokio::sync::Semaphore;

use super::SalesSelectionProcess;
use super::password_audit::update_password;
use super::public_retry::{SelectionAttemptError, retry_public_write};
use crate::{Error, Result};

static PASSWORD_WORK: Semaphore = Semaphore::const_new(8);

impl SalesSelectionProcess {
    /// 读取个人公开页，缺少或失效授权时返回锁定视图。
    ///
    /// `token` 为链接，`access` 为密文授权，`ip` 为来源。返回隔离后的个人页面。无效链接、限流或读取失败时拒绝。
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

        Ok(SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .public_page_by_token(token, grant.as_ref(), Instant::now(), &mut ReadExecutor::default())
            .await?)
    }

    /// 公开选品访客提交访问密码和个人提货码时，先限流，再在事务外校验密码，成功后返回个人授权和隔离页面。
    /// 密码或提货码错误、历史选品册未设密码、超限时拒绝。
    pub async fn public_unlock(
        &self,
        token: String,
        req: UnlockSelectionRequest,
        ip: String,
    ) -> Result<PublicSelectionAccessView> {
        self.admit_public("unlock", &ip, Some(&token), 2000).await?;
        let service = SalesSelectionService::new(self.db.clone(), self.services.authorization.clone());
        let scope = service
            .public_unlock_scope(&token, req.voucher_code.as_deref(), &mut ReadExecutor::default())
            .await?;
        self.admit_participant("unlock", &scope, 10).await?;

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
            return Err(Error::Forbidden("访问密码或提货码有误，请核对后重新输入".into()));
        }

        Ok(service
            .public_unlock(
                &token,
                &hash,
                req.voucher_code.as_deref(),
                Instant::now(),
                &self.crypto,
                &mut ReadExecutor::default(),
            )
            .await?)
    }

    /// 访客暂存个人选择时，校验链接和密文授权、按来源和会话限流，再在保存事务内重验个人资格。
    /// 成功返回更新后的个人页面；授权失效、额度不符或版本冲突时拒绝。
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

    /// 访客最终确认个人方案时，校验链接和个人授权并限流，在同一事务提交选择、地址和个人方案，返回个人回执。
    /// 授权失效、额度或地址不符、版本冲突时不完成提交。
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

    /// 校验个人授权后返回图片键。
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
        self.admit_participant("image", &grant.session_id, 600).await?;
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

    /// 按复制链接范围读取个人提货码。
    ///
    /// `id` 为册，`actor` 为管理操作人。返回可发放提货码及提交状态。权限或范围不足时拒绝。
    pub async fn vouchers(&self, id: &str, actor: &AuditActor) -> Result<Vec<SelectionVoucherView>> {
        let db = self.db.clone();
        let access = self.services.selections.clone();
        let crypto = self.crypto.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let scope = access.resolve(&actor, &sales_selection::BOOKLET_COPY_LINK, executor).await?;
                    let book = SalesSelectionDomainRepository::new(&db)
                        .find_authorized_booklet(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;

                    let result = SalesSelectionService::new(db, access.authorization())
                        .vouchers(&book.base.id, &crypto, executor)
                        .await?;
                    scope.validate(executor).await?;
                    Ok(result)
                })
            })
            .await
    }

    /// 按册详情范围读取全部个人选择与地址供导出。
    ///
    /// `id` 为册，`actor` 为管理操作人。返回全部提交行与收件资料。权限或范围不足时拒绝。
    pub async fn selection_details(&self, id: &str, actor: &AuditActor) -> Result<Vec<SelectionDetailView>> {
        let db = self.db.clone();
        let access = self.services.selections.clone();
        let crypto = self.crypto.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let scope = access.resolve(&actor, &sales_selection::BOOKLET_DETAIL, executor).await?;
                    let book = SalesSelectionDomainRepository::new(&db)
                        .find_authorized_booklet(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;

                    let result = SalesSelectionService::new(db, access.authorization())
                        .selection_details(&book.base.id, &crypto, executor)
                        .await?;
                    scope.validate(executor).await?;
                    Ok(result)
                })
            })
            .await
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
