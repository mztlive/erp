use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dto::supplier_catalog::CatalogFilter;
use crate::dto::supplier_offering::SupplierOfferingTermsWrite;
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::{Error, Result};

/// 一次预览至多冻结的来源规格数量。
pub const MAX_PREVIEW_ROWS: usize = 500;
/// 一次确认至多处理的来源规格数量。
pub const MAX_CONFIRM_ROWS: usize = 50;

/// 明确勾选与当前筛选只能选择一种。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewAdoptionRequest {
    pub item_ids: Option<Vec<String>>,
    pub filter: Option<CatalogFilter>,
}

impl PreviewAdoptionRequest {
    /// 验证选择模式和批次大小，不静默截取超限集合。
    /// # 参数
    /// 无。
    /// # 返回
    /// 规范合法时返回空结果。
    /// # 错误
    /// 两种模式同时出现、空选择、重复或超限时拒绝。
    pub fn validate(&self) -> Result<()> {
        match (&self.item_ids, &self.filter) {
            (Some(ids), None) => {
                let unique: HashSet<_> = ids.iter().collect();
                if ids.is_empty()
                    || ids.len() > MAX_PREVIEW_ROWS
                    || unique.len() != ids.len()
                    || ids.iter().any(|id| id.trim().is_empty())
                {
                    return Err(Error::ValidationError("请选择 1 至 500 个不重复的商品规格".into()));
                }
                Ok(())
            },
            (None, Some(filter)) => filter.validate(),
            _ => Err(Error::ValidationError("勾选商品与当前筛选必须选择且只能选择一种".into())),
        }
    }
}

/// 冻结的来源资料；预览的版本不随分批确认推进。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogAdoptionPreview {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub actor_id: String,
    pub expires_at: Instant,
    pub items: Vec<SupplierCatalogItem>,
}

impl CatalogAdoptionPreview {
    /// 核验当前来源仍对应预览时实际看过的内容和选品状态。
    /// # 参数
    /// `current` 为事务内重读的最新来源。
    /// # 返回
    /// 身份和来源内容均未变化时返回空结果。
    /// # 错误
    /// 行不在冻结范围、来源或正式选品关联变化时拒绝。
    pub fn ensure_current_item(&self, current: &SupplierCatalogItem) -> Result<()> {
        let frozen = self
            .items
            .iter()
            .find(|item| item.base.id == current.base.id)
            .ok_or_else(|| Error::ValidationError("来源规格不属于本次冻结范围".into()))?;
        let media_changed = frozen.images != current.images || frozen.detail_images != current.detail_images;
        let (mut original, mut latest) = (frozen.clone(), current.clone());
        preserve_unseen_media(&mut latest.images, &original.images);
        preserve_unseen_media(&mut latest.detail_images, &original.detail_images);
        original.refresh_fingerprint()?;
        latest.refresh_fingerprint()?;
        let source_changed = original.source_fingerprint != latest.source_fingerprint
            || (!media_changed
                && !frozen.source_fingerprint.is_empty()
                && !current.source_fingerprint.is_empty()
                && frozen.source_fingerprint != current.source_fingerprint);
        if frozen.connection_id != current.connection_id
            || frozen.product_id != current.product_id
            || frozen.spec_id != current.spec_id
            || source_changed
            || frozen.offering_id != current.offering_id
        {
            return Err(Error::ConflictError("供应商商品或选品状态已变化，请重新预览".into()));
        }
        Ok(())
    }

    /// 核验预览归属、冻结版本、有效期及请求行是否属于冻结集合。
    /// # 参数
    /// 连接、操作人、版本、确认行与当前时间由命令入口提供。
    /// # 返回
    /// 全部行合法时返回空结果。
    /// # 错误
    /// 越权、过期、旧版本、重复或超量行时拒绝。
    pub fn ensure_confirmable(
        &self,
        connection: &str,
        actor: &str,
        request: &ConfirmAdoptionRequest,
        now: Instant,
    ) -> Result<()> {
        self.ensure_request(connection, actor, request)?;
        if self.expires_at <= now {
            return Err(Error::ConflictError("选品预览已变化或过期，请重新预览".into()));
        }
        Ok(())
    }

    /// 核验原始选择及归属，允许已提交命令在预览过期后只读恢复结果。
    /// # 参数
    /// 连接、操作人及原始确认请求。
    /// # 返回
    /// 请求属于该冻结预览时返回空结果。
    /// # 错误
    /// 越权、版本不符、重复或越界行时拒绝。
    pub fn ensure_request(
        &self,
        connection: &str,
        actor: &str,
        request: &ConfirmAdoptionRequest,
    ) -> Result<()> {
        if self.connection_id != connection || self.actor_id != actor || self.base.is_deleted() {
            return Err(Error::NotFound("选品预览不存在或无权使用".into()));
        }
        if self.base.version != request.expected_version {
            return Err(Error::ConflictError("选品预览已变化或过期，请重新预览".into()));
        }
        request.validate()?;
        if request.rows.iter().any(|row| !self.items.iter().any(|item| item.base.id == row.item_id)) {
            return Err(Error::ValidationError("确认行不属于本次冻结的选品范围".into()));
        }
        Ok(())
    }
}

