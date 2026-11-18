//! 复制选品册：保留配置与冻结陈列，重置客户交互和生命周期。

use entity_core::BaseModel;
use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId, SalesSelectionDisplayItemId};
use erp_core::{Error, Result};

use super::{BookletStatus, SalesSelectionBooklet, SalesSelectionBookletData, SalesSelectionDisplayItem};

impl SalesSelectionBooklet {
    /// 为重新选择的客户复制配置和准备批次，创建独立的草稿或待发布册。
    /// 返回无链接、会话或方案关联的新册；责任归属与原配置保持一致。
    /// 原册准备中、身份复用或新客户字段非法时拒绝。
    ///
    /// 输入约定：
    /// - `id`：新册身份，同时派生新批次身份
    /// - `customer_id`：新客户身份
    /// - `customer_no`：新客户编号
    /// - `customer_name`：新客户名称
    /// - `actor_id`：本次复制操作人
    pub fn copied(
        &self,
        id: SalesSelectionBookletId,
        customer_id: CustomerAccountId,
        customer_no: String,
        customer_name: String,
        actor_id: &str,
    ) -> Result<Self> {
        if self.status == BookletStatus::Preparing || self.active_task_id.is_some() {
            return Err(Error::from("选品册正在准备，请准备完成后再复制"));
        }
        if id.as_ref() == self.base.id {
            return Err(Error::from("复制选品册必须使用新的身份"));
        }

        let mut copied = Self::new(
            id,
            SalesSelectionBookletData {
                customer_id,
                customer_no,
                customer_name,
                sales_owner_user_id: self.sales_owner_user_id.clone(),
                business_org_unit_id: self.business_org_unit_id.clone(),
                form: self.form,
                submit_mode: self.submit_mode,
                access_password_hash: self.access_password_hash.clone(),
                per_person_budget: self.per_person_budget,
                voucher_count: self.voucher_count,
                pool_source: self.pool_source.clone(),
                tiers: self.tiers.clone(),
                created_by: actor_id.to_owned(),
            },
        )?;

        if self.current_batch_id.is_some() {
            copied.status = BookletStatus::PendingPublish;
            copied.current_batch_id = Some(format!("copy-{}", copied.base.id));
            copied.eligibility_as_of = self.eligibility_as_of;
            copied.prepared_at = self.prepared_at;
        }
        Ok(copied)
    }
}

impl SalesSelectionDisplayItem {
    /// 将陈列快照复制到新册新批次，保留商品、组合、价格、图片及删减标记。
    /// 返回与原项无可变关联的独立陈列。
    ///
    /// 输入约定：
    /// - `id`：新陈列身份
    /// - `booklet_id`：新册身份
    /// - `batch_id`：新批次
    pub fn copied(
        &self,
        id: SalesSelectionDisplayItemId,
        booklet_id: SalesSelectionBookletId,
        batch_id: String,
    ) -> Self {
        Self {
            base: BaseModel::new(id.to_string()),
            booklet_id,
            batch_id,
            effective: self.effective,
            removed: self.removed,
            public_item_id: id.to_string(),
            kind: self.kind.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{ProductId, SalesSelectionProposalId, SkuId, SkuRevisionId};

    use super::*;
    use crate::entity::sales_selection::{
        DisplayKind, ImageAssetSnapshot, PackageCoverRef, PoolFilterSnapshot, PoolSource, PoolSourceKind,
        SelectionForm, SelectionReferencePrices, SkuSnapshot, SubmitMode, TierRule,
    };

    fn source() -> SalesSelectionBooklet {
        let mut book = SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("source"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("old-customer"),
                customer_no: "C001".into(),
                customer_name: "原客户".into(),
                sales_owner_user_id: "sales-owner".into(),
                business_org_unit_id: "sales-org".into(),
                form: SelectionForm::Package,
                submit_mode: SubmitMode::PickupVoucher,
                access_password_hash: Some("existing-slow-password-hash".into()),
                per_person_budget: Some("100.00".parse().unwrap()),
                voucher_count: Some(20),
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot { q: Some("保温杯".into()), ..Default::default() }),
                    None,
                )
                .unwrap(),
                tiers: vec![TierRule {
                    tier_id: "tier-original".into(),
                    name: "百元档".into(),
                    target_amount: "100.00".parse().unwrap(),
                    tolerance: "10.00".parse().unwrap(),
                    expected_count: 3,
                    sku_count: 2,
                }],
                created_by: "original-creator".into(),
            },
        )
        .unwrap();

