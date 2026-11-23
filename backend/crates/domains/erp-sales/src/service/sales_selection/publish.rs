//! 发布与链接生命周期：复核、发布、删项、换链、关闭、撤销、作废。
//!
//! 发布复核当前修订与资格，失效整批拒绝，不静默换价换图剔除。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionSessionId};
use erp_permissions::sales_selection;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};

use crate::entity::sales_selection::SalesSelectionPoolMember;

mod replay;

use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    PublishSalesSelectionRequest, SalesSelectionBookletView, SalesSelectionCommandRequest,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, LinkTokenCrypto, SalesSelectionBooklet, SalesSelectionDisplayItem,
    SalesSelectionSession, ensure_publishable_display, normalize_idempotency_key, request_hash,
};
use crate::ports::sales_selection::SelectionCatalogPort;
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::SalesSelectionDomainRepository;
use crate::{Error, Result};

struct PublishCommand {
    booklet_id: String,
    request: PublishSalesSelectionRequest,
    actor: AuditActor,
    key: String,
    hash: String,
    crypto: LinkTokenCrypto,
}

impl SalesSelectionService {
    /// 管理端发布已准备的选品册时，复核当前批次资格并原子保存发布状态与初始会话。
    /// 返回已发布详情，不含令牌明文。
    /// 版本冲突、批次漂移、复核失效或写入失败时拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `req`：发布请求
    /// - `actor`：已认证发布人
    /// - `catalog`：供给复核端口
    /// - `crypto`：链接令牌编解码器
    pub async fn publish(
        &self,
        booklet_id: &str,
        req: PublishSalesSelectionRequest,
        actor: &AuditActor,
        catalog: Arc<dyn SelectionCatalogPort>,
        crypto: &LinkTokenCrypto,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = request_hash(&serde_json::to_string(&(booklet_id, &req)).unwrap_or_default());
        let command = PublishCommand {
            booklet_id: booklet_id.to_owned(),
            request: req,
            actor: actor.clone(),
            key,
            hash,
            crypto: crypto.clone(),
        };
        let service = self.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.apply_publish(&command, catalog.as_ref(), executor).await })
            })
            .await
    }

    // 发布入口先查同键结果，再准备或执行首次发布；回放检查与所有写入共享入口持有的执行器。
    async fn apply_publish(
        &self,
        command: &PublishCommand,
        catalog: &dyn SelectionCatalogPort,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let PublishCommand { booklet_id, request: req, actor, key, hash, crypto } = command;
        if let Some(replay) = self.publish_replay(booklet_id, actor, key, hash, executor).await? {
            return Ok(replay);
        }

        let mut booklet = self.load_booklet(booklet_id, executor).await?;
        booklet
            .ensure_version(req.expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        let batch_id = req
            .batch_id
            .as_deref()
            .ok_or_else(|| Error::ValidationError("请重新预览并确认准备批次".into()))?;
        self.ensure_batch_current(&booklet, batch_id)?;

        let domain = SalesSelectionDomainRepository::new(&self.db);
        let items = domain.list_effective_items(&booklet.base.id, batch_id, executor).await?;
        let tier_ids: Vec<String> = booklet.tiers.iter().map(|tier| tier.tier_id.clone()).collect();
        let publishable = ensure_publishable_display(booklet.form.is_package(), &tier_ids, &items)?;
        self.review_publishable(&booklet, batch_id, &publishable, catalog, executor).await?;

        let (_token, token_hash, cipher) =
            crypto.issue().map_err(|error| Error::Internal(error.to_string()))?;
        booklet.publish(token_hash, cipher, Instant::now(), actor.id())?;
        let session = self.publish_session(&booklet)?;
        self.commit_publish(booklet, session, items, command, executor).await
    }

    fn publish_session(&self, book: &SalesSelectionBooklet) -> Result<SalesSelectionSession> {
        if book.access_password_hash.is_none() {
            return Err(Error::ValidationError("请先设置选品访问密码".into()));
        }
        let book_id = SalesSelectionBookletId::new(book.base.id.clone());
        Ok(SalesSelectionSession::new(SalesSelectionSessionId::new(next_id()), book_id))
    }

    // 复核完成后在发布事务内更新册并写入会话、幂等记录，避免返回无法重新打开的发布结果。
    // 返回已发布详情。
    // 失败条件：版本冲突或写入失败时回滚。
    //
    // 输入约定：
    // - `booklet`：已发布态的选品册
    // - `session`：新建空会话
    // - `items`：有效陈列（仅用于视图）
    // - `command`：已认证发布人、幂等键与请求哈希
    // - `executor`：发布入口持有的事务执行器
    async fn commit_publish(
        &self,
        mut booklet: SalesSelectionBooklet,
        session: SalesSelectionSession,
        items: Vec<SalesSelectionDisplayItem>,
        command: &PublishCommand,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let token_version = booklet.link_token_version;
        let booklet_id = SalesSelectionBookletId::new(booklet.base.id.clone());
        self.authorize_booklet(&command.actor, &booklet.base.id, &sales_selection::BOOKLET_PUBLISH, executor)
            .await?;

        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        self.db.sales_selection_sessions().create(&session, executor).await?;

        let view = Self::booklet_view(&booklet, &items, None, None);
        let record = Self::idempotency_record(IdempotencyStoreInput {
            operation: IdempotencyOperation::Publish,
            scope_id: command.actor.id(),
            key: &command.key,
            hash: &command.hash,
            result: &view,
            token_version: Some(token_version),
            booklet_id: Some(booklet_id),
        })?;
        self.db.sales_selection_idempotency().create(&record, executor).await?;
        Ok(view)
    }

    // 正式发布前再次确认准备批次仍是当前批次，拒绝发布已被新准备结果替代的陈列。
    // 失败条件：批次不一致时拒绝重新预览。
    //
    // 输入约定：
    // - `booklet`：选品册
    // - `batch_id`：已确认批次
    fn ensure_batch_current(&self, booklet: &SalesSelectionBooklet, batch_id: &str) -> Result<()> {
        if booklet.current_batch_id.as_deref() == Some(batch_id) {
            return Ok(());
        }
        Err(Error::ValidationError("准备批次已变化，请重新预览后发布".into()))
    }

    // 对拟发布陈列核对冻结 SKU 修订及其当日可售资格，防止商品资格变化后仍按旧快照发布。
    // 失败条件：任一成员失效或修订变化整批拒绝并列明。
    //
    // 输入约定：
    // - `booklet`：选品册
    // - `batch_id`：批次
    // - `publishable`：可发布陈列
    // - `catalog`：复核端口
    async fn review_publishable(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: &str,
        publishable: &[&SalesSelectionDisplayItem],
        catalog: &dyn SelectionCatalogPort,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let refs: Vec<(String, String)> = publishable
            .iter()
            .flat_map(|item| item.sellable_refs())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let qualified = catalog.qualified_refs(&refs, BusinessDate::today(), executor).await?;
        ensure_refs_qualified(&refs, &qualified)?;

        self.ensure_revisions_unchanged(booklet, batch_id, publishable, executor).await
    }

    // 发布复核时对照当前 SKU 修订与陈列冻结修订，不因源商品更新而静默改写选品内容。
    // 结果：修订一致返回 `Ok(())`。
    // 任一修订变化整批拒绝。
    //
    // 输入约定：
    // - `booklet`：选品册
    // - `batch_id`：批次
    // - `publishable`：可发布陈列
    async fn ensure_revisions_unchanged(
        &self,
        booklet: &SalesSelectionBooklet,
        batch_id: &str,
        publishable: &[&SalesSelectionDisplayItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let domain = SalesSelectionDomainRepository::new(&self.db);
        let members = domain.list_pool_members(&booklet.base.id, batch_id, executor).await?;
        ensure_revision_match(&members, publishable)
    }

    /// 册尚待发布时移除当前批次指定陈列，沿用发布前的版本竞争规则。
    /// 返回更新后的详情。
    /// 版本冲突、非待发布或项不属于当前批次时拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `display_id`：陈列项身份
    /// - `expected_version`：册版本
    /// - `actor`：已认证操作人
    pub async fn delete_display_item(
        &self,
        booklet_id: &str,
        display_id: &str,
        expected_version: u64,
        actor: &AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        let service = self.clone();
        let (booklet_id, display_id, actor) = (booklet_id.to_string(), display_id.to_string(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .delete_display_item_in(&booklet_id, &display_id, expected_version, &actor, executor)
                        .await
                })
            })
            .await
    }

    // 在调用方事务中同时删除陈列并以乐观锁更新册；与发布发生版本冲突时本次删除整体回滚。
    async fn delete_display_item_in(
        &self,
        booklet_id: &str,
        display_id: &str,
        expected_version: u64,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        self.authorize_booklet(actor, booklet_id, &sales_selection::BOOKLET_DELETE_ITEM, executor).await?;
        let mut booklet = self.load_booklet(booklet_id, executor).await?;
        booklet
            .ensure_version(expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        if !booklet.status.allows_delete_display() {
            return Err(Error::ValidationError("只有待发布可以删除陈列项".into()));
        }

        let batch = booklet.current_batch_id.clone().unwrap_or_default();
        let mut item = self
            .db
            .sales_selection_display_items()
            .find_by_id(display_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("陈列项不存在".into()))?;
        ensure_item_of_batch(&item, &booklet.base.id, &batch)?;

        item.remove()?;
        booklet.record_display_edit(actor.id())?;
        self.db.sales_selection_display_items().update(&mut item, executor).await?;
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        self.detail_view(&booklet, Some(batch), executor).await
    }

    /// 管理端需要换掉旧访问链接时更新令牌及其幂等结果，不生成新的选品册。
    /// 返回详情，原令牌立即失效。
    /// 版本冲突或非已发布时拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `req`：版本与幂等
    /// - `actor`：已认证操作人
    /// - `crypto`：令牌编解码器
    pub async fn rotate_link(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor: &AuditActor,
        crypto: &LinkTokenCrypto,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor, IdempotencyOperation::RotateLink, Some(crypto.clone()))
            .await
    }

    /// 在已授权且现有链接仍可读时恢复当前公开令牌，供管理端复制链接而不轮换令牌。
    /// 返回明文令牌，调用方拼接公开地址，不记录完整链接。
    /// 无链接或密文损坏时拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `crypto`：令牌编解码器
    /// - `executor`：已完成精确册授权的原快照执行器。
    pub async fn copy_link(
        &self,
        booklet_id: &str,
        crypto: &LinkTokenCrypto,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let booklet = self.load_booklet(booklet_id, executor).await?;
        if booklet.link_revoked || booklet.link_token_ciphertext.is_none() {
            return Err(Error::ValidationError("当前没有可复制的选品链接".into()));
        }
        let cipher = booklet.link_token_ciphertext.as_deref().unwrap_or_default();
        tracing::info!(booklet_id, "复制选品链接");
        crypto.decrypt(cipher).map_err(|error| Error::Internal(error.to_string()))
    }

    /// 不再收集选择时关闭尚未提交的已发布选品册，通过共同生命周期入口处理重放与版本检查。
    /// 返回已关闭详情。
    /// 版本冲突或非已发布时拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `req`：版本与幂等
    /// - `actor`：已认证操作人
    pub async fn close_booklet(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor: &AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor, IdempotencyOperation::Close, None).await
    }

    /// 管理端撤销已提交册的公开访问时停用链接，保留已提交记录。
    /// 返回详情，不改变已提交状态。
    /// 非已提交时拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `req`：版本与幂等
    /// - `actor`：已认证操作人
    pub async fn revoke_access(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor: &AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor, IdempotencyOperation::RevokeAccess, None).await
    }

    /// 发布之前放弃本册时迁移到作废状态，不误作关闭已发布的册。
    /// 返回已作废详情。
    /// 终态或已发布后拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册身份
    /// - `req`：版本与幂等
    /// - `actor`：已认证操作人
    pub async fn void_booklet(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor: &AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor, IdempotencyOperation::Void, None).await
    }
}

