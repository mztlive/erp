//! 公开会话保存、提交与页面读取。

use erp_core::common::time::Instant;
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId};
use persistence_core::Executor;

use super::mapper::{public_kind, public_receipt};
use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    PublicSelectionPageKind, PublicSelectionPageView, SaveSelectionSessionRequest,
    SubmitSelectionSessionRequest,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, SalesSelectionDisplayItem, SalesSelectionProposal, SalesSelectionProposalData,
    SessionChoice, build_proposal_lines, normalize_idempotency_key, request_hash, token_hash,
};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 按令牌读取公开页。
    ///
    /// # 参数
    /// * `token` - 明文令牌
    /// * `now` - 服务端时间
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回公开视图。到期立即结束，不依赖后台把状态改为已关闭。
    ///
    /// # 错误
    /// 令牌无效。
    pub async fn public_page_by_token(
        &self,
        token: &str,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let booklet = self.load_by_token(token, executor).await?;
        let kind = public_kind(&booklet, now);
        if kind == PublicSelectionPageKind::Ended {
            return Ok(Self::public_page(kind, &booklet, &[], None, None));
        }
        let items = if let Some(batch_id) = &booklet.current_batch_id {
            self.items_of(&booklet.base.id, batch_id, executor)
                .await?
                .into_iter()
                .filter(|item| item.is_publishable())
                .collect()
        } else {
            Vec::new()
        };
        let session = self.db.sales_selection_sessions().find_by_booklet(&booklet.base.id, executor).await?;
        let receipt = if kind == PublicSelectionPageKind::Receipt {
            if let Some(proposal_id) = &booklet.proposal_id {
                let proposal = self
                    .db
                    .sales_selection_proposals()
                    .find_by_id(proposal_id.as_ref(), executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("销售方案不存在".into()))?;
                let lines = self
                    .db
                    .sales_selection_proposal_display_lines()
                    .list_by_proposal(&proposal.base.id, executor)
                    .await?;
                Some(public_receipt(&proposal, &lines))
            } else {
                None
            }
        } else {
            None
        };
        let mut page = Self::public_page(kind, &booklet, &items, session.as_ref(), receipt);
        if let (Some(session), crate::entity::sales_selection::SubmitMode::ByQuantity) =
            (session.as_ref(), booklet.submit_mode)
        {
            let prices: std::collections::BTreeMap<_, _> =
                items.iter().map(|item| (item.base.id.clone(), item.price())).collect();
            for choice in &mut page.choices {
                if let (Some(price), Some(quantity)) = (prices.get(&choice.item_id), choice.quantity) {
                    choice.line_amount = Some(crate::entity::sales_selection::try_mul_u32(*price, quantity)?);
                }
            }
            page.total_amount =
                session.quantity_total(booklet.submit_mode, |id| prices.get(id.as_ref()).copied())?;
        }
        Ok(page)
    }

    /// 保存公开会话。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `req` - 保存请求
    /// * `now` - 服务端时间
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回最新公开页。
    ///
    /// # 错误
    /// 结束、版本冲突或选择非法。
    pub async fn public_save(
        &self,
        token: &str,
        req: SaveSelectionSessionRequest,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let mut booklet = self.load_by_token(token, executor).await?;
        if public_kind(&booklet, now) != PublicSelectionPageKind::Selecting {
            return self.public_page_by_token(token, now, executor).await;
        }
        booklet.ensure_public_write(&token_hash(token), now)?;
        let hash = request_hash(&serde_json::to_string(&req).unwrap_or_default());
        if let Some(replay) = self
            .replay_idempotency::<PublicSelectionPageView>(
                IdempotencyOperation::SaveSession,
                &booklet.base.id,
                &key,
                &hash,
                executor,
            )
            .await?
        {
            return Ok(replay);
        }
        let mut session = self
            .db
            .sales_selection_sessions()
            .find_by_booklet(&booklet.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品会话不存在".into()))?;
        let items = self
            .items_of(&booklet.base.id, booklet.current_batch_id.as_deref().unwrap_or(""), executor)
            .await?;
        let allowed: Vec<_> = items
            .iter()
            .filter(|item| item.is_publishable())
            .map(|item| SalesSelectionDisplayItemId::new(item.base.id.clone()))
            .collect();
        let choices = req
            .choices
            .into_iter()
            .map(|choice| SessionChoice {
                display_item_id: SalesSelectionDisplayItemId::new(choice.item_id),
                quantity: choice.quantity,
            })
            .collect();
        if session.session_version != req.expected_session_version {
            return Err(Error::selection_conflict("选择已在其他设备更新，请核对后重试"));
        }
        session.save(req.expected_session_version, choices, booklet.submit_mode, &allowed)?;
        // 乐观更新册文档取得写冲突保护，阻止旧令牌快照迟到写入。
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        self.db.sales_selection_sessions().update(&mut session, executor).await?;
        let page = self.public_page_by_token(token, now, executor).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation: IdempotencyOperation::SaveSession,
                scope_id: &booklet.base.id,
                key: &key,
                hash: &hash,
                result: &page,
                token_version: None,
                booklet_id: Some(SalesSelectionBookletId::new(booklet.base.id.clone())),
            },
            executor,
        )
        .await?;
        Ok(page)
    }

    /// 提交选品，原子创建唯一方案。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `req` - 提交请求
    /// * `now` - 服务端时间
    /// * `executor` - 必须为事务
    ///
    /// # 返回
    /// 返回只读回执页。
    ///
    /// # 错误
    /// 版本冲突、已提交或明细失败则整单回滚。
    pub async fn public_submit(
        &self,
        token: &str,
        req: SubmitSelectionSessionRequest,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let booklet = self.load_by_token(token, executor).await?;
        booklet.ensure_current_token(&token_hash(token))?;
        if public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return self.public_page_by_token(token, now, executor).await;
        }
        let hash = request_hash(&format!("{}:{}", booklet.base.id, req.expected_session_version));
        if let Some(replay) = self
            .replay_idempotency::<PublicSelectionPageView>(
                IdempotencyOperation::Submit,
                &booklet.base.id,
                &key,
                &hash,
                executor,
            )
            .await?
        {
            return Ok(replay);
        }
        if booklet.status == crate::entity::sales_selection::BookletStatus::Submitted {
            return self.public_page_by_token(token, now, executor).await;
        }
        booklet.ensure_public_write(&token_hash(token), now)?;
        let mut session = self
            .db
            .sales_selection_sessions()
            .find_by_booklet(&booklet.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品会话不存在".into()))?;
        if session.session_version != req.expected_session_version {
            return Err(Error::selection_conflict("选择已在其他设备更新，请核对后重试"));
        }
        session.freeze_for_submit(req.expected_session_version)?;
        let items = self
            .items_of(&booklet.base.id, booklet.current_batch_id.as_deref().unwrap_or(""), executor)
            .await?;
        let proposal_id = SalesSelectionProposalId::new(id_generator::next_id());
        let (display_lines, sku_lines, total) = build_proposal_lines(
            &proposal_id,
            booklet.submit_mode,
            &session.choices,
            &items,
            id_generator::next_id,
        )?;
        let proposal = SalesSelectionProposal::new(
            proposal_id.clone(),
            SalesSelectionProposalData {
                proposal_no: format!(
                    "XP{}",
                    &proposal_id.to_string()[..12.min(proposal_id.to_string().len())]
                ),
                customer_id: booklet.customer_id.clone(),
                customer_name: booklet.customer_name.clone(),
                booklet_id: SalesSelectionBookletId::new(booklet.base.id.clone()),
                sales_owner_user_id: booklet.sales_owner_user_id.clone(),
                business_org_unit_id: booklet.business_org_unit_id.clone(),
                batch_id: booklet.current_batch_id.clone().unwrap_or_default(),
                form: booklet.form,
                submit_mode: booklet.submit_mode,
                session_version: session.session_version,
                submitted_at: now,
                total_amount: total,
            },
        )?;
        let mut booklet = booklet;
        booklet.mark_submitted(proposal_id, now)?;
        self.db.sales_selection_proposals().create(&proposal, executor).await?;
        for line in &display_lines {
            self.db.sales_selection_proposal_display_lines().create(line, executor).await?;
        }
        for line in &sku_lines {
            self.db.sales_selection_proposal_sku_lines().create(line, executor).await?;
        }
        self.db.sales_selection_sessions().update(&mut session, executor).await?;
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        let page = self.public_page_by_token(token, now, executor).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation: IdempotencyOperation::Submit,
                scope_id: &booklet.base.id,
                key: &key,
                hash: &hash,
                result: &page,
                token_version: None,
                booklet_id: Some(SalesSelectionBookletId::new(booklet.base.id.clone())),
            },
            executor,
        )
        .await?;
        Ok(page)
    }

    /// 公开图片授权读取。
    ///
    /// # 参数
    /// * `token` - 令牌
    /// * `asset_id` - 文件资产
    /// * `now` - 服务端时间
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回快照对象键。
    ///
    /// # 错误
    /// 令牌无效、结束或不属于本册。
    pub async fn public_image_key(
        &self,
        token: &str,
        asset_id: &str,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let booklet = self.load_by_token(token, executor).await?;
        booklet.ensure_current_token(&token_hash(token))?;
        if public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return Err(Error::BusinessLogicError("选品已结束".into()));
        }
        let items = if let Some(batch_id) = &booklet.current_batch_id {
            self.items_of(&booklet.base.id, batch_id, executor).await?
        } else {
            Vec::new()
        };
        for item in items.iter().filter(|item| item.is_publishable()) {
            if let Some(key) = image_key_if_authorized(item, asset_id) {
                return Ok(key);
            }
        }
        Err(Error::Forbidden("无权查看该图片".into()))
    }

    /// 管理端历史图片授权。入口已校验客户归属，资产必须由该册的历史陈列引用。
    ///
    /// 组合层已在调用方事务内完成对象重验；本方法只映射对象键。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `asset_id` - 文件资产
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回快照对象键。
    ///
    /// # 错误
    /// 任意文件身份、其他册资产或不存在的册均拒绝。
    pub async fn image_key_of(
        &self,
        booklet_id: &str,
        asset_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        self.load_booklet(booklet_id, executor).await?;
        let items = self.db.sales_selection_display_items().list_by_booklet(booklet_id, executor).await?;
        items
            .iter()
            .find_map(|item| image_key_if_authorized(item, asset_id))
            .ok_or_else(|| Error::Forbidden("无权查看该图片".into()))
    }

    /// 管理端历史图片授权。入口已校验客户归属，资产必须由该册的历史陈列引用。
    /// # 错误
    /// 任意文件身份、其他册资产或不存在的册均拒绝。
    pub async fn admin_image_key(&self, booklet_id: &str, asset_id: &str) -> Result<String> {
        let mut tx = persistence_core::NoTransaction;
        self.image_key_of(booklet_id, asset_id, &mut tx).await
    }

    /// 拒绝 P1 换品/自组。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 始终拒绝。
    pub fn reject_custom_package() -> Result<()> {
        Err(Error::BusinessLogicError("本期不支持换品和自组".into()))
    }

    /// 读取批次有效陈列。
    ///
    /// # 参数
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 准备批次
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回有效且未删除的陈列项。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn items_of(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesSelectionDisplayItem>> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        domain.list_effective_items(booklet_id, batch_id, executor).await.map_err(Error::from)
    }

    /// 按令牌哈希加载选品册。
    ///
    /// # 参数
    /// * `token` - 明文令牌
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回选品册。
    ///
    /// # 错误
    /// 令牌无效。
    async fn load_by_token(
        &self,
        token: &str,
        executor: &mut dyn Executor,
    ) -> Result<crate::entity::sales_selection::SalesSelectionBooklet> {
        self.db
            .sales_selection_booklets()
            .find_by_token_hash(&token_hash(token), executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品链接无效".into()))
    }
}

/// 若资产属于该陈列则返回快照对象键。
///
/// # 参数
/// * `item` - 陈列
/// * `asset_id` - 请求的资产
///
/// # 返回
/// 授权时返回对象键。
///
/// # 错误
/// 无。
fn image_key_if_authorized(
    item: &crate::entity::sales_selection::SalesSelectionDisplayItem,
    asset_id: &str,
) -> Option<String> {
    match &item.kind {
        crate::entity::sales_selection::DisplayKind::SingleSku { sku } => sku
            .image
            .as_ref()
            .filter(|image| image.file_asset_id == asset_id)
            .map(|image| image.storage_object_key.clone()),
        crate::entity::sales_selection::DisplayKind::Package { cover, members, .. } => {
            if cover.file_asset_id == asset_id {
                return Some(cover.storage_object_key.clone());
            }
            members.iter().find_map(|sku| {
                sku.image
                    .as_ref()
                    .filter(|image| image.file_asset_id == asset_id)
                    .map(|image| image.storage_object_key.clone())
            })
        },
    }
}