        book.base = BaseModel::fake();
        book.base.id = "source".into();
        book.status = BookletStatus::Submitted;
        book.current_batch_id = Some("old-batch".into());
        book.eligibility_as_of = Some(BusinessDate::today());
        book.prepared_at = Some(Instant::now());

        book.link_token_hash = Some("old-token-hash".into());
        book.link_token_ciphertext = Some("old-token-cipher".into());
        book.link_token_version = 7;
        book.link_expires_at = Some(Instant::now());
        book.link_revoked = true;
        book.proposal_id = Some(SalesSelectionProposalId::new("old-proposal"));
        book.published_by = Some("publisher".into());
        book.published_at = Some(Instant::now());
        book.submitted_at = Some(Instant::now());
        book.closed_at = Some(Instant::now());
        book.voided_at = Some(Instant::now());
        book.last_prepare_failure = Some("old-failure".into());
        book
    }

    fn copy(book: &SalesSelectionBooklet) -> Result<SalesSelectionBooklet> {
        book.copied(
            SalesSelectionBookletId::new("target"),
            CustomerAccountId::new("new-customer"),
            "C002".into(),
            "新客户".into(),
            "copy-operator",
        )
    }

    #[test]
    fn copy_preserves_settings_and_resets_customer_activity_in_every_stable_state() {
        for status in [
            BookletStatus::PendingPublish,
            BookletStatus::Published,
            BookletStatus::Submitted,
            BookletStatus::Closed,
            BookletStatus::Voided,
        ] {
            let mut original = source();
            original.status = status;
            let before = serde_json::to_value(&original).unwrap();

            let copied = copy(&original).unwrap();

            assert_eq!(copied.customer_id.as_ref(), "new-customer");
            assert_eq!(copied.customer_no, "C002");
            assert_eq!(copied.customer_name, "新客户");
            assert_eq!(copied.sales_owner_user_id, original.sales_owner_user_id);
            assert_eq!(copied.business_org_unit_id, original.business_org_unit_id);
            assert_eq!(copied.form, original.form);
            assert_eq!(copied.submit_mode, original.submit_mode);
            assert_eq!(copied.access_password_hash, original.access_password_hash);
            assert_eq!(copied.per_person_budget, original.per_person_budget);
            assert_eq!(copied.voucher_count, original.voucher_count);
            assert_eq!(copied.pool_source, original.pool_source);
            assert_eq!(copied.tiers, original.tiers);
            assert_eq!(copied.eligibility_as_of, original.eligibility_as_of);
            assert_eq!(copied.prepared_at, original.prepared_at);
            assert_eq!(copied.status, BookletStatus::PendingPublish);
            assert_eq!(copied.current_batch_id.as_deref(), Some("copy-target"));
            assert_eq!(copied.created_by, "copy-operator");
            assert_eq!(copied.updated_by, "copy-operator");

            assert!(copied.link_token_hash.is_none());
            assert!(copied.link_token_ciphertext.is_none());
            assert_eq!(copied.link_token_version, 0);
            assert!(copied.link_expires_at.is_none());
            assert!(!copied.link_revoked);
            assert!(copied.proposal_id.is_none());
            assert!(copied.published_by.is_none());
            assert!(copied.published_at.is_none());
            assert!(copied.submitted_at.is_none());
            assert!(copied.closed_at.is_none());
            assert!(copied.voided_at.is_none());
            assert!(copied.active_task_id.is_none());
            assert!(copied.pre_prepare_status.is_none());
            assert!(copied.last_prepare_failure.is_none());

            assert_eq!(serde_json::to_value(&original).unwrap(), before);
        }
    }

    #[test]
    fn copy_draft_preserves_selected_source_without_starting_preparation() {
        let mut original = source();
        original.form = SelectionForm::SingleSku;
        original.submit_mode = SubmitMode::ByQuantity;
        original.per_person_budget = None;
        original.voucher_count = None;
        original.tiers.clear();
        original.status = BookletStatus::Draft;
        original.current_batch_id = None;
        original.pool_source = PoolSource::new(
            PoolSourceKind::Selection,
            None,
            Some(vec![SkuId::new("sku-2"), SkuId::new("sku-1")]),
        )
        .unwrap();

        let copied = copy(&original).unwrap();
        assert_eq!(copied.status, BookletStatus::Draft);
        assert!(copied.current_batch_id.is_none());
        assert!(copied.prepared_at.is_none());
        assert_eq!(copied.pool_source, original.pool_source);
        assert!(copied.tiers.is_empty());
    }

    #[test]
    fn copy_rejects_changing_source_invalid_customer_and_reused_identity() {
        let mut original = source();
        original.status = BookletStatus::Preparing;
        assert!(copy(&original).unwrap_err().to_string().contains("正在准备"));
        original.status = BookletStatus::PendingPublish;
        original.active_task_id = Some("active-task".into());
        assert!(copy(&original).is_err());

        original.active_task_id = None;
        assert!(
            original
                .copied(
                    SalesSelectionBookletId::new("source"),
                    CustomerAccountId::new("customer"),
                    "C002".into(),
                    "客户".into(),
                    "operator",
                )
                .is_err()
        );

        assert!(
            original
                .copied(
                    SalesSelectionBookletId::new("target"),
                    CustomerAccountId::new("customer"),
                    "C002".into(),
                    " ".into(),
                    "operator",
                )
                .is_err()
        );
    }

    fn sku(id: &str) -> SkuSnapshot {
        SkuSnapshot {
            sku_id: SkuId::new(id),
            sku_revision_id: SkuRevisionId::new(format!("revision-{id}")),
            product_id: ProductId::new(format!("product-{id}")),
            product_kind: "physical".into(),
            category_id: None,
            name: "保温杯".into(),
            specification_attributes: vec![],
            unit: "个".into(),
            image: Some(ImageAssetSnapshot {
                file_asset_id: "asset".into(),
                content_checksum: "checksum".into(),
                storage_object_key: "frozen/image.jpg".into(),
            }),
            reference_prices: Some(SelectionReferencePrices {
                dropship_price_gross: Some("49.99".parse().unwrap()),
                bulk_price_gross: Some("40".parse().unwrap()),
                market_price: None,
            }),
            sales_visible_price_gross: "49.99".parse().unwrap(),
        }
    }

    #[test]
    fn copy_retains_exact_single_and_package_snapshots_and_removed_marks() {
        let single = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("source"),
            "old-batch".into(),
            sku("sku-1"),
        )
        .unwrap();
        let mut package = SalesSelectionDisplayItem::package(
            SalesSelectionDisplayItemId::new("package"),
            SalesSelectionBookletId::new("source"),
            "old-batch".into(),
            "tier-original".into(),
            vec![sku("sku-1"), sku("sku-2")],
            PackageCoverRef {
                file_asset_id: "cover".into(),
                content_checksum: "cover-checksum".into(),
                storage_object_key: "frozen/cover.jpg".into(),
                generator_version: "v1".into(),
            },
        )
        .unwrap();
        package.remove().unwrap();

        for original in [single, package] {
            let mut copied = original.copied(
                SalesSelectionDisplayItemId::new(format!("copy-{}", original.base.id)),
                SalesSelectionBookletId::new("target"),
                "new-batch".into(),
            );
            assert_eq!(copied.kind, original.kind);
            assert_eq!(copied.price(), original.price());
            assert_eq!(copied.cover_asset_id(), original.cover_asset_id());
            assert_eq!(copied.removed, original.removed);
            assert_eq!(copied.booklet_id.as_ref(), "target");
            assert_eq!(copied.batch_id, "new-batch");
            assert_ne!(copied.public_item_id, original.public_item_id);

            if let DisplayKind::SingleSku { sku } = &mut copied.kind {
                sku.name = "副本商品".into();
            }
            assert_eq!(original.sellable_refs()[0].1, "revision-sku-1");
        }
    }
}
