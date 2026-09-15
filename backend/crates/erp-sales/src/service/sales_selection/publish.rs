//! 发布与链接生命周期：复核、发布、删项、换链、关闭、撤销、作废。
//!
//! 发布复核当前修订与资格，失效整批拒绝，不静默换价换图剔除。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionSessionId};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    PublishSalesSelectionRequest, SalesSelectionBookletView, SalesSelectionCommandRequest,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, LinkTokenCrypto, SalesSelectionSession, ensure_publishable_display,
    normalize_idempotency_key, request_hash,
};
use crate::ports::sales_selection::SelectionCatalogPort;
use crate::repository::SalesSelectionExt;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 发布选品册并建立空会话。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `req` - 发布请求
    /// * `actor_id` - 发布人
    /// * `catalog` - 供给复核端口
    /// * `crypto` - 链接令牌编解码器
    ///
    /// # 返回
    /// 返回已发布详情，不含令牌明文。
    ///
    /// # 错误
    /// 版本冲突、批次漂移、复核失效或写入失败时拒绝。
    pub async fn publish(
        &self,
        booklet_id: &str,
        req: PublishSalesSelectionRequest,
        actor_id: &str,
        catalog: &dyn SelectionCatalogPort,
        crypto: &LinkTokenCrypto,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = request_hash(&serde_json::to_string(&(booklet_id, &req)).unwrap_or_default());
        let mut executor = NoTransaction;
        if let Some(replay) = self
            .replay_idempotency(IdempotencyOperation::Publish, actor_id, &key, &hash, &mut executor)
            .await?
        {
            return Ok(replay);
        }
        let mut booklet = self.load_booklet(booklet_id, &mut executor).await?;
        booklet
            .ensure_version(req.expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        let batch_id =
            req.batch_id.clone().ok_or_else(|| Error::ValidationError("请重新预览并确认准备批次".into()))?;
        self.ensure_batch_current(&booklet, &batch_id)?;
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let items = domain.list_effective_items(&booklet.base.id, &batch_id, &mut executor).await?;
        let tier_ids: Vec<String> = booklet.tiers.iter().map(|tier| tier.tier_id.clone()).collect();
        let publishable = ensure_publishable_display(booklet.form.is_package(), &tier_ids, &items)?;
        self.review_publishable(&booklet, &batch_id, &publishable, catalog, &mut executor).await?;
        let (_token, token_hash, cipher) =
            crypto.issue().map_err(|error| Error::Internal(error.to_string()))?;
        booklet.publish(token_hash, cipher, Instant::now(), actor_id)?;
        let session = SalesSelectionSession::new(
            SalesSelectionSessionId::new(next_id()),
            SalesSelectionBookletId::new(booklet.base.id.clone()),
        );
        self.commit_publish_tx(booklet, session, items, actor_id, key, hash).await
    }

    /// 事务内提交发布写入。
    ///
    /// 复核已在事务外完成；事务内只做册更新、会话创建与幂等落库，
    /// 乐观锁保证并发下整批回滚。
    ///
    /// # 参数
    /// * `booklet` - 已发布态的选品册
    /// * `session` - 新建空会话
    /// * `items` - 有效陈列（仅用于视图）
    /// * `actor_id` - 发布人（幂等作用域）
    /// * `key` - 幂等键
    /// * `hash` - 请求哈希
    ///
    /// # 返回
    /// 返回已发布详情。
    ///
    /// # 错误
    /// 版本冲突或写入失败时回滚。
    async fn commit_publish_tx(
        &self,
        booklet: crate::entity::sales_selection::SalesSelectionBooklet,
        session: SalesSelectionSession,
        items: Vec<crate::entity::sales_selection::SalesSelectionDisplayItem>,
        actor_id: &str,
        key: String,
        hash: String,
    ) -> Result<SalesSelectionBookletView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let scope = actor_id.to_string();
        let token_version = booklet.link_token_version;
        let booklet_id = SalesSelectionBookletId::new(booklet.base.id.clone());
        let mut booklet_tx = booklet;
        let session_tx = session;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let executor: &mut dyn Executor = session;
                    db.sales_selection_booklets().update(&mut booklet_tx, executor).await?;
                    db.sales_selection_sessions().create(&session_tx, executor).await?;
                    let view = Self::booklet_view(&booklet_tx, &items, None, None);
                    let record = Self::idempotency_record(IdempotencyStoreInput {
                        operation: IdempotencyOperation::Publish,
                        scope_id: &scope,
                        key: &key,
                        hash: &hash,
                        result: &view,
                        token_version: Some(token_version),
                        booklet_id: Some(booklet_id.clone()),
                    })?;
                    db.sales_selection_idempotency().create(&record, executor).await?;
                    Ok::<_, Error>(view)
                })
            })
            .await
    }

    /// 校验发布批次未漂移。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `batch_id` - 已确认批次
    ///
    /// # 返回
    /// 一致返回 `Ok(())`。
    ///
    /// # 错误
    /// 批次不一致时拒绝重新预览。
    fn ensure_batch_current(
        &self,
        booklet: &crate::entity::sales_selection::SalesSelectionBooklet,
        batch_id: &str,
    ) -> Result<()> {
        if booklet.current_batch_id.as_deref() == Some(batch_id) {
            return Ok(());
        }
        Err(Error::ValidationError("准备批次已变化，请重新预览后发布".into()))
    }

    /// 复核陈列修订与资格。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `batch_id` - 批次
    /// * `publishable` - 可发布陈列
    /// * `catalog` - 复核端口
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 全部有效返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一成员失效或修订变化整批拒绝并列明。
    async fn review_publishable(
        &self,
        booklet: &crate::entity::sales_selection::SalesSelectionBooklet,
        batch_id: &str,
        publishable: &[&crate::entity::sales_selection::SalesSelectionDisplayItem],
        catalog: &dyn SelectionCatalogPort,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let refs: Vec<(String, String)> = publishable
            .iter()
            .flat_map(|item| item.sellable_refs())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let qualified = catalog.qualified_refs(&refs, BusinessDate::today()).await?;
        ensure_refs_qualified(&refs, &qualified)?;
        self.ensure_revisions_unchanged(booklet, batch_id, publishable, executor).await
    }

    /// 复核修订未变化。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `batch_id` - 批次
    /// * `publishable` - 可发布陈列
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 修订一致返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一修订变化整批拒绝。
    async fn ensure_revisions_unchanged(
        &self,
        booklet: &crate::entity::sales_selection::SalesSelectionBooklet,
        batch_id: &str,
        publishable: &[&crate::entity::sales_selection::SalesSelectionDisplayItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        let members = domain.list_pool_members(&booklet.base.id, batch_id, executor).await?;
        ensure_revision_match(&members, publishable)
    }

    /// 待发布删除陈列项。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `display_id` - 陈列项身份
    /// * `expected_version` - 册版本
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回更新后的详情。
    ///
    /// # 错误
    /// 版本冲突、非待发布或项不属于当前批次时拒绝。
    pub async fn delete_display_item(
        &self,
        booklet_id: &str,
        display_id: &str,
        expected_version: u64,
        actor_id: &str,
    ) -> Result<SalesSelectionBookletView> {
        let service = Self::new(self.db.clone());
        let (booklet_id, display_id, actor_id) =
            (booklet_id.to_string(), display_id.to_string(), actor_id.to_string());
        self.db
            .client()
            .with_transaction(move |tx| {
                Box::pin(async move {
                    service
                        .delete_display_item_in(&booklet_id, &display_id, expected_version, &actor_id, tx)
                        .await
                })
            })
            .await
    }

    /// 陈列删除与册版本竞争更新共用调用方事务，发布竞争时整次回滚。
    async fn delete_display_item_in(
        &self,
        booklet_id: &str,
        display_id: &str,
        expected_version: u64,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
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
        booklet.record_display_edit(actor_id)?;
        self.db.sales_selection_display_items().update(&mut item, executor).await?;
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        self.detail_view(&booklet, Some(batch), executor).await
    }

    /// 更换公开链接。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `req` - 版本与幂等
    /// * `actor_id` - 操作人
    /// * `crypto` - 令牌编解码器
    ///
    /// # 返回
    /// 返回详情，原令牌立即失效。
    ///
    /// # 错误
    /// 版本冲突或非已发布时拒绝。
    pub async fn rotate_link(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor_id: &str,
        crypto: &LinkTokenCrypto,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(
            booklet_id,
            req,
            actor_id,
            IdempotencyOperation::RotateLink,
            Some(crypto.clone()),
        )
        .await
    }

    /// 复制当前链接令牌。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `crypto` - 令牌编解码器
    ///
    /// # 返回
    /// 返回明文令牌，调用方拼接公开地址，不记录完整链接。
    ///
    /// # 错误
    /// 无链接或密文损坏时拒绝。
    pub async fn copy_link(&self, booklet_id: &str, crypto: &LinkTokenCrypto) -> Result<String> {
        let mut executor = NoTransaction;
        let booklet = self.load_booklet(booklet_id, &mut executor).await?;
        if booklet.link_revoked || booklet.link_token_ciphertext.is_none() {
            return Err(Error::ValidationError("当前没有可复制的选品链接".into()));
        }
        let cipher = booklet.link_token_ciphertext.clone().unwrap_or_default();
        tracing::info!(booklet_id, "复制选品链接");
        crypto.decrypt(&cipher).map_err(|error| Error::Internal(error.to_string()))
    }

    /// 关闭未提交选品册。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `req` - 版本与幂等
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回已关闭详情。
    ///
    /// # 错误
    /// 版本冲突或非已发布时拒绝。
    pub async fn close_booklet(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor_id: &str,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor_id, IdempotencyOperation::Close, None).await
    }

    /// 撤销已提交选品册的链接访问。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `req` - 版本与幂等
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回详情，不改变已提交状态。
    ///
    /// # 错误
    /// 非已提交时拒绝。
    pub async fn revoke_access(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor_id: &str,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor_id, IdempotencyOperation::RevokeAccess, None).await
    }

    /// 作废发布前选品册。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册身份
    /// * `req` - 版本与幂等
    /// * `actor_id` - 操作人
    ///
    /// # 返回
    /// 返回已作废详情。
    ///
    /// # 错误
    /// 终态或已发布后拒绝。
    pub async fn void_booklet(
        &self,
        booklet_id: &str,
        req: SalesSelectionCommandRequest,
        actor_id: &str,
    ) -> Result<SalesSelectionBookletView> {
        self.lifecycle_command(booklet_id, req, actor_id, IdempotencyOperation::Void, None).await
    }
}

