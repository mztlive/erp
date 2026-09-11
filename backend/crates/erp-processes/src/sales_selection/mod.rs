//! 销售选品跨域流程：客户、商品池、图片与事务。

mod adapters;

use crate::{Error, Result};
use erp_core::common::time::Instant;
use erp_sales::dto::sales_selection::{
    CopyLinkView, CreateSalesSelectionBookletRequest, DeleteDisplayItemRequest, PrepareSalesSelectionRequest,
    PublicSelectionPageView, PublishSalesSelectionRequest, SalesSelectionBookletListParams,
    SalesSelectionBookletPage, SalesSelectionBookletView, SalesSelectionCommandRequest,
    SalesSelectionProposalListParams, SalesSelectionProposalPage, SalesSelectionProposalView,
    SalesSelectionSessionView, SaveSelectionSessionRequest, SubmitSelectionSessionRequest,
};
use erp_sales::entity::sales_selection::LinkTokenCrypto;
use erp_sales::ports::sales_selection::SelectionImagePort;
use erp_sales::service::sales_selection::SalesSelectionService;
use mongodb::Database;
use persistence_core::Transactional;
use std::sync::Arc;
use storage::S3Storage;

use self::adapters::{CatalogAdapter, CustomerAdapter, ImageAdapter};

/// 销售选品组合根。
pub struct SalesSelectionProcess {
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
    ///
    /// # 返回
    /// 返回流程。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, storage: S3Storage, secret: &[u8]) -> Self {
        Self {
            db,
            storage,
            crypto: LinkTokenCrypto::from_secret(secret),
        }
    }

    /// 创建选品册。
    ///
    /// # 参数
    /// * `req` - 请求
    /// * `actor_id` - 创建人
    ///
    /// # 返回
    /// 返回草稿。
    ///
    /// # 错误
    /// 校验或客户不可用。
    pub async fn create(
        &self,
        req: CreateSalesSelectionBookletRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        let db = self.db.clone();
        let customer = CustomerAdapter { db: db.clone() };
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                let req = req.clone();
                let actor_id = actor_id.clone();
                let db = db.clone();
                Box::pin(async move {
                    SalesSelectionService::new(db)
                        .create(req, &actor_id, &customer, session)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }

    /// 列表。
    ///
    /// # 参数
    /// * `params` - 筛选
    ///
    /// # 返回
    /// 返回分页。
    ///
    /// # 错误
    /// 参数非法。
    pub async fn booklet_list(
        &self,
        params: SalesSelectionBookletListParams,
    ) -> Result<SalesSelectionBookletPage> {
        Ok(SalesSelectionService::new(self.db.clone())
            .list_booklets(params)
            .await?)
    }

    /// 详情。
    ///
    /// # 参数
    /// * `id` - 选品册
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 不存在。
    pub async fn booklet_detail(&self, id: &str) -> Result<SalesSelectionBookletView> {
        Ok(SalesSelectionService::new(self.db.clone())
            .booklet_detail(id)
            .await?)
    }

    /// 启动准备。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 请求
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回准备中详情。
    ///
    /// # 错误
    /// 状态或版本。
    pub async fn start_prepare(
        &self,
        id: String,
        req: PrepareSalesSelectionRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        let mut req = req;
        req.booklet_id = id;
        let catalog = CatalogAdapter { db: self.db.clone() };
        let images = ImageAdapter {
            db: self.db.clone(),
            storage: std::sync::Arc::new(self.storage.clone()),
        };
        Ok(SalesSelectionService::new(self.db.clone())
            .start_prepare(
                req,
                &actor_id,
                &catalog,
                &images,
                &erp_sales::entity::sales_selection::FirstNonEmptyMemberImage,
            )
            .await?)
    }

    /// 删除陈列。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `item_id` - 陈列
    /// * `req` - 版本
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 状态或版本。
    pub async fn delete_display_item(
        &self,
        booklet_id: String,
        item_id: String,
        req: DeleteDisplayItemRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        Ok(SalesSelectionService::new(self.db.clone())
            .delete_display_item(&booklet_id, &item_id, req.expected_version, &actor_id)
            .await?)
    }

    /// 发布。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 请求
    /// * `actor_id` - 发布人
    ///
    /// # 返回
    /// 返回含链接的详情。
    ///
    /// # 错误
    /// 复核失败或状态不允许。
    pub async fn publish(
        &self,
        id: String,
        req: PublishSalesSelectionRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        let catalog = CatalogAdapter { db: self.db.clone() };
        Ok(SalesSelectionService::new(self.db.clone())
            .publish(&id, req, &actor_id, &catalog, &self.crypto)
            .await?)
    }

    /// 复制链接。
    ///
    /// # 参数
    /// * `id` - 选品册
    ///
    /// # 返回
    /// 返回含路径的详情。
    ///
    /// # 错误
    /// 不存在。
    pub async fn copy_link(&self, id: &str) -> Result<SalesSelectionBookletView> {
        let service = SalesSelectionService::new(self.db.clone());
        let token = service.copy_link(id, &self.crypto).await?;
        let mut view = service.booklet_detail(id).await?;
        let path = format!("/s/{token}");
        view.public_path = Some(path.clone());
        view.public_url = Some(path);
        Ok(view)
    }

    /// 复制当前链接的相对地址。
    ///
    /// # 参数
    /// * `id` - 选品册
    ///
    /// # 返回
    /// 返回 `/s/{token}`，不记录完整令牌。
    ///
    /// # 错误
    /// 无链接。
    pub async fn copy_link_url(&self, id: &str) -> Result<CopyLinkView> {
        let token = SalesSelectionService::new(self.db.clone())
            .copy_link(id, &self.crypto)
            .await?;
        let path = format!("/s/{token}");
        Ok(CopyLinkView {
            public_url: path.clone(),
            public_path: path,
        })
    }

    /// 内部会话快照。
    ///
    /// # 参数
    /// * `id` - 选品册
    ///
    /// # 返回
    /// 返回当前会话。
    ///
    /// # 错误
    /// 无会话。
    pub async fn admin_session(&self, id: &str) -> Result<SalesSelectionSessionView> {
        Ok(SalesSelectionService::new(self.db.clone())
            .admin_session(id)
            .await?)
    }

    /// 更换链接。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回新链接。
    ///
    /// # 错误
    /// 状态或版本。
    pub async fn rotate_link(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        let service = SalesSelectionService::new(self.db.clone());
        let view = service.rotate_link(&id, req, &actor_id, &self.crypto).await?;
        Ok(view)
    }

    /// 关闭。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回已关闭详情。
    ///
    /// # 错误
    /// 状态不允许。
    pub async fn close(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        self.command(id, req, actor_id, CommandKind::Close).await
    }

    /// 撤销访问。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 状态不允许。
    pub async fn revoke(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        self.command(id, req, actor_id, CommandKind::Revoke).await
    }

    /// 作废。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回已作废详情。
    ///
    /// # 错误
    /// 状态不允许。
    pub async fn void(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor_id: String,
    ) -> Result<SalesSelectionBookletView> {
        self.command(id, req, actor_id, CommandKind::Void).await
    }

    /// 方案列表。
    ///
    /// # 参数
    /// * `params` - 筛选
    ///
    /// # 返回
    /// 返回分页。
    ///
    /// # 错误
    /// 参数非法。
    pub async fn proposal_list(
        &self,
        params: SalesSelectionProposalListParams,
    ) -> Result<SalesSelectionProposalPage> {
        Ok(SalesSelectionService::new(self.db.clone())
            .proposal_list(params)
            .await?)
    }

    /// 方案详情。
    ///
    /// # 参数
    /// * `id` - 方案
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 不存在。
    pub async fn proposal_detail(&self, id: &str) -> Result<SalesSelectionProposalView> {
        Ok(SalesSelectionService::new(self.db.clone())
            .proposal_detail(id)
            .await?)
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
        let catalog = CatalogAdapter { db: self.db.clone() };
        let images = ImageAdapter {
            db: self.db.clone(),
            storage: Arc::new(self.storage.clone()),
        };
        Ok(SalesSelectionService::new(self.db.clone())
            .run_due_prepare_tasks(&catalog, &images)
            .await?)
    }

    /// 公开页。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `ip` - 来源 IP
    ///
    /// # 返回
    /// 返回公开视图。
    ///
    /// # 错误
    /// 令牌无效或超限。
    pub async fn public_page(&self, token: &str, ip: &str) -> Result<PublicSelectionPageView> {
        self.admit_public("read", ip, Some(token), 120).await?;
        let mut tx = persistence_core::NoTransaction;
        Ok(SalesSelectionService::new(self.db.clone())
            .public_page_by_token(token, Instant::now(), &mut tx)
            .await?)
    }

    /// 公开保存。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `req` - 请求
    /// * `ip` - 来源 IP
    ///
    /// # 返回
    /// 返回最新公开页。
    ///
    /// # 错误
    /// 冲突、结束或超限。
    pub async fn public_save(
        &self,
        token: String,
        req: SaveSelectionSessionRequest,
        ip: String,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("write", &ip, Some(&token), 30).await?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                let db = db.clone();
                Box::pin(async move {
                    SalesSelectionService::new(db)
                        .public_save(&token, req, Instant::now(), session)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }

    /// 公开提交。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `req` - 请求
    /// * `ip` - 来源 IP
    ///
    /// # 返回
    /// 返回回执。
    ///
    /// # 错误
    /// 冲突、结束或超限。
    pub async fn public_submit(
        &self,
        token: String,
        req: SubmitSelectionSessionRequest,
        ip: String,
    ) -> Result<PublicSelectionPageView> {
        self.admit_public("submit", &ip, Some(&token), 10).await?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                let db = db.clone();
                Box::pin(async move {
                    SalesSelectionService::new(db)
                        .public_submit(&token, req, Instant::now(), session)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }

    /// 公开图片对象键。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `asset_id` - 资产
    /// * `ip` - 来源 IP
    ///
    /// # 返回
    /// 返回快照对象键。
    ///
    /// # 错误
    /// 越权或超限。
    pub async fn public_image_key(&self, token: &str, asset_id: &str, ip: &str) -> Result<String> {
        self.admit_public("image", ip, Some(token), 600).await?;
        let mut tx = persistence_core::NoTransaction;
        Ok(SalesSelectionService::new(self.db.clone())
            .public_image_key(token, asset_id, Instant::now(), &mut tx)
            .await?)
    }

    /// 读取已通过管理端客户权限校验的册图片。
    /// # 错误
    /// 资产不属于本册或对象不存在时拒绝。
    pub async fn admin_image(&self, id: &str, asset_id: &str) -> Result<(Vec<u8>, String)> {
        let key = SalesSelectionService::new(self.db.clone())
            .admin_image_key(id, asset_id)
            .await?;
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
        ImageAdapter {
            db: self.db.clone(),
            storage: Arc::new(self.storage.clone()),
        }
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
        let ok = self
            .db
            .sales_selection_rate()
            .admit(&ip_key, limit, window)
            .await?;
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
            .find_by_token_hash(&hash, &mut persistence_core::NoTransaction)
            .await?
            .is_none()
        {
            return Ok(());
        }
        let token_key = format!("token:{kind}:{hash}");
        let ok = self
            .db
            .sales_selection_rate()
            .admit(&token_key, limit, window)
            .await?;
        if !ok {
            return Err(Error::BusinessLogicError("请求过于频繁，请稍后重试".into()));
        }
        Ok(())
    }

    /// 关闭/撤销/作废命令。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor_id` - 操作人
    /// * `kind` - 命令种类
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 状态不允许。
    async fn command(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor_id: String,
        kind: CommandKind,
    ) -> Result<SalesSelectionBookletView> {
        let service = SalesSelectionService::new(self.db.clone());
        match kind {
            CommandKind::Close => service.close_booklet(&id, req, &actor_id).await,
            CommandKind::Revoke => service.revoke_access(&id, req, &actor_id).await,
            CommandKind::Void => service.void_booklet(&id, req, &actor_id).await,
        }
        .map_err(Error::from)
    }
}

/// 内部命令种类。
enum CommandKind {
    /// 关闭。
    Close,
    /// 撤销。
    Revoke,
    /// 作废。
    Void,
}
