//! 公开会话按授权凭证隔离保存、提交和回执。

use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId};
use persistence_core::Executor;

use super::mapper::{public_kind, public_receipt};
use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    PublicReceiptView, PublicSelectionPageKind, PublicSelectionPageView, SaveSelectionSessionRequest,
    SubmitSelectionSessionRequest,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionProposal,
    SalesSelectionProposalData, SalesSelectionSession, SelectionGrant, SelectionRecipient, SessionChoice,
    SubmitMode, build_proposal_lines, normalize_idempotency_key, request_hash, token_hash,
};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 按当前令牌与个人授权读取公开页。
    ///
    /// # 参数
    /// `token` 为链接令牌，`grant` 为解码授权，`now` 为服务端时间，`executor` 为读取边界。
    ///
    /// # 返回
    /// 终态返回已结束页；其它未授权访问返回锁定页，两者均不含敏感信息。
    ///
    /// # 错误
    /// 链接不存在或数据库读取失败时拒绝。
    pub async fn public_page_by_token(
        &self,
        token: &str,
        grant: Option<&SelectionGrant>,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let booklet = self.load_by_token(token, executor).await?;
        if let Some(page) = ended_public_page(&booklet, now) {
            return Ok(page);
        }
        let Some(grant) = grant else {
            return Ok(Self::public_page(PublicSelectionPageKind::Locked, &booklet, &[], None, None));
        };
        let session = match self.authorized_session(&booklet, grant, now, executor).await {
            Ok(session) => session,
            Err(Error::Forbidden(_)) => {
                return Ok(Self::public_page(PublicSelectionPageKind::Locked, &booklet, &[], None, None));
            },
            Err(error) => return Err(error),
        };
        self.participant_page(&booklet, &session, now, executor).await
    }

    /// 保存授权参与人的完整选择集合。
    ///
    /// # 参数
    /// `token`、`grant` 为访问资格，`req` 为带版本和幂等键的选择，`now` 为当前时间，`executor` 必须为事务。
    ///
    /// # 返回
    /// 返回该参与人的最新页面。
    ///
    /// # 错误
    /// 授权失效、版本冲突、超额度或非法选择时原子拒绝。
    pub async fn public_save(
        &self,
        token: &str,
        grant: &SelectionGrant,
        req: SaveSelectionSessionRequest,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let mut booklet = self.load_by_token(token, executor).await?;
        let mut session = self.authorized_session(&booklet, grant, now, executor).await?;
        if public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return Ok(Self::public_page(PublicSelectionPageKind::Ended, &booklet, &[], None, None));
        }
        let scope = participant_scope(&booklet, &session);
        let hash = request_hash(
            &serde_json::to_string(&req).map_err(|_| Error::Internal("选择载荷无法编码".into()))?,
        );
        if self
            .replay_idempotency::<PublicSelectionPageView>(
                IdempotencyOperation::SaveSession,
                &scope,
                &key,
                &hash,
                executor,
            )
            .await?
            .is_some()
        {
            return self.participant_page(&booklet, &session, now, executor).await;
        }
        if session.frozen || public_kind(&booklet, now) != PublicSelectionPageKind::Selecting {
            return self.participant_page(&booklet, &session, now, executor).await;
        }
        booklet.ensure_public_write(&token_hash(token), now)?;
        let items = self.current_items(&booklet, executor).await?;
        apply_choices(&booklet, &mut session, req, &items)?;
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        self.db.sales_selection_sessions().update(&mut session, executor).await?;
        let page = self.participant_page(&booklet, &session, now, executor).await?;
        self.store_public_result(
            IdempotencyOperation::SaveSession,
            &booklet,
            (&scope, &key, &hash),
            &page,
            executor,
        )
        .await?;
        Ok(page)
    }

    /// 冻结个人选择与地址，并原子生成个人销售方案。
    ///
    /// # 参数
    /// `token`、`grant` 为访问资格，`req` 为最终确认，`now` 为当前时间，`executor` 必须为事务。
    ///
    /// # 返回
    /// 返回当前参与人的只读回执。
    ///
    /// # 错误
    /// 授权、版本、额度、地址或任何写入失败时整次拒绝。
    pub async fn public_submit(
        &self,
        token: &str,
        grant: &SelectionGrant,
        req: SubmitSelectionSessionRequest,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let mut booklet = self.load_by_token(token, executor).await?;
        let mut session = self.authorized_session(&booklet, grant, now, executor).await?;
        if public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return Ok(Self::public_page(PublicSelectionPageKind::Ended, &booklet, &[], None, None));
        }
        let scope = participant_scope(&booklet, &session);
        let hash = request_hash(
            &serde_json::to_string(&req).map_err(|_| Error::Internal("提交载荷无法编码".into()))?,
        );
        if self
            .replay_idempotency::<PublicSelectionPageView>(
                IdempotencyOperation::Submit,
                &scope,
                &key,
                &hash,
                executor,
            )
            .await?
            .is_some()
        {
            return self.participant_page(&booklet, &session, now, executor).await;
        }
        if session.frozen || public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return self.participant_page(&booklet, &session, now, executor).await;
        }
        booklet.ensure_public_write(&token_hash(token), now)?;
        ensure_session_version(&session, req.expected_session_version)?;
        session.recipient = submit_recipient(booklet.submit_mode, req.recipient)?;
        session
            .freeze_for_submit(req.expected_session_version)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let items = self.current_items(&booklet, executor).await?;
        ensure_budget(&booklet, &session, &items)?;
        let proposal_id = self.create_participant_proposal(&booklet, &session, &items, now, executor).await?;
        session.proposal_id = Some(proposal_id.clone());
        booklet.mark_submitted(proposal_id, now)?;
        self.db.sales_selection_sessions().update(&mut session, executor).await?;
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        let page = self.participant_page(&booklet, &session, now, executor).await?;
        self.store_public_result(
            IdempotencyOperation::Submit,
            &booklet,
            (&scope, &key, &hash),
            &page,
            executor,
        )
        .await?;
        Ok(page)
    }

    async fn create_participant_proposal(
        &self,
        booklet: &SalesSelectionBooklet,
        session: &SalesSelectionSession,
        items: &[SalesSelectionDisplayItem],
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposalId> {
        let proposal_id = SalesSelectionProposalId::new(id_generator::next_id());
        let (display_lines, sku_lines, total) = build_proposal_lines(
            &proposal_id,
            booklet.submit_mode,
            &session.choices,
            items,
            id_generator::next_id,
        )?;
        let proposal = SalesSelectionProposal::new(
            proposal_id.clone(),
            SalesSelectionProposalData {
                proposal_no: format!("XP{}", &proposal_id.as_ref()[..12.min(proposal_id.as_ref().len())]),
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
                participant_id: session.participant_id.clone(),
                recipient: session.recipient.clone(),
            },
        )?;
        self.db.sales_selection_proposals().create(&proposal, executor).await?;
        for line in &display_lines {
            self.db.sales_selection_proposal_display_lines().create(line, executor).await?;
        }
        for line in &sku_lines {
            self.db.sales_selection_proposal_sku_lines().create(line, executor).await?;
        }
        Ok(proposal_id)
    }

    async fn store_public_result(
        &self,
        operation: IdempotencyOperation,
        booklet: &SalesSelectionBooklet,
        replay: (&str, &str, &str),
        page: &PublicSelectionPageView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.store_idempotency(
            IdempotencyStoreInput {
                operation,
                scope_id: replay.0,
                key: replay.1,
                hash: replay.2,
                result: page,
                token_version: Some(booklet.link_token_version),
                booklet_id: Some(SalesSelectionBookletId::new(booklet.base.id.clone())),
            },
            executor,
        )
        .await
    }

    pub(super) async fn authorized_session(
        &self,
        booklet: &SalesSelectionBooklet,
        grant: &SelectionGrant,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionSession> {
        let session = self
            .db
            .sales_selection_sessions()
            .find_by_id(&grant.session_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("选品授权无效".into()))?;
        grant
            .ensure_current(booklet, &session, now)
            .map_err(|_| Error::Forbidden("选品授权已失效，请重新输入密码".into()))?;
        Ok(session)
    }

    pub(super) async fn participant_page(
        &self,
        booklet: &SalesSelectionBooklet,
        session: &SalesSelectionSession,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PublicSelectionPageView> {
        let mut kind = public_kind(booklet, now);
        if kind == PublicSelectionPageKind::Ended {
            return Ok(Self::public_page(kind, booklet, &[], None, None));
        }
        if session.frozen {
            kind = PublicSelectionPageKind::Receipt;
        }
        let items = self
            .current_items(booklet, executor)
            .await?
            .into_iter()
            .filter(|item| item.is_publishable())
            .collect::<Vec<_>>();
        let receipt = if kind == PublicSelectionPageKind::Receipt {
            self.participant_receipt(booklet, session, executor).await?
        } else {
            None
        };
        let mut page = Self::public_page(kind, booklet, &items, Some(session), receipt);
        if booklet.submit_mode.requires_quantity() {
            let prices =
                items.iter().map(|item| (item.base.id.clone(), item.price())).collect::<BTreeMap<_, _>>();
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

    async fn participant_receipt(
        &self,
        booklet: &SalesSelectionBooklet,
        session: &SalesSelectionSession,
        executor: &mut dyn Executor,
    ) -> Result<Option<PublicReceiptView>> {
        let Some(id) = session.proposal_id.as_ref().or(booklet.proposal_id.as_ref()) else {
            return Ok(None);
        };
        let proposal = self
            .db
            .sales_selection_proposals()
            .find_by_id(id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售方案不存在".into()))?;
        if proposal.participant_id != session.participant_id {
            return Err(Error::Forbidden("无权查看该回执".into()));
        }
        let lines = self
            .db
            .sales_selection_proposal_display_lines()
            .list_by_proposal(&proposal.base.id, executor)
            .await?;
        Ok(Some(public_receipt(&proposal, &lines)))
    }

    pub(super) async fn current_items(
        &self,
        booklet: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesSelectionDisplayItem>> {
        self.items_of(&booklet.base.id, booklet.current_batch_id.as_deref().unwrap_or(""), executor).await
    }

    /// 校验个人授权后读取本册快照图片。
    ///
    /// # 参数
    /// `token` 为链接，`asset_id` 为图片，`grant` 为个人授权，`now` 为时间，`executor` 为读取边界。
    ///
    /// # 返回
    /// 返回仅属于当前册的对象键。
    ///
    /// # 错误
    /// 缺少授权、授权失效、结束或图片越权时拒绝。
    pub async fn public_image_key(
        &self,
        token: &str,
        asset_id: &str,
        grant: &SelectionGrant,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let booklet = self.load_by_token(token, executor).await?;
        self.authorized_session(&booklet, grant, now, executor).await?;
        if public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return Err(Error::BusinessLogicError("选品已结束".into()));
        }
        let items = self.current_items(&booklet, executor).await?;
        items
            .iter()
            .filter(|item| item.is_publishable())
            .find_map(|item| image_key_if_authorized(item, asset_id))
            .ok_or_else(|| Error::Forbidden("无权查看该图片".into()))
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
    pub(super) async fn items_of(
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
    pub(super) async fn load_by_token(
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

fn participant_scope(booklet: &SalesSelectionBooklet, session: &SalesSelectionSession) -> String {
    format!("{}:{}:{}", booklet.base.id, session.participant_id, booklet.link_token_version)
}

fn ensure_session_version(session: &SalesSelectionSession, expected: u64) -> Result<()> {
    if session.session_version != expected {
        return Err(Error::selection_conflict("选择已在其他设备更新，请核对后重试"));
    }
    Ok(())
}

fn ensure_budget(
    booklet: &SalesSelectionBooklet,
    session: &SalesSelectionSession,
    items: &[SalesSelectionDisplayItem],
) -> Result<()> {
    if let Some(total) = session.quantity_total(booklet.submit_mode, |id| {
        items.iter().find(|item| item.base.id == id.as_ref()).map(SalesSelectionDisplayItem::price)
    })? {
        booklet.ensure_participant_total(total).map_err(|error| Error::selection_limit(error.to_string()))?;
    }
    Ok(())
}

fn submit_recipient(
    mode: SubmitMode,
    recipient: Option<SelectionRecipient>,
) -> Result<Option<SelectionRecipient>> {
    if mode == SubmitMode::PickupVoucher {
        return SelectionRecipient::new(
            recipient.ok_or_else(|| Error::ValidationError("请填写完整收件地址".into()))?,
        )
        .map(Some)
        .map_err(|error| Error::ValidationError(error.to_string()));
    }
    if recipient.is_some() {
        return Err(Error::ValidationError("普通选品模式不填写个人收件地址".into()));
    }
    Ok(None)
}

fn apply_choices(
    booklet: &SalesSelectionBooklet,
    session: &mut SalesSelectionSession,
    req: SaveSelectionSessionRequest,
    items: &[SalesSelectionDisplayItem],
) -> Result<()> {
    let allowed = items
        .iter()
        .filter(|item| item.is_publishable())
        .map(|item| SalesSelectionDisplayItemId::new(item.base.id.clone()))
        .collect::<Vec<_>>();
    let choices = req
        .choices
        .into_iter()
        .map(|choice| SessionChoice {
            display_item_id: SalesSelectionDisplayItemId::new(choice.item_id),
            quantity: choice.quantity,
        })
        .collect();
    ensure_session_version(session, req.expected_session_version)?;
    session
        .save(req.expected_session_version, choices, booklet.submit_mode, &allowed)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    ensure_budget(booklet, session, items)
}

/// 在读取任何个人会话前投影公开终态。
///
/// # 参数
/// `booklet` 为当前册事实，`now` 为服务端时间。
///
/// # 返回
/// 结束、撤销或到期时返回不含商品、选择及个人信息的结束页，其它状态继续授权读取。
///
/// # 错误
/// 无。
fn ended_public_page(booklet: &SalesSelectionBooklet, now: Instant) -> Option<PublicSelectionPageView> {
    (public_kind(booklet, now) == PublicSelectionPageKind::Ended)
        .then(|| SalesSelectionService::public_page(PublicSelectionPageKind::Ended, booklet, &[], None, None))
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{CustomerAccountId, ProductId, SalesSelectionSessionId, SkuId, SkuRevisionId};

    use super::*;
    use crate::dto::sales_selection::PublicChoiceRequest;
    use crate::entity::sales_selection::{
        BookletStatus, PoolFilterSnapshot, PoolSource, PoolSourceKind, SalesSelectionBookletData,
        SelectionForm, SkuSnapshot,
    };

    fn book() -> SalesSelectionBooklet {
        SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("book"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("customer"),
                customer_no: "C1".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::PickupVoucher,
                access_password_hash: Some("hash".into()),
                per_person_budget: Some("50.00".parse().unwrap()),
                voucher_count: Some(2),
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .unwrap(),
                tiers: Vec::new(),
                created_by: "sales".into(),
            },
        )
        .unwrap()
    }

    fn item() -> SalesSelectionDisplayItem {
        SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("item"),
            SalesSelectionBookletId::new("book"),
            "batch".into(),
            SkuSnapshot {
                sku_id: SkuId::new("sku"),
                sku_revision_id: SkuRevisionId::new("revision"),
                product_id: ProductId::new("product"),
                product_kind: "PHYSICAL".into(),
                category_id: None,
                name: "商品".into(),
                specification_attributes: Vec::new(),
                unit: "件".into(),
                image: None,
                sales_visible_price_gross: "25.00".parse().unwrap(),
            },
        )
        .unwrap()
    }

    fn recipient() -> SelectionRecipient {
        SelectionRecipient {
            name: "张三".into(),
            phone: "13800138000".into(),
            province: "浙江省".into(),
            city: "杭州市".into(),
            district: "西湖区".into(),
            address: "文三路1号".into(),
        }
    }

    #[test]
    fn terminal_read_precedes_grant_lookup_and_is_sanitized() {
        let mut active = book();
        active.status = BookletStatus::Published;
        active.link_expires_at = Some(Instant::from_unix_secs(200));
        assert!(ended_public_page(&active, Instant::from_unix_secs(100)).is_none());
        let mut closed = active.clone();
        closed.status = BookletStatus::Closed;
        let mut revoked = active.clone();
        revoked.link_revoked = true;
        for (book, now) in [
            (closed, Instant::from_unix_secs(100)),
            (revoked, Instant::from_unix_secs(100)),
            (active, Instant::from_unix_secs(200)),
        ] {
            let page = ended_public_page(&book, now).unwrap();
            assert_eq!(page.kind, PublicSelectionPageKind::Ended);
            assert!(page.items.is_empty() && page.choices.is_empty());
            assert!(page.customer_name.is_none() && page.form.is_none() && page.submit_mode.is_none());
            assert!(page.receipt.is_none() && page.recipient.is_none() && page.participant_id.is_none());
            assert!(page.total_amount.is_none() && page.per_person_budget.is_none());
            assert!(page.session_version.is_none() && !page.voucher_required);
        }
    }

    #[test]
    fn saved_choices_use_frozen_prices_and_enforce_personal_budget() {
        let mut session = SalesSelectionSession::new(
            SalesSelectionSessionId::new("session"),
            SalesSelectionBookletId::new("book"),
        );
        session.participant_id = "person".into();
        let request = |version, quantity| SaveSelectionSessionRequest {
            idempotency_key: "key".into(),
            expected_session_version: version,
            choices: vec![PublicChoiceRequest { item_id: "item".into(), quantity: Some(quantity) }],
        };
        let book = book();
        apply_choices(&book, &mut session, request(1, 2), &[item()]).unwrap();
        assert_eq!(session.session_version, 2);
        assert_eq!(
            session.quantity_total(book.submit_mode, |_| Some("25.00".parse().unwrap())).unwrap(),
            Some("50.00".parse().unwrap())
        );
        assert!(matches!(
            apply_choices(&book, &mut session, request(2, 3), &[item()]),
            Err(Error::SelectionLimitExceeded(_))
        ));
        assert!(matches!(
            apply_choices(&book, &mut session, request(1, 1), &[item()]),
            Err(Error::SelectionConflict(_))
        ));
    }

    #[test]
    fn recipient_required_only_for_personal_vouchers() {
        assert!(submit_recipient(SubmitMode::PickupVoucher, Some(recipient())).unwrap().is_some());
        assert!(matches!(submit_recipient(SubmitMode::PickupVoucher, None), Err(Error::ValidationError(_))));
        let mut invalid = recipient();
        invalid.phone = "invalid".into();
        assert!(matches!(
            submit_recipient(SubmitMode::PickupVoucher, Some(invalid)),
            Err(Error::ValidationError(_))
        ));
        assert!(submit_recipient(SubmitMode::ByQuantity, Some(recipient())).is_err());
        assert!(submit_recipient(SubmitMode::MallRedeem, None).unwrap().is_none());
    }

    #[test]
    fn scope_isolates_people_and_rotated_links() {
        let mut book = book();
        let mut session = SalesSelectionSession::new(
            SalesSelectionSessionId::new("session"),
            SalesSelectionBookletId::new("book"),
        );
        session.participant_id = "first".into();
        let first = participant_scope(&book, &session);
        session.participant_id = "second".into();
        assert_ne!(first, participant_scope(&book, &session));
        session.participant_id = "first".into();
        book.link_token_version += 1;
        assert_ne!(first, participant_scope(&book, &session));
    }
}
