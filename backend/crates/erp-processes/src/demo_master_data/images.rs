//! 图片与价格缺项合并为一次正式商品修订；重复执行保留已有值。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_catalog::{ProductMediaInput, UpdateProductRequest};
use erp_core::ids::FileAssetId;
use erp_support::entity::file_asset::content_fingerprint;
use erp_support::{PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass, SensitivityClass};
use id_generator::next_id;
use persistence_core::NoTransaction;
use sha2::{Digest, Sha256};

use super::plan::DemoStep;
use super::record::DemoMasterRecord;
use super::seed::SeedRequest;
use super::{DemoMasterDataProcess, image_assets, prices};
use crate::adapters::catalog_access;
use crate::attachments::product_update_with_assets;
use crate::{Error, Result};

impl DemoMasterDataProcess {
    /// 仅给已登记的演示商品补齐图片及价格缺项；失败可再次生成继续。
    /// # 参数
    /// `actor` 为操作人；`step` 为种子；`records` 为实际 ID 登记。
    /// # 返回
    /// 登记的模板 SKU 补齐价格，启用 SKU 有主图，商品有轮播图；已有值不替换。
    /// # 错误
    /// 归属、图片、存储或商品版本冲突时返回错误，禁止报告生成完成。
    pub(super) async fn ensure_product_defaults(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        records: &HashMap<String, DemoMasterRecord>,
    ) -> Result<()> {
        let record = records.get(&step.key).ok_or_else(|| Error::NotFound("演示商品尚未登记".into()))?;
        let mut request = self.product_update_request(actor, &record.entity_id).await?;
        let SeedRequest::Product(template) = &step.request else {
            return Err(Error::ValidationError("商品补齐必须使用商品种子".into()));
        };
        let prices_changed = prices::fill_missing(&mut request.skus, &template.skus, record);
        let missing = request.skus.iter().any(|sku| {
            needs_image(sku.sku_id.as_ref().map(AsRef::as_ref), sku.main_image_asset_id.is_some(), record)
        });
        if !prices_changed && !missing && !request.carousel_media.is_empty() {
            return Ok(());
        }
        if prices_changed {
            request.change_reason = Some("补齐演示商品价格及展示资料".into());
        }
        let pending = self.fill_images(step, record, &mut request).await?;
        product_update_with_assets(
            self.db.clone(),
            self.rbac.clone(),
            record.entity_id.clone(),
            request,
            pending,
            actor.clone(),
        )
        .await?;
        Ok(())
    }

    async fn fill_images(
        &self,
        step: &DemoStep,
        record: &DemoMasterRecord,
        request: &mut UpdateProductRequest,
    ) -> Result<Vec<PendingFileAssetRequest>> {
        let image = image_assets::for_product(&step.key)?;
        let mut pending = Vec::new();
        for sku in &mut request.skus {
            if needs_image(sku.sku_id.as_ref().map(AsRef::as_ref), sku.main_image_asset_id.is_some(), record)
            {
                sku.main_image_asset_id = Some(self.store_demo_image(image, &step.key, &mut pending).await?);
            }
        }
        if request.carousel_media.is_empty() {
            let file_asset_id = self.store_demo_image(image, &step.key, &mut pending).await?;
            request.carousel_media.push(ProductMediaInput {
                file_asset_id,
                sort_order: 0,
                alt_text: Some(step.request.label().into()),
            });
        }
        Ok(pending)
    }

    async fn product_update_request(
        &self,
        actor: &AuditActor,
        product_id: &str,
    ) -> Result<UpdateProductRequest> {
        let product = catalog_access(self.db.clone(), self.rbac.clone())
            .require_product(actor, "update", product_id, &mut NoTransaction)
            .await?;
        super::repository::images::update_request(&self.db, &product, &mut NoTransaction).await
    }

    async fn store_demo_image(
        &self,
        image: &[u8],
        product_no: &str,
        pending: &mut Vec<PendingFileAssetRequest>,
    ) -> Result<FileAssetId> {
        let token = next_id();
        let object_key = format!("demo-products/{token}.jpg");
        self.storage
            .save_with_content_type(&object_key, image, Some("image/jpeg"))
            .await
            .map_err(|error| Error::Internal(format!("演示商品图片保存失败：{error}")))?;
        let reference = format!("pending-file:{token}");
        pending.push(PendingFileAssetRequest {
            reference: reference.clone(),
            registration: RegisterFileAssetRequest {
                storage_object_key: object_key,
                file_name: format!("{product_no}.jpg"),
                content_type: "image/jpeg".into(),
                byte_size: u64::try_from(image.len()).expect("嵌入图片小于 5 MiB"),
                content_hmac: content_fingerprint(&hex::encode(Sha256::digest(image)), &self.image_secret),
                sensitivity_class: SensitivityClass::General,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
            },
        });
        Ok(FileAssetId::new(reference))
    }
}

fn needs_image(sku_id: Option<&str>, has_image: bool, record: &DemoMasterRecord) -> bool {
    !has_image && sku_id.is_some_and(|id| record.related_ids.iter().any(|known| known == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_registered_missing_images_are_filled() {
        let record = DemoMasterRecord {
            key: "product".into(),
            kind: "product".into(),
            entity_id: "p".into(),
            related_ids: vec!["sku".into()],
            label: "商品".into(),
            removed: false,
        };
        assert!(needs_image(Some("sku"), false, &record));
        assert!(!needs_image(Some("sku"), true, &record));
        assert!(!needs_image(Some("human-added"), false, &record));
        assert!(!needs_image(None, false, &record));
    }
}
