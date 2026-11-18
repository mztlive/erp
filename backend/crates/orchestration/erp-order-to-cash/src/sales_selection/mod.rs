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
    /// 应用装配选品流程时注入客户、目录、文件和授权范围能力，以应用密钥派生链接加密器，并接入对象存储。
    pub fn new(db: Database, storage: S3Storage, secret: &[u8], services: Arc<SalesServices>) -> Self {
        Self { db, storage, services, crypto: LinkTokenCrypto::from_secret(secret) }
    }

    /// 销售为客户新建选品册时，在事务内核验经办人组织和客户范围，创建册并排队首次准备，返回准备中详情。
    /// 请求校验、客户不可用或排队失败时返回错误。
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

    /// 选品册开始准备时，调用目录和图片能力处理当前册的准备命令，返回准备中详情；权限、状态或版本不符时拒绝。
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

    /// 维护选品陈列时，按所见版本删除指定册的指定陈列并返回更新详情；权限、状态或版本不符时拒绝。
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

    /// 选品册对外发布前按当前权限和目录复核册内容，成功后返回带访问链接的详情。
    /// 无权操作、复核失败或状态不允许时不发布。
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

    /// 内部人员分享当前选品册时，在授权快照读取带当前链接路径的详情；册或链接不可用、无权查看时拒绝。
    pub async fn copy_link(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionBookletView> {
        copy_link_checked(&self.db, &self.crypto, self.services.selections.clone(), id, actor).await
    }
    /// 仅需分发相对地址时，在同一快照检查复制链接权限和册归属，返回 `/s/{token}`，不记录完整令牌。
    /// 没有可复制的链接或无权访问时拒绝。
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

    /// 内部查看选品进度时，在选品册详情授权快照读取当前会话；无会话或当前账号无权查看时拒绝。
    pub async fn admin_session(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionSessionView> {
        session_checked(&self.db, self.services.selections.clone(), id, actor).await
    }

    /// 选品册需要换发访问链接时，执行带当前版本的更换命令，成功返回新链接；权限、状态或版本不符时拒绝。
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

    /// 经办人结束选品办理时，将册关闭并返回关闭后的详情；状态不允许或当前账号无权操作时拒绝。
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

    /// 需要撤回选品册的公开访问时，执行访问撤销并返回更新后的册详情；状态不允许或无权操作时拒绝。
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

    /// 不再使用选品册时，由经办人作废并返回已作废详情；状态不允许或无权限时拒绝。
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

    /// 后台轮询到期选品准备任务时，用已装配的目录与图片能力执行一轮准备，返回本轮处理数量；仓储失败向轮询端返回错误。
    pub async fn run_due_prepare_tasks(&self) -> Result<u32> {
        let catalog = CatalogAdapter { query: self.services.catalog.clone() };
        let images =
            ImageAdapter { files: self.services.files.clone(), storage: Arc::new(self.storage.clone()) };
        Ok(SalesSelectionService::new(self.db.clone(), self.services.authorization.clone())
            .run_due_prepare_tasks(&catalog, &images)
            .await?)
    }

    /// 管理端展示选品册图片时，先按当前账号核对本册权限和资产归属，再从对象存储读取快照字节及内容类型。
    /// 资产不属于本册或对应对象不存在时不返回内容。
    pub async fn admin_image(
        &self,
        id: &str,
        asset_id: &str,
        actor: &AuditActor,
    ) -> Result<(Vec<u8>, String)> {
        let key = image_key_checked(&self.db, self.services.selections.clone(), id, asset_id, actor).await?;
        self.read_image_bytes(&key).await
    }

    /// 图片入口取得可读对象键后，由本方法读取快照字节及内容类型；对象不存在时返回错误。
    pub async fn read_image_bytes(&self, storage_object_key: &str) -> Result<(Vec<u8>, String)> {
        ImageAdapter { files: self.services.files.clone(), storage: Arc::new(self.storage.clone()) }
            .load_bytes(storage_object_key)
            .await
            .map_err(Error::from)
    }

    // 公开选品入口按读、写、提交或图片动作执行每分钟限流；有有效令牌时纳入对应限制，无效令牌只按来源 IP 计限。
    // 超过调用方给定上限时拒绝。
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
