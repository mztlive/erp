//! 销售选品跨域流程：客户、商品池、图片与事务。

use std::sync::Arc;

use erp_permissions::sales_selection;

use crate::services::SalesServices;
mod adapters;
mod copy;
mod password_audit;
mod pricing;
mod public_access;
mod public_retry;
mod read;
mod scope_checks;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::service::business_ownership::required_business_org;
use erp_sales::dto::sales_selection::{
    CopyLinkView, CreateSalesSelectionBookletRequest, DeleteDisplayItemRequest, PrepareSalesSelectionRequest,
    PublishSalesSelectionRequest, SalesSelectionBookletView, SalesSelectionCommandRequest,
    SalesSelectionSessionView, SelectionBookletCreator,
};
use erp_sales::entity::sales_selection::{IdempotencyOperation, LinkTokenCrypto};
use erp_sales::ports::sales_selection::SelectionImagePort;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_selection::SalesSelectionDomainRepository;
use erp_sales::service::sales_selection::SalesSelectionService;
use mongodb::Database;
use persistence_core::{ReadExecutor, Transactional};
use storage::S3Storage;

use self::adapters::{CatalogAdapter, CustomerAdapter, ImageAdapter};
use self::scope_checks::{
    check_create_scope, copy_link_checked, image_key_checked, publish_checked, session_checked,
    start_prepare_checked,
};
use crate::{Error, Result};

/// 销售选品组合根。
pub struct SalesSelectionProcess {
    services: Arc<SalesServices>,
    db: Database,
    storage: S3Storage,
    crypto: LinkTokenCrypto,
}

