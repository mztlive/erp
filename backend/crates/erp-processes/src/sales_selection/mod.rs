//! 销售选品跨域流程：客户、商品池、图片与事务。

mod adapters;
mod scope_checks;

use std::sync::Arc;

use application_core::{AuditActor, FilterOption};
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_sales::dto::sales_selection::{
    CopyLinkView, CreateSalesSelectionBookletRequest, DeleteDisplayItemRequest, PrepareSalesSelectionRequest,
    PublicSelectionPageView, PublishSalesSelectionRequest, SalesSelectionBookletListParams,
    SalesSelectionBookletView, SalesSelectionCommandRequest, SalesSelectionProposalListParams,
    SalesSelectionProposalView, SalesSelectionSessionView, SaveSelectionSessionRequest,
    SubmitSelectionSessionRequest,
};
use erp_sales::entity::sales_selection::LinkTokenCrypto;
use erp_sales::ports::sales_selection::SelectionImagePort;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_selection::SalesSelectionService;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use storage::S3Storage;

use self::adapters::{CatalogAdapter, CustomerAdapter, ImageAdapter};
use self::scope_checks::{
    booklet_list_snapshot, copy_link_checked, delete_item_checked, image_key_checked, owner_display_names,
    proposal_list_snapshot, publish_checked, rotate_link_checked, session_checked, start_prepare_checked,
};
use crate::{Error, Result};