// 发布前要求拟展示的每个精确 SKU 修订都出现在资格复核结果中，缺一即拒绝发布。
// 失败条件：任一失效整批拒绝并列明。
//
// 输入约定：
// - `refs`：陈列精确引用
// - `qualified`：仍合格的引用
fn ensure_refs_qualified(refs: &[(String, String)], qualified: &[(String, String)]) -> Result<()> {
    if refs.iter().all(|item| qualified.contains(item)) {
        return Ok(());
    }
    let missing: Vec<&str> =
        refs.iter().filter(|item| !qualified.contains(item)).map(|item| item.0.as_str()).collect();
    Err(Error::ValidationError(format!("以下陈列已失效: {}", missing.join("、"))))
}

// 核对复验得到的 SKU 修订与准备时冻结值，两者不一致时保留旧快照并拒绝发布。
// 任一修订变化整批拒绝。
//
// 输入约定：
// - `members`：批次冻结成员
// - `publishable`：可发布陈列
fn ensure_revision_match(
    members: &[SalesSelectionPoolMember],
    publishable: &[&SalesSelectionDisplayItem],
) -> Result<()> {
    let revisions: BTreeMap<&str, &str> = members
        .iter()
        .map(|member| (member.sku.sku_id.as_ref(), member.sku.sku_revision_id.as_ref()))
        .collect();
    let mut changed = Vec::new();
    for item in publishable {
        for (sku_id, revision) in item.sellable_refs() {
            if revisions.get(sku_id.as_str()).copied() != Some(revision.as_str()) {
                changed.push(sku_id);
            }
        }
    }

    if changed.is_empty() {
        return Ok(());
    }
    Err(Error::ValidationError(format!("以下陈列修订已变化: {}", changed.join("、"))))
}