impl SalesSelectionProcess {
    /// 创建流程。
    ///
    /// # 参数
    /// * `db` - 数据库
    /// * `storage` - 对象存储
    /// * `secret` - 应用密钥，用于链接加密
    /// * `services` - 应用已装配的客户、目录、文件与范围能力。
    ///
    /// # 返回
    /// 返回流程。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, storage: S3Storage, secret: &[u8], services: Arc<SalesServices>) -> Self {
        Self { db, storage, services, crypto: LinkTokenCrypto::from_secret(secret) }
    }

    /// 创建选品册并排队首次准备。
    ///
    /// # 参数
    /// * `req` - 请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回准备中详情。
    ///
    /// # 错误
    /// 校验、客户不可用或无法排队准备。
    pub async fn create(
        &self,
        req: CreateSalesSelectionBookletRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        let fingerprint = self.crypto.request_fingerprint(IdempotencyOperation::Create, &req)?;
        let password_hash = public_access::password_hash(req.access_password.clone()).await?;

        let db = self.db.clone();
        let customer = CustomerAdapter { service: self.services.customers.clone() };
        let selection = self.services.selections.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                let req = req.clone();
                let actor = actor.clone();
                let db = db.clone();
                let selection = selection.clone();
                let password_hash = password_hash.clone();
                let fingerprint = fingerprint.clone();
                Box::pin(async move {
                    let access = selection;
                    let creator = SelectionBookletCreator {
                        user_id: actor.id().to_owned(),
                        business_org_unit_id: required_business_org(&db, actor.id(), executor).await?,
                    };
                    check_create_scope(
                        &access,
                        &actor,
                        &req.customer_id,
                        &creator.user_id,
                        &creator.business_org_unit_id,
                        executor,
                    )
                    .await?;

                    SalesSelectionService::new(db, access.authorization())
                        .create(req, &creator, password_hash, fingerprint, &customer, executor)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }

    /// 启动准备。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回准备中详情。
    ///
    /// # 错误
    /// 状态、版本或无权操作。
    pub async fn start_prepare(
        &self,
        id: String,
        req: PrepareSalesSelectionRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        start_prepare_checked(
            &self.db,
            &self.storage,
            &self.services,
            self.services.selections.clone(),
            id,
            req,
            actor,
        )
        .await
    }

    /// 删除陈列。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `item_id` - 陈列
    /// * `req` - 版本
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 状态、版本或无权操作。
    pub async fn delete_display_item(
        &self,
        booklet_id: String,
        item_id: String,
        req: DeleteDisplayItemRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        SalesSelectionService::new(self.db.clone(), self.services.selections.authorization())
            .delete_display_item(&booklet_id, &item_id, req.expected_version, &actor)
            .await
            .map_err(Error::from)
    }

    /// 发布。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回含链接的详情。
    ///
    /// # 错误
    /// 复核失败、状态不允许或无权操作。
    pub async fn publish(
        &self,
        id: String,
        req: PublishSalesSelectionRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        publish_checked(
            &self.db,
            &self.services,
            &self.crypto,
            self.services.selections.clone(),
            id,
            req,
            actor,
        )
        .await
    }

    /// 复制链接。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回含路径的详情。
    ///
    /// # 错误
    /// 不存在或无权操作。
    pub async fn copy_link(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionBookletView> {
        copy_link_checked(&self.db, &self.crypto, self.services.selections.clone(), id, actor).await
    }
    /// 复制当前链接的相对地址。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回 `/s/{token}`，不记录完整令牌。
    ///
    /// # 错误
    /// 无链接或无权操作。
    pub async fn copy_link_url(&self, id: &str, actor: &AuditActor) -> Result<CopyLinkView> {
        let access = self.services.selections.clone();
        let db = self.db.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        let crypto = self.crypto.clone();
        let authorization = self.services.authorization.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    read::checked(
                        &access,
                        &actor,
                        &sales_selection::BOOKLET_COPY_LINK,
                        executor,
                        move |scope, executor| {
                            Box::pin(async move {
                                let book = SalesSelectionDomainRepository::new(&db)
                                    .find_authorized_booklet(&id, scope, executor)
                                    .await?
                                    .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;

                                let token = SalesSelectionService::new(db, authorization)
                                    .copy_link(&book.base.id, &crypto, executor)
                                    .await?;
                                let path = format!("/s/{token}");
                                Ok(CopyLinkView { public_url: path.clone(), public_path: path })
                            })
                        },
                    )
                    .await
                })
            })
            .await
    }

    /// 内部会话快照。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回当前会话。
    ///
    /// # 错误
    /// 无会话或无权查看。
    pub async fn admin_session(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionSessionView> {
        session_checked(&self.db, self.services.selections.clone(), id, actor).await
    }

    /// 更换链接。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回新链接。
    ///
    /// # 错误
    /// 状态、版本或无权操作。
    pub async fn rotate_link(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        SalesSelectionService::new(self.db.clone(), self.services.selections.authorization())
            .rotate_link(&id, req, &actor, &self.crypto)
            .await
            .map_err(Error::from)
    }

    /// 关闭。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回已关闭详情。
    ///
    /// # 错误
    /// 状态不允许或无权操作。
    pub async fn close(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .close_booklet(&id, req, &actor)
            .await
            .map_err(Error::from)
    }

    /// 撤销访问。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 状态不允许或无权操作。
    pub async fn revoke(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .revoke_access(&id, req, &actor)
            .await
            .map_err(Error::from)
    }

    /// 作废。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回已作废详情。
    ///
    /// # 错误
    /// 状态不允许或无权操作。
    pub async fn void(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .void_booklet(&id, req, &actor)
            .await
            .map_err(Error::from)
    }

    /// 运行到期准备任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 处理数量。
    ///
    /// # 错误
    /// 仓储失败。
    pub async fn run_due_prepare_tasks(&self) -> Result<u32> {
        let catalog = CatalogAdapter { query: self.services.catalog.clone() };
        let images =
            ImageAdapter { files: self.services.files.clone(), storage: Arc::new(self.storage.clone()) };
        Ok(SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .run_due_prepare_tasks(&catalog, &images)
            .await?)
    }

    /// 读取已通过管理端客户权限校验的册图片。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `asset_id` - 资产
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回快照字节与内容类型。
    ///
    /// # 错误
    /// 资产不属于本册或对象不存在时拒绝。
    pub async fn admin_image(
        &self,
        id: &str,
        asset_id: &str,
        actor: &AuditActor,
    ) -> Result<(Vec<u8>, String)> {
        let key = image_key_checked(&self.db, self.services.selections.clone(), id, asset_id, actor).await?;
        self.read_image_bytes(&key).await
    }

    /// 读取图片字节。
    ///
    /// # 参数
    /// * `storage_object_key` - 对象键
    ///
    /// # 返回
    /// 返回内容。
    ///
    /// # 错误
    /// 对象不存在。
    pub async fn read_image_bytes(&self, storage_object_key: &str) -> Result<(Vec<u8>, String)> {
        ImageAdapter { files: self.services.files.clone(), storage: Arc::new(self.storage.clone()) }
            .load_bytes(storage_object_key)
            .await
            .map_err(Error::from)
    }

    /// 公开限流。非法令牌只按 IP 限流。
    ///
    /// # 参数
    /// * `kind` - 读/写/提交/图片
    /// * `ip` - 来源 IP
    /// * `token` - 令牌；无效时忽略
    /// * `limit` - 每分钟上限
    ///
    /// # 返回
    /// 通过时 `Ok`。
    ///
    /// # 错误
    /// 超限。
    async fn admit_public(&self, kind: &str, ip: &str, token: Option<&str>, limit: i64) -> Result<()> {
        use erp_sales::entity::sales_selection::token_hash;
        use erp_sales::repository::SalesSelectionExt;
        let window = Instant::now().unix_secs() / 60;
        let ip_key = format!("ip:{kind}:{ip}");
        let ok = self.db.sales_selection_rate().admit(&ip_key, limit, window).await?;
        if !ok {
            return Err(Error::BusinessLogicError("请求过于频繁，请稍后重试".into()));
        }

        let Some(token) = token else {
            return Ok(());
        };
        if token.len() != 64 || hex::decode(token).is_err() {
            return Ok(());
        }
        let hash = token_hash(token);
        if self
            .db
            .sales_selection_booklets()
            .find_by_token_hash(&hash, &mut ReadExecutor::default())
            .await?
            .is_none()
        {
            return Ok(());
        }

        let token_key = format!("token:{kind}:{hash}");
        let ok = self.db.sales_selection_rate().admit(&token_key, limit, window).await?;
        if !ok {
            return Err(Error::BusinessLogicError("请求过于频繁，请稍后重试".into()));
        }
        Ok(())
    }
}