/// 校验资格引用全覆盖。
///
/// # 参数
/// * `refs` - 陈列精确引用
/// * `qualified` - 仍合格的引用
///
/// # 返回
/// 全合格返回 `Ok(())`。
///
/// # 错误
/// 任一失效整批拒绝并列明。
fn ensure_refs_qualified(refs: &[(String, String)], qualified: &[(String, String)]) -> Result<()> {
    if refs.iter().all(|item| qualified.contains(item)) {
        return Ok(());
    }
    let missing: Vec<String> =
        refs.iter().filter(|item| !qualified.contains(item)).map(|item| item.0.clone()).collect();
    Err(Error::ValidationError(format!("以下陈列已失效: {}", missing.join("、"))))
}

/// 校验修订与快照一致。
///
/// # 参数
/// * `members` - 批次冻结成员
/// * `publishable` - 可发布陈列
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 任一修订变化整批拒绝。
fn ensure_revision_match(
    members: &[crate::entity::sales_selection::SalesSelectionPoolMember],
    publishable: &[&crate::entity::sales_selection::SalesSelectionDisplayItem],
) -> Result<()> {
    let revisions: std::collections::BTreeMap<&str, &str> = members
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

/// 校验陈列归属当前批次。
///
/// # 参数
/// * `item` - 陈列项
/// * `booklet_id` - 选品册
/// * `batch_id` - 当前批次
///
/// # 返回
/// 归属一致返回 `Ok(())`。
///
/// # 错误
/// 跨册或跨批次时拒绝。
fn ensure_item_of_batch(
    item: &crate::entity::sales_selection::SalesSelectionDisplayItem,
    booklet_id: &str,
    batch_id: &str,
) -> Result<()> {
    if item.booklet_id.as_ref() == booklet_id && item.batch_id == batch_id {
        return Ok(());
    }
    Err(Error::ValidationError("陈列项不属于当前批次".into()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{ensure_item_of_batch, ensure_refs_qualified};

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
        let item = crate::entity::sales_selection::SalesSelectionDisplayItem::single_sku(
            erp_core::ids::SalesSelectionDisplayItemId::new("d1"),
            erp_core::ids::SalesSelectionBookletId::new("b1"),
            "batch-1".into(),
            crate::entity::sales_selection::SkuSnapshot {
                sku_id: erp_core::ids::SkuId::new("a"),
                sku_revision_id: erp_core::ids::SkuRevisionId::new("rev-a"),
                product_id: erp_core::ids::ProductId::new("p-a"),
                product_kind: "PHYSICAL".into(),
                category_id: None,
                name: "a".into(),
                specification_attributes: Vec::new(),
                unit: "件".into(),
                image: None,
                sales_visible_price_gross: erp_core::money::Amount::from_str("10.00").unwrap(),
            },
        )
        .unwrap();
        assert!(ensure_item_of_batch(&item, "b1", "batch-1").is_ok());
        assert!(ensure_item_of_batch(&item, "b1", "batch-2").is_err());
    }
}