/// 销售选品组合根。
pub struct SalesSelectionProcess {
    db: Database,
    storage: S3Storage,
    crypto: LinkTokenCrypto,
    rbac: Option<SharedRbacService>,
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
        Self { db, storage, crypto: LinkTokenCrypto::from_secret(secret), rbac: None }
    }

    /// 注入组合根的授权源；未注入时授权入口失败关闭。
    ///
    /// # 参数
    /// * `rbac` - 当前 RBAC 快照
    ///
    /// # 返回
    /// 返回已注入的流程。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得把 RBAC 交给选品域，只能用于组合层 adapter。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }

    /// 返回列表与候选使用的授权源。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回注入的 RBAC。
    ///
    /// # 错误
    /// 未注入时返回内部错误，不得补公司范围。
    fn require_rbac(&self) -> Result<SharedRbacService> {
        self.rbac.clone().ok_or_else(|| Error::Internal("选品范围授权未装配".into()))
    }

    /// 构造当前流程的选品范围访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已注入组合层 adapter 的访问器。
    ///
    /// # 错误
    /// 未注入 RBAC 时拒绝。
    fn selection_access(&self) -> Result<erp_sales::service::sales_selection::SelectionAccess> {
        let rbac = self.require_rbac()?;
        Ok(crate::adapters::selection_access(self.db.clone(), rbac))
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
        let db = self.db.clone();
        let customer = CustomerAdapter { db: db.clone() };
        let rbac = self.require_rbac()?;
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                let req = req.clone();
                let actor = actor.clone();
                let db = db.clone();
                let rbac = rbac.clone();
                Box::pin(async move {
                    let access = crate::adapters::selection_access(db.clone(), rbac);
                    check_create_scope(
                        &access,
                        &actor,
                        &req.sales_owner_user_id,
                        &req.business_org_unit_id,
                        session,
                    )
                    .await?;
                    SalesSelectionService::new(db)
                        .create(req, actor.id(), &customer, session)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }

    /// 列表。
    ///
    /// 纯读快照不开启事务会话，直接用无事务执行器读取，避免读流量占用事务资源。
    ///
    /// # 参数
    /// * `params` - 筛选
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页与候选。
    ///
    /// # 错误
    /// 参数非法或范围变化。
    pub async fn booklet_list(
        &self,
        params: SalesSelectionBookletListParams,
        actor: AuditActor,
    ) -> Result<SelectionBookletListView> {
        let access = self.selection_access()?;
        let mut executor = NoTransaction;
        booklet_list_snapshot(&access, &self.db, &params, &actor, &mut executor).await
    }

    /// 详情。
    ///
    /// 纯读快照不开启事务会话；负责人显示名与详情读取共用同一无事务执行器。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 不存在或无权查看。
    pub async fn booklet_detail(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionBookletView> {
        let access = self.selection_access()?;
        let mut executor = NoTransaction;
        let book = access.require_booklet(actor, "get", id, &mut executor).await?;
        let mut view = SalesSelectionService::new(self.db.clone())
            .detail_view(&book, None, &mut executor)
            .await
            .map_err(Error::from)?;
        view.sales_owner_name = owner_display_names(&self.db, &[view.sales_owner_user_id.clone()])
            .await?
            .remove(&view.sales_owner_user_id);
        Ok(view)
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
        start_prepare_checked(&self.db, &self.storage, self.selection_access()?, id, req, actor).await
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
        delete_item_checked(&self.db, self.selection_access()?, booklet_id, item_id, req, actor).await
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
        publish_checked(&self.db, &self.storage, &self.crypto, self.selection_access()?, id, req, actor).await
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
        copy_link_checked(&self.db, &self.crypto, self.selection_access()?, id, actor).await
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
        let access = self.selection_access()?;
        let mut executor = NoTransaction;
        access.require_booklet(actor, "copy_link", id, &mut executor).await?;
        let token = SalesSelectionService::new(self.db.clone()).copy_link(id, &self.crypto).await?;
        let path = format!("/s/{token}");
        Ok(CopyLinkView { public_url: path.clone(), public_path: path })
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
        session_checked(&self.db, self.selection_access()?, id, actor).await
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
        rotate_link_checked(&self.db, &self.crypto, self.selection_access()?, id, req, actor).await
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
        self.command(id, req, actor, CommandKind::Close).await
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
        self.command(id, req, actor, CommandKind::Revoke).await
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
        self.command(id, req, actor, CommandKind::Void).await
    }

    /// 方案列表。
    ///
    /// 纯读快照不开启事务会话，直接用无事务执行器读取。
    ///
    /// # 参数
    /// * `params` - 筛选
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回分页与候选。
    ///
    /// # 错误
    /// 参数非法或范围变化。
    pub async fn proposal_list(
        &self,
        params: SalesSelectionProposalListParams,
        actor: AuditActor,
    ) -> Result<SelectionProposalListView> {
        let access = self.selection_access()?;
        let mut executor = NoTransaction;
        proposal_list_snapshot(&access, &self.db, &params, &actor, &mut executor).await
    }

    /// 方案详情。
    ///
    /// 纯读快照不开启事务会话，直接用无事务执行器读取。
    ///
    /// # 参数
    /// * `id` - 方案
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 不存在或无权查看。
    pub async fn proposal_detail(&self, id: &str, actor: &AuditActor) -> Result<SalesSelectionProposalView> {
        let access = self.selection_access()?;
        let mut executor = NoTransaction;
        let proposal = access.require_proposal(actor, "get", id, &mut executor).await?;
        SalesSelectionService::new(self.db.clone())
            .proposal_view_of(&proposal, &mut executor)
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
        let catalog = CatalogAdapter { db: self.db.clone() };
        let images = ImageAdapter { db: self.db.clone(), storage: Arc::new(self.storage.clone()) };
        Ok(SalesSelectionService::new(self.db.clone()).run_due_prepare_tasks(&catalog, &images).await?)
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
        let key = image_key_checked(&self.db, self.selection_access()?, id, asset_id, actor).await?;
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
        ImageAdapter { db: self.db.clone(), storage: Arc::new(self.storage.clone()) }
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
            .find_by_token_hash(&hash, &mut persistence_core::NoTransaction)
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

    /// 关闭/撤销/作废命令。
    ///
    /// # 参数
    /// * `id` - 选品册
    /// * `req` - 命令
    /// * `actor` - 已认证操作人
    /// * `kind` - 命令种类
    ///
    /// # 返回
    /// 返回详情。
    ///
    /// # 错误
    /// 状态不允许或无权操作。
    async fn command(
        &self,
        id: String,
        req: SalesSelectionCommandRequest,
        actor: AuditActor,
        kind: CommandKind,
    ) -> Result<SalesSelectionBookletView> {
        let access = self.selection_access()?;
        let action = kind.action();
        let mut executor = NoTransaction;
        access.require_booklet(&actor, action, &id, &mut executor).await?;
        let service = SalesSelectionService::new(self.db.clone());
        let actor_id = actor.id().to_string();
        match kind {
            CommandKind::Close => service.close_booklet(&id, req, &actor_id).await,
            CommandKind::Revoke => service.revoke_access(&id, req, &actor_id).await,
            CommandKind::Void => service.void_booklet(&id, req, &actor_id).await,
        }
        .map_err(Error::from)
    }
}

/// 选品册列表视图：分页、候选与范围版本。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionBookletListView {
    /// 当前页。
    pub page: erp_sales::dto::sales_selection::SalesSelectionBookletPage,
    /// 完整可见范围内的负责人候选；只含 ID 与显示名。
    pub owner_options: Vec<FilterOption>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 无范围时为 true，前端据此区分空结果。
    pub no_scope: bool,
}

/// 方案列表视图：分页、候选与范围版本。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionProposalListView {
    /// 当前页。
    pub page: erp_sales::dto::sales_selection::SalesSelectionProposalPage,
    /// 完整可见范围内的负责人候选；只含 ID 与显示名。
    pub owner_options: Vec<FilterOption>,
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 无范围时为 true，前端据此区分空结果。
    pub no_scope: bool,
}

/// 内部命令种类。
#[derive(Debug, Clone, Copy)]
enum CommandKind {
    /// 关闭。
    Close,
    /// 撤销。
    Revoke,
    /// 作废。
    Void,
}

impl CommandKind {
    /// 返回本次命令对应的选品册动作。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已注册动作名。
    ///
    /// # 错误
    /// 无。
    const fn action(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Revoke => "revoke",
            Self::Void => "void",
        }
    }
}

/// 校验创建责任在创建动作范围内；提交人不成为负责人。
///
/// # 参数
/// * `access` - 选品范围访问器
/// * `actor` - 已认证操作人
/// * `owner` - 拟写入的显式销售负责人
/// * `org` - 拟写入的业务组织
/// * `executor` - 调用方执行器
///
/// # 返回
/// 范围允许时成功。
///
/// # 错误
/// 无动作权限或责任不在范围内时拒绝。
///
/// # 关键业务约束
/// 不得把创建人或客户提交人当作负责人。
async fn check_create_scope(
    access: &erp_sales::service::sales_selection::SelectionAccess,
    actor: &AuditActor,
    owner: &str,
    org: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    scope_checks::check_create_scope(access, actor, owner, org, executor).await
}