// 编辑当前册陈列前校验条目确实属于本册的有效批次，避免处理另一册或旧批次的条目。
// 跨册或跨批次时拒绝。
//
// 输入约定：
// - `item`：陈列项
// - `booklet_id`：选品册
// - `batch_id`：当前批次
fn ensure_item_of_batch(item: &SalesSelectionDisplayItem, booklet_id: &str, batch_id: &str) -> Result<()> {
    if item.booklet_id.as_ref() == booklet_id && item.batch_id == batch_id {
        return Ok(());
    }
    Err(Error::ValidationError("陈列项不属于当前批次".into()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        ProductId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SkuId, SkuRevisionId,
    };
    use erp_core::money::Amount;

    use super::{ensure_item_of_batch, ensure_refs_qualified};
    use crate::entity::sales_selection::{SalesSelectionDisplayItem, SkuSnapshot};

    #[test]
    fn refs_must_fully_cover() {
        let refs = vec![("a".to_string(), "rev-a".to_string())];
        assert!(ensure_refs_qualified(&refs, &refs).is_ok());
        assert!(ensure_refs_qualified(&[refs[0].clone(), refs[0].clone()], &refs).is_ok());
        let qualified = vec![];
        let error = ensure_refs_qualified(&refs, &qualified).unwrap_err();
        assert!(error.to_string().contains('a'));
    }

    #[test]
    fn item_batch_mismatch_rejected() {
        let item = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("d1"),
            SalesSelectionBookletId::new("b1"),
            "batch-1".into(),
            SkuSnapshot {
                sku_id: SkuId::new("a"),
                sku_revision_id: SkuRevisionId::new("rev-a"),
                product_id: ProductId::new("p-a"),
                product_kind: "PHYSICAL".into(),
                category_id: None,
                name: "a".into(),
                specification_attributes: Vec::new(),
                unit: "件".into(),
                image: None,
                reference_prices: None,
                sales_visible_price_gross: Amount::from_str("10.00").unwrap(),
            },
        )
        .unwrap();
        assert!(ensure_item_of_batch(&item, "b1", "batch-1").is_ok());
        assert!(ensure_item_of_batch(&item, "b1", "batch-2").is_err());
    }
}
