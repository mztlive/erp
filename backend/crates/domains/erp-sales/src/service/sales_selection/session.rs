//! 公开会话按授权凭证保存、提交和回执。

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
    SalesSelectionProposalData, SalesSelectionSession, SelectionGrant, SessionChoice, build_proposal_lines,
    normalize_idempotency_key, request_hash, token_hash,
};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 客户打开公开链接时按当前令牌状态及访问凭证决定可交付页面，未获授权不读取选择。
    /// 结果：终态返回已结束页；其它未授权访问返回锁定页，两者均不含敏感信息。
    /// 链接不存在或数据库读取失败时拒绝。
    ///
    /// 输入约定：`token` 为链接令牌，`grant` 为解码授权，`now` 为服务端时间，`executor` 为读取边界。
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
        self.session_page(&booklet, &session, now, executor).await
    }

    /// 客户在链接有效且授权通过时保存整份选择集，并按会话版本拒绝并发覆盖。
    /// 返回最新公开页。
    /// 授权失效、版本冲突或非法选择时原子拒绝。
    ///
    /// 输入约定：`token`、`grant` 为访问资格，`req` 为带版本和幂等键的选择，`now` 为当前时间，`executor` 必须为事务。
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

        let scope = session_scope(&booklet);
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
            return self.session_page(&booklet, &session, now, executor).await;
        }

        if session.frozen || public_kind(&booklet, now) != PublicSelectionPageKind::Selecting {
            return self.session_page(&booklet, &session, now, executor).await;
        }
        booklet.ensure_public_write(&token_hash(token), now)?;
        let items = self.current_items(&booklet, executor).await?;
        apply_choices(&mut session, req, &items)?;

        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;
        self.db.sales_selection_sessions().update(&mut session, executor).await?;

        let page = self.session_page(&booklet, &session, now, executor).await?;
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

    /// 客户提交时冻结选择，在同一事务写入销售方案并结束选品；重试不生成第二份。
    /// 成功提交或有效重放返回只读回执；链接已终止时返回结束页。
    /// 授权、版本或任何写入失败时整次拒绝。
    ///
    /// 输入约定：`token`、`grant` 为访问资格，`req` 为最终确认，`now` 为当前时间，`executor` 必须为事务。
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

        let scope = session_scope(&booklet);
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
            return self.session_page(&booklet, &session, now, executor).await;
        }

        if session.frozen || public_kind(&booklet, now) == PublicSelectionPageKind::Ended {
            return self.session_page(&booklet, &session, now, executor).await;
        }
        booklet.ensure_public_write(&token_hash(token), now)?;
        ensure_session_version(&session, req.expected_session_version)?;
        session
            .freeze_for_submit(req.expected_session_version)
            .map_err(|error| Error::ValidationError(error.to_string()))?;

        let items = self.current_items(&booklet, executor).await?;
        let proposal_id = self.create_proposal(&booklet, &session, &items, now, executor).await?;
        booklet.mark_submitted(proposal_id, now)?;
        self.db.sales_selection_sessions().update(&mut session, executor).await?;
        self.db.sales_selection_booklets().update(&mut booklet, executor).await?;

        let page = self.session_page(&booklet, &session, now, executor).await?;
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

    async fn create_proposal(
        &self,
        booklet: &SalesSelectionBooklet,
        session: &SalesSelectionSession,
        items: &[SalesSelectionDisplayItem],
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposalId> {
        let proposal_id = SalesSelectionProposalId::new(id_generator::next_id());
        let (display_lines, sku_lines, total) =
            build_proposal_lines(&proposal_id, &session.choices, items, id_generator::next_id)?;
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
                total_amount: Some(total),
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

    pub(super) async fn session_page(
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
            self.session_receipt(booklet, executor).await?
        } else {
            None
        };

        let mut page = Self::public_page(kind, booklet, &items, Some(session), receipt);
        let prices =
            items.iter().map(|item| (item.base.id.clone(), item.price())).collect::<BTreeMap<_, _>>();
        for choice in &mut page.choices {
            if let (Some(price), Some(quantity)) = (prices.get(&choice.item_id), choice.quantity) {
                choice.line_amount = Some(crate::entity::sales_selection::try_mul_u32(*price, quantity)?);
            }
        }
        page.total_amount = Some(session.quantity_total(|id| prices.get(id.as_ref()).copied())?);
        Ok(page)
    }

    async fn session_receipt(
        &self,
        booklet: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<Option<PublicReceiptView>> {
        let Some(id) = booklet.proposal_id.as_ref() else {
            return Ok(None);
        };
        let proposal = self
            .db
            .sales_selection_proposals()
            .find_by_id(id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售方案不存在".into()))?;

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

    /// 服务公开图片之前重验令牌与访问资格，只解析本册快照实际引用的对象键。
    /// 返回仅属于当前册的对象键。
    /// 缺少授权、授权失效、结束或图片越权时拒绝。
    ///
    /// 输入约定：`token` 为链接，`asset_id` 为图片，`grant` 为访问授权，`now` 为时间，`executor` 为读取边界。
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

    /// 为已校验客户归属的管理端历史图片请求解析对象键，要求该资产由本册某个历史陈列实际引用。
    ///
    /// 组合层已在调用方事务内完成对象重验；本方法只映射对象键。
    /// 返回快照对象键。
    /// 任意文件身份、其他册资产或不存在的册均拒绝。
    ///
    /// 输入约定：
    /// - `booklet_id`：选品册
    /// - `asset_id`：文件资产
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

    // 加载选品册指定有效批次的陈列供公开页或提交核对使用，沿用调用方的执行器。
    // 返回有效且未删除的陈列项。
    // 查询失败时返回仓储错误。
    //
    // 输入约定：
    // - `booklet_id`：选品册
    // - `batch_id`：准备批次
    pub(super) async fn items_of(
        &self,
        booklet_id: &str,
        batch_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesSelectionDisplayItem>> {
        let domain = crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db);
        domain.list_effective_items(booklet_id, batch_id, executor).await.map_err(Error::from)
    }

    // 公开请求收件时先对令牌取哈希并定位册，不以明文值查询数据库。
    // 返回选品册。
    // 失败条件：令牌无效。
    //
    // 输入约定：
    // - `token`：明文令牌
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

// 解析图片对象键之前核对资产确实出现在目标陈列的冻结图片引用中。
// 结果：授权时返回对象键。
//
// 输入约定：
// - `item`：陈列
// - `asset_id`：请求的资产
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

fn session_scope(booklet: &SalesSelectionBooklet) -> String {
    format!("{}:{}", booklet.base.id, booklet.link_token_version)
}

fn ensure_session_version(session: &SalesSelectionSession, expected: u64) -> Result<()> {
    if session.session_version != expected {
        return Err(Error::selection_conflict("选择已在其他设备更新，请核对后重试"));
    }
    Ok(())
}

fn apply_choices(
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
        .save(req.expected_session_version, choices, &allowed)
        .map_err(|error| Error::ValidationError(error.to_string()))
}

// 链接终止时先投影结束态，避免为了显示空页面而加载会话。
// 结果：结束、撤销或到期时返回不含商品和选择的结束页，其它状态继续授权读取。
//
// 输入约定：`booklet` 为当前册事实，`now` 为服务端时间。
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
        SelectionForm, SkuSnapshot, SubmitMode,
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
                submit_mode: SubmitMode::ByQuantity,
                access_password_hash: Some("hash".into()),
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
                reference_prices: None,
                sales_visible_price_gross: "25.00".parse().unwrap(),
            },
        )
        .unwrap()
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
            assert!(page.receipt.is_none() && page.total_amount.is_none() && page.session_version.is_none());
        }
    }

    #[test]
    fn saved_choices_use_frozen_prices_and_enforce_version() {
        let mut session = SalesSelectionSession::new(
            SalesSelectionSessionId::new("session"),
            SalesSelectionBookletId::new("book"),
        );
        let request = |version, quantity| SaveSelectionSessionRequest {
            idempotency_key: "key".into(),
            expected_session_version: version,
            choices: vec![PublicChoiceRequest { item_id: "item".into(), quantity }],
        };

        apply_choices(&mut session, request(1, Some(2)), &[item()]).unwrap();
        assert_eq!(session.session_version, 2);
        assert_eq!(
            session.quantity_total(|_| Some("25.00".parse().unwrap())).unwrap(),
            "50.00".parse().unwrap()
        );

        assert!(matches!(
            apply_choices(&mut session, request(2, None), &[item()]),
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            apply_choices(&mut session, request(1, Some(1)), &[item()]),
            Err(Error::SelectionConflict(_))
        ));
    }

    #[test]
    fn scope_isolates_rotated_links() {
        let mut book = book();
        let first = session_scope(&book);
        book.link_token_version += 1;
        assert_ne!(first, session_scope(&book));
    }
}
