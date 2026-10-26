//! 选品确认仅采用预览时已经核验的图片内容，不以 URL 或新资产编号代替内容版本。

use super::CatalogAdoptionPreview;
use crate::entity::supplier_catalog::{CatalogImage, SupplierCatalogItem};
use crate::{Error, Result};

impl CatalogAdoptionPreview {
    /// 列出当前来源图片尚不能冻结预览的原因，无图片不产生阻断。
    /// # 参数
    /// `item` 为已同步的目录观察，不执行网络读取。
    /// # 返回
    /// 未核验的图片组说明；空集合表示可确认。
    /// # 错误
    /// 不返回错误。
    pub fn media_blockers(item: &SupplierCatalogItem) -> Vec<String> {
        [(&item.images, "主图"), (&item.detail_images, "详情图")]
            .into_iter()
            .filter(|(group, _)| group.as_deref().unwrap_or_default().iter().any(|image| !verified(image)))
            .map(|(_, label)| format!("{label}尚未核验，请先重新同步商品资料后预览"))
            .collect()
    }

    /// 验证本次图片已经具备可展示的冻结内容证明。
    /// # 参数
    /// `item_id` 为本次冻结范围内的来源规格。
    /// # 返回
    /// 来源未提供图片或所有图片均已核验时返回空结果。
    /// # 错误
    /// 未冻结行或图片无可靠内容证明时拒绝。
    pub fn ensure_item_media_verified(&self, item_id: &str) -> Result<()> {
        let item = self
            .items
            .iter()
            .find(|item| item.base.id == item_id)
            .ok_or_else(|| Error::ValidationError("来源规格不属于本次冻结范围".into()))?;
        let blockers = Self::media_blockers(item);
        if !blockers.is_empty() {
            return Err(Error::ConflictError(blockers.join("；")));
        }
        Ok(())
    }

    /// 核对确认时实际读取的图片是否仍为采购在预览中看过的内容。
    /// # 参数
    /// `prepared` 带本次真实下载得到的内容摘要及待提交资产。
    /// # 返回
    /// 按内容和顺序一致时返回空结果；同内容的新归档身份不算变化。
    /// # 错误
    /// 未核验、内容或顺序变化时拒绝，不覆盖公司资料。
    pub fn ensure_prepared_media(&self, prepared: &SupplierCatalogItem) -> Result<()> {
        self.ensure_item_media_verified(&prepared.base.id)?;
        let frozen = self
            .items
            .iter()
            .find(|item| item.base.id == prepared.base.id)
            .ok_or_else(|| Error::ValidationError("来源规格不属于本次冻结范围".into()))?;
        if !same_group(&frozen.images, &prepared.images)
            || !same_group(&frozen.detail_images, &prepared.detail_images)
        {
            return Err(Error::ConflictError(
                "供应商图片内容在预览后发生变化，请重新同步并预览后确认".into(),
            ));
        }
        Ok(())
    }
}

fn verified(image: &CatalogImage) -> bool {
    image
        .content_fingerprint
        .as_deref()
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        && image
            .file_asset_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty() && !id.starts_with("pending-file:"))
}

fn same_group(frozen: &Option<Vec<CatalogImage>>, prepared: &Option<Vec<CatalogImage>>) -> bool {
    let frozen = frozen.as_deref().unwrap_or_default();
    let prepared = prepared.as_deref().unwrap_or_default();
    frozen.len() == prepared.len()
        && frozen.iter().zip(prepared).all(|(before, after)| {
            verified(before) && verified(after) && before.content_fingerprint == after.content_fingerprint
        })
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::offer::{Offer, OfferDetails};

    fn image(hash: char, asset: &str) -> CatalogImage {
        CatalogImage {
            url: "https://images.example/same.png".into(),
            content_fingerprint: Some(hash.to_string().repeat(64)),
            file_asset_id: Some(asset.into()),
        }
    }

    fn preview(images: Option<Vec<CatalogImage>>) -> CatalogAdoptionPreview {
        let item = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(100),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("product".into()), spec_id: "spec".into() },
                    name: "商品".into(),
                    specification: "规格".into(),
                    unit: None,
                    quote: None,
                    details: OfferDetails { images, ..OfferDetails::default() },
                },
            },
        )
        .unwrap();
        CatalogAdoptionPreview {
            base: BaseModel::new("preview".into()),
            connection_id: "connection".into(),
            actor_id: "buyer".into(),
            expires_at: Instant::from_unix_secs(200),
            items: vec![item],
        }
    }

    #[test]
    fn same_url_with_new_bytes_is_rejected_but_same_bytes_with_new_asset_is_accepted() {
        let before = Some(vec![image('a', "old-asset")]);
        assert!(same_group(&before, &Some(vec![image('a', "new-asset")])));
        assert!(!same_group(&before, &Some(vec![image('b', "old-asset")])));
    }

    #[test]
    fn missing_proof_and_reordered_or_deleted_images_are_rejected() {
        let before = Some(vec![image('a', "one"), image('b', "two")]);
        assert!(!same_group(&before, &Some(vec![image('b', "two"), image('a', "one")])));
        assert!(!same_group(&before, &Some(vec![image('a', "one")])));
        let mut unverified = image('a', "one");
        unverified.content_fingerprint = None;
        assert!(!same_group(&Some(vec![image('a', "one")]), &Some(vec![unverified])));
        assert!(!verified(&image('a', "pending-file:one")));
    }

    #[test]
    fn absent_or_explicitly_empty_images_need_no_network_proof() {
        assert!(same_group(&None, &None));
        assert!(same_group(&Some(vec![]), &Some(vec![])));
        assert!(same_group(&None, &Some(vec![])));
    }

    #[test]
    fn confirmation_rejects_changed_unverified_and_outside_source_content() {
        let preview = preview(Some(vec![image('a', "asset-one")]));
        let mut prepared = preview.items[0].clone();
        prepared.images = Some(vec![image('b', "asset-two")]);
        assert!(matches!(preview.ensure_prepared_media(&prepared), Err(Error::ConflictError(_))));
        prepared.images = Some(vec![image('a', "new-archive")]);
        assert!(preview.ensure_prepared_media(&prepared).is_ok());
        prepared.images.as_mut().unwrap()[0].content_fingerprint = None;
        assert!(matches!(preview.ensure_prepared_media(&prepared), Err(Error::ConflictError(_))));
        prepared.base.id = "outside-preview".into();
        assert!(matches!(preview.ensure_prepared_media(&prepared), Err(Error::ValidationError(_))));
    }

    #[test]
    fn preview_blocks_only_unverified_nonempty_groups() {
        for images in [None, Some(vec![]), Some(vec![image('a', "asset")])] {
            let preview = preview(images);
            assert!(CatalogAdoptionPreview::media_blockers(&preview.items[0]).is_empty());
        }
        let mut missing = image('a', "asset");
        missing.content_fingerprint = None;
        let preview = preview(Some(vec![missing]));
        assert_eq!(CatalogAdoptionPreview::media_blockers(&preview.items[0]).len(), 1);
        assert!(matches!(
            preview.ensure_item_media_verified(&preview.items[0].base.id),
            Err(Error::ConflictError(_))
        ));
    }
}