/// 尚未展示的图片完成归档时，不让后台处理阻断已冻结的选品。
fn preserve_unseen_media(
    current: &mut Option<Vec<crate::entity::supplier_catalog::CatalogImage>>,
    frozen: &Option<Vec<crate::entity::supplier_catalog::CatalogImage>>,
) {
    if let (Some(current), Some(frozen)) = (current, frozen) {
        for (current, frozen) in current.iter_mut().zip(frozen) {
            if frozen.file_asset_id.is_none()
                && frozen.content_fingerprint.is_none()
                && current.url == frozen.url
            {
                *current = frozen.clone();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptionSpecEntry {
    pub attribute_code: String,
    pub attribute_value_code: String,
}

/// 新建时只接收公司映射；名称和介绍从冻结来源取得。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "UPPERCASE", deny_unknown_fields)]
pub enum AdoptionTarget {
    Create {
        #[serde(default)]
        company_name: Option<String>,
        #[serde(default)]
        company_description: Option<String>,
        category_id: String,
        brand_id: String,
        unit_id: String,
        product_kind: String,
        #[serde(default)]
        spec_entries: Vec<AdoptionSpecEntry>,
        unit_confirmed: bool,
        specification_confirmed: bool,
    },
    Existing {
        sku_id: String,
        expected_sku_version: u64,
        expected_product_version: u64,
        unit_id: String,
        specification_confirmed: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptionRow {
    pub item_id: String,
    pub target: AdoptionTarget,
    pub terms: SupplierOfferingTermsWrite,
    pub delivery_template_id: Option<String>,
}

impl AdoptionRow {
    /// 固定一行原始命令的内容指纹，不包含可重试的操作号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回确定性 SHA256 指纹。
    /// # 错误
    /// 请求无法序列化时返回内部错误。
    pub fn fingerprint(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Internal("选品命令无法编码".into()))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmAdoptionRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
    pub rows: Vec<AdoptionRow>,
}

/// 批次操作号固定全部原始行，防止重试时追加或更换行。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogAdoptionCommand {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub actor_id: String,
    pub fingerprint: String,
}

impl ConfirmAdoptionRequest {
    /// 验证分批确认边界及原操作号，不静默丢弃行。
    /// # 参数
    /// 无。
    /// # 返回
    /// 请求合法时返回空结果。
    /// # 错误
    /// 空行、重复、超量、非法版本或操作号时拒绝。
    pub fn validate(&self) -> Result<()> {
        let ids: HashSet<_> = self.rows.iter().map(|row| &row.item_id).collect();
        if self.expected_version == 0
            || self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 128
            || self.rows.is_empty()
            || self.rows.len() > MAX_CONFIRM_ROWS
            || ids.len() != self.rows.len()
        {
            return Err(Error::ValidationError(
                "每次确认须为 1 至 50 个不重复规格，并提供有效版本和操作号".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptionResult {
    pub item_id: String,
    pub product_id: String,
    pub sku_id: String,
    pub offering_id: String,
}

/// 每个来源规格的永久选品结果；与商品、供给和来源基线同事务。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogAdoptionReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub item_id: String,
    pub actor_id: String,
    pub command_id: String,
    pub fingerprint: String,
    pub result: AdoptionResult,
}

impl CatalogAdoptionReceipt {
    /// 同一来源重复确认恢复原结果，拒绝变更原目标或条款。
    /// # 参数
    /// `fingerprint` 为本次逐行原请求的指纹。
    /// # 返回
    /// 已提交的原结果。
    /// # 错误
    /// 回执损坏、已删除或异载荷重用来源时拒绝。
    pub fn replay(&self, fingerprint: &str) -> Result<AdoptionResult> {
        if self.base.is_deleted() || self.result.item_id != self.item_id {
            return Err(Error::Internal("选品回执无效，禁止重复创建".into()));
        }
        if self.fingerprint != fingerprint {
            return Err(Error::ConflictError("该来源规格已完成选品，不能通过重复选品更换关联或条款".into()));
        }
        Ok(self.result.clone())
    }
}

/// 同一来源 SPU 的后续规格复用已建立的公司商品。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogAdoptionProduct {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub source_product_id: String,
    pub product_id: String,
}

/// 组合来源身份，使用长度分帧避免编号分隔符碰撞。
/// # 参数
/// `parts` 为有顺序的身份组成部分。
/// # 返回
/// 固定长度 SHA256 身份。
/// # 错误
/// 不返回错误。
pub fn adoption_identity(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.len().to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_item() -> SupplierCatalogItem {
        use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
        use crate::ports::connector::offer::{Offer, OfferDetails};
        SupplierCatalogItem::observed(
            "c",
            "s",
            "j",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(10),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("p".into()), spec_id: "s".into() },
                    name: "商品".into(),
                    specification: "规格".into(),
                    unit: None,
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap()
    }

    #[test]
    fn selection_rejects_ambiguous_empty_duplicate_and_oversized_requests() {
        for ids in [vec![], vec!["a".into(), "a".into()], vec!["".into()], vec!["a".into(); 501]] {
            assert!(PreviewAdoptionRequest { item_ids: Some(ids), filter: None }.validate().is_err());
        }
        assert!(PreviewAdoptionRequest { item_ids: None, filter: None }.validate().is_err());
        assert!(PreviewAdoptionRequest { item_ids: Some(vec!["a".into()]), filter: None }.validate().is_ok());
    }

    #[test]
    fn replay_keeps_original_result_and_rejects_changed_payload_or_corruption() {
        let mut receipt = CatalogAdoptionReceipt {
            base: BaseModel::new("receipt".into()),
            connection_id: "connection".into(),
            item_id: "item".into(),
            actor_id: "actor".into(),
            command_id: "command".into(),
            fingerprint: "original".into(),
            result: AdoptionResult {
                item_id: "item".into(),
                product_id: "product".into(),
                sku_id: "sku".into(),
                offering_id: "offer".into(),
            },
        };
        assert_eq!(receipt.replay("original").unwrap().offering_id, "offer");
        assert!(receipt.replay("changed").is_err());
        receipt.result.item_id = "other".into();
        assert!(receipt.replay("original").is_err());
        assert_ne!(adoption_identity(&["a|b", "c"]), adoption_identity(&["a", "b|c"]));
    }

    #[test]
    fn preview_checks_content_changes_independently_from_observation_time() {
        let item = source_item();
        let preview = CatalogAdoptionPreview {
            base: BaseModel::new("preview".into()),
            connection_id: "c".into(),
            actor_id: "actor".into(),
            expires_at: Instant::from_unix_secs(200),
            items: vec![item.clone()],
        };
        let mut current = item.clone();
        current.observed_at = Instant::from_unix_secs(20);
        assert!(preview.ensure_current_item(&current).is_ok());
        current.source_fingerprint = "new-content".into();
        assert!(preview.ensure_current_item(&current).is_err());
        current = item;
        current.offering_id = Some("new-adoption".into());
        assert!(preview.ensure_current_item(&current).is_err());
    }

    #[test]
    fn background_archive_of_unseen_image_does_not_invalidate_selection() {
        use crate::ports::connector::offer::CatalogImage;
        let mut item = source_item();
        item.images = Some(vec![CatalogImage {
            url: "https://example.com/a".into(),
            content_fingerprint: None,
            file_asset_id: None,
        }]);
        item.refresh_fingerprint().unwrap();
        let preview = CatalogAdoptionPreview {
            base: BaseModel::new("preview".into()),
            connection_id: "c".into(),
            actor_id: "actor".into(),
            expires_at: Instant::from_unix_secs(200),
            items: vec![item.clone()],
        };
        let mut current = item;
        let image = &mut current.images.as_mut().unwrap()[0];
        image.content_fingerprint = Some("a".repeat(64));
        image.file_asset_id = Some("asset".into());
        current.refresh_fingerprint().unwrap();
        assert!(preview.ensure_current_item(&current).is_ok());
        current.images.as_mut().unwrap()[0].url = "https://example.com/b".into();
        current.refresh_fingerprint().unwrap();
        assert!(preview.ensure_current_item(&current).is_err());
    }

    #[test]
    fn preview_validates_owner_expiry_version_and_membership_before_writes() {
        let item = source_item();
        let preview = CatalogAdoptionPreview {
            base: BaseModel::new("preview".into()),
            connection_id: "c".into(),
            actor_id: "actor".into(),
            expires_at: Instant::from_unix_secs(200),
            items: vec![item.clone()],
        };
        let row = AdoptionRow {
            item_id: item.base.id,
            target: AdoptionTarget::Existing {
                sku_id: "sku".into(),
                expected_sku_version: 1,
                expected_product_version: 1,
                unit_id: "unit".into(),
                specification_confirmed: true,
            },
            terms: SupplierOfferingTermsWrite::new(
                "10".into(),
                "9".into(),
                "0.13".into(),
                "10".into(),
                "2026-10-26".into(),
            ),
            delivery_template_id: None,
        };
        let mut request = ConfirmAdoptionRequest {
            expected_version: preview.base.version,
            idempotency_key: "key".into(),
            rows: vec![row],
        };
        assert!(preview.ensure_confirmable("c", "actor", &request, Instant::from_unix_secs(100)).is_ok());
        assert!(preview.ensure_confirmable("c", "actor", &request, Instant::from_unix_secs(200)).is_err());
        assert!(preview.ensure_request("c", "actor", &request).is_ok());
        assert!(preview.ensure_request("c", "other", &request).is_err());
        assert!(preview.ensure_request("other", "actor", &request).is_err());
        request.expected_version += 1;
        assert!(preview.ensure_request("c", "actor", &request).is_err());
        request.expected_version = preview.base.version;
        request.rows[0].item_id = "other-source".into();
        assert!(preview.ensure_request("c", "actor", &request).is_err());
    }
}
