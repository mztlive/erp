//! 商品来源资料应用：所有业务写入使用调用方同一执行器。
use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::ids::{
    FileAssetId, ProductId, ProductRevisionId, ProductRevisionMediaId, SkuId, SkuRevisionId,
};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::CatalogService;
use crate::entity::catalog::product_revision::ProductRevisionData;
use crate::entity::catalog::product_revision_media::{
    MediaRole, ProductRevisionMedia, ProductRevisionMediaData,
};
use crate::entity::catalog::{ProductRevision, Sku, SkuRevision, next_revision_no};
use crate::repository::CatalogExt;
use crate::source_sync::command::{self, ResolutionFacts, ResolutionStore};
use crate::source_sync::repository::ProductSourceExt;
use crate::source_sync::*;
use crate::{Error, Result};

impl CatalogService {
    /// 读取经商品详情权限授权的待确认资料。
    /// # 参数
    /// 路径商品和当前操作人。
    /// # 返回
    /// 无资料来源时为 None，否则返回真实差异及版本。
    /// # 错误
    /// 越范围、商品不存在或读取失败时拒绝。
    pub async fn product_source_diff(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Option<ProductSourceDiffView>> {
        let product = self.access().require_product(actor, "detail", id, &mut NoTransaction).await?;
        let Some(source) = self.db.product_sources().find_by_id(id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        let current = self.source_company_content(id, &mut NoTransaction).await?;
        let mut view = source.view(product.base.version);
        view.sku_versions = current.sku_versions;
        for field in &mut view.fields {
            field.sku_no = field.sku_id.as_ref().and_then(|id| current.sku_numbers.get(id)).cloned();
        }
        for field in &mut view.fields {
            if let Some((_, _, value)) =
                current.values.iter().find(|(f, s, _)| *f == field.field && *s == field.sku_id)
            {
                field.company = value.clone();
            }
        }
        Ok(Some(view))
    }

    /// 确认用户实际比较过的来源和公司版本，回执与商品修订原子保存。
    /// # 参数
    /// 路径商品、完整版本与字段决策、当前操作人。
    /// # 返回
    /// 确认后的最新差异；重放不再次写入。
    /// # 错误
    /// 越权、异载荷、旧版本和非法图片等返回对应错误。
    pub async fn resolve_product_source_diff(
        &self,
        id: &str,
        req: ResolveSourceDiffRequest,
        actor: &AuditActor,
    ) -> Result<Option<ProductSourceDiffView>> {
        if req.idempotency_key.trim().is_empty() || req.idempotency_key.len() > 128 {
            return Err(Error::ValidationError("差异确认操作号无效".into()));
        }
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.resolve_source_write(&id, req, &actor, executor).await })
            })
            .await?;
        Ok(Some(result))
    }

    async fn resolve_source_write(
        &self,
        id: &str,
        req: ResolveSourceDiffRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductSourceDiffView> {
        let mut store = MongoResolutionStore { service: self, actor, id, executor, product_version: None };
        command::resolve(&mut store, id, req).await
    }

    /// 读取公司当前商品与SKU主图，避免缓存代替真实编辑事实。
    /// # 参数
    /// 商品身份及调用方执行器。
    /// # 返回
    /// 公司展示内容和SKU版本。
    /// # 错误
    /// 商品或当前修订缺失、数据库错误时拒绝。
    pub async fn source_company_content(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompanySourceContent> {
        let snapshot = self
            .db
            .catalog()
            .source_product_snapshot(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".into()))?;
        let revision = snapshot
            .current_revision
            .ok_or_else(|| Error::BusinessLogicError("商品当前修订不存在".into()))?;
        let mut result = CompanySourceContent {
            values: vec![
                (SourceField::Name, None, SourceValue::Text(Some(revision.name))),
                (SourceField::Description, None, SourceValue::Text(revision.description)),
            ],
            sku_versions: Default::default(),
            sku_numbers: Default::default(),
        };
        result.values.extend(current_media_values(&snapshot.media));
        self.append_source_sku_content(id, &mut result, executor).await?;
        Ok(result)
    }

    async fn append_source_sku_content(
        &self,
        id: &str,
        result: &mut CompanySourceContent,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let skus = self.db.catalog().skus_for_product(&ProductId::new(id), executor).await?;
        for sku in skus {
            result.sku_versions.insert(sku.base.id.clone(), sku.base.version);
            result.sku_numbers.insert(sku.base.id.clone(), sku.sku_no.clone());
            let Some(revision_id) = sku.stable.current_revision_id else { continue };
            let revision = self
                .db
                .sku_revisions()
                .find_by_id(&revision_id, executor)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("SKU当前修订不存在".into()))?;
            result.values.push((
                SourceField::SkuName,
                Some(sku.base.id.clone()),
                SourceValue::Text(Some(revision.name.clone())),
            ));
            let images = revision
                .source_main_image_asset_id
                .into_iter()
                .map(|id| SourceImage {
                    url: None,
                    file_asset_id: Some(id.to_string()),
                    content_fingerprint: None,
                })
                .collect();
            result.values.push((SourceField::MainImage, Some(sku.base.id), SourceValue::Images(images)));
        }
        Ok(())
    }

    /// 在真实商品编辑事务内登记人工维护责任。
    /// # 参数
    /// 已更新商品身份及当前事务执行器。
    /// # 返回
    /// 无来源商品保持原行为；有来源时保存字段责任。
    /// # 错误
    /// 读取或乐观锁写入失败时拒绝并回滚编辑。
    pub async fn record_source_company_edit(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        let Some(mut source) = self.db.product_sources().find_by_id(id, executor).await? else {
            return Ok(());
        };
        let current = self.source_company_content(id, executor).await?;
        let previous = source.fields.clone();
        source.record_manual_edit(&current.values);
        if source.fields == previous {
            return Ok(());
        }
        self.db.product_sources().update(&mut source, executor).await?;
        Ok(())
    }

    /// 只追加展示资料修订，保留价格、规格、公司分类和主动停用状态。
    /// # 参数
    /// 商品、字段变更、审计身份及调用方事务。
    /// # 返回
    /// 保存后的公司商品版本。
    /// # 错误
    /// 素材不存在、版本冲突或不完整商品修订时拒绝。
    pub async fn write_source_content(
        &self,
        id: &str,
        changes: &[(SourceField, Option<String>, SourceValue)],
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let snapshot = self
            .db
            .catalog()
            .source_product_snapshot(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".into()))?;
        let mut product = snapshot.product;
        if changes.is_empty() {
            return Ok(product.base.version);
        }
        let old = snapshot
            .current_revision
            .ok_or_else(|| Error::BusinessLogicError("商品当前修订不存在".into()))?;
        let mut revision = source_successor(id, old, snapshot.latest_revision_no, changes)?;
        revision.status = product.stable.status;
        let revision_id = ProductRevisionId::new(revision.base.id.clone());
        let mut media = copy_source_media(snapshot.media, &revision_id)?;
        for (field, _, value) in changes {
            value.validate(*field)?;
            match (field, value) {
                (SourceField::Name | SourceField::Description, SourceValue::Text(_)) => {},
                (SourceField::MainImage | SourceField::SkuName, _) => {},
                (SourceField::CarouselMedia | SourceField::DetailMedia, SourceValue::Images(images)) => {
                    self.replace_source_media(&revision_id, *field, images, &mut media, executor).await?;
                },
                _ => return Err(Error::ValidationError("商品资料类型无效".into())),
            }
        }
        for sku in source_sku_ids(changes)? {
            self.write_source_sku_changes(id, &sku, changes, actor, executor).await?;
        }
        product.attach_revision(&revision, actor)?;
        self.db.catalog().create_product_revision_with_media(&revision, &media, executor).await?;
        self.db.products().update(&mut product, executor).await?;
        Ok(product.base.version)
    }

    async fn replace_source_media(
        &self,
        revision: &ProductRevisionId,
        field: SourceField,
        images: &[SourceImage],
        media: &mut Vec<ProductRevisionMedia>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let role = if field == SourceField::CarouselMedia { MediaRole::Carousel } else { MediaRole::Detail };
        media.retain(|m| m.media_role != role);
        for (order, image) in images.iter().enumerate() {
            let asset = self.source_image_asset(image, executor).await?;
            media.push(ProductRevisionMedia::new(
                ProductRevisionMediaId::new(next_id()),
                ProductRevisionMediaData {
                    product_revision_id: revision.clone(),
                    file_asset_id: asset,
                    media_role: role,
                    sort_order: i32::try_from(order)
                        .map_err(|_| Error::ValidationError("图片数量过多".into()))?,
                    alt_text: None,
                },
            )?);
        }
        Ok(())
    }

    async fn source_image_asset(
        &self,
        image: &SourceImage,
        executor: &mut dyn Executor,
    ) -> Result<FileAssetId> {
        let id = image
            .file_asset_id
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::BusinessLogicError("供应商图片尚未归档".into()))?;
        let asset = FileAssetId::new(id);
        let fact = self
            .file_assets
            .find_by_id(&asset, executor)
            .await?
            .ok_or_else(|| Error::NotFound("图片文件资产不存在".into()))?;
        if !fact.usable_image {
            return Err(Error::BusinessLogicError("图片文件未通过内容核验或已不可用".into()));
        }
        Ok(asset)
    }

    async fn current_source_sku(
        &self,
        product: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Sku, SkuRevision)> {
        let sku = self
            .db
            .skus()
            .find_by_id(id, executor)
            .await?
            .filter(|s| s.product_id.as_ref() == product)
            .ok_or_else(|| Error::NotFound("商品规格不存在".into()))?;
        let current = sku
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("SKU当前修订缺失".into()))?;
        let old = self
            .db
            .sku_revisions()
            .find_by_id(current, executor)
            .await?
            .ok_or_else(|| Error::NotFound("SKU当前修订不存在".into()))?;
        Ok((sku, old))
    }

    async fn write_source_sku_changes(
        &self,
        product: &str,
        id: &str,
        changes: &[(SourceField, Option<String>, SourceValue)],
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (mut sku, old) = self.current_source_sku(product, id, executor).await?;
        let latest = self.db.catalog().latest_sku_revision_no(&SkuId::new(id), executor).await?;
        let name = changes
            .iter()
            .find_map(|(field, sku, value)| match (field, sku.as_deref(), value) {
                (SourceField::SkuName, Some(sku), SourceValue::Text(Some(name))) if sku == id => {
                    Some(name.clone())
                },
                _ => None,
            })
            .unwrap_or_else(|| old.name.clone());
        let mut revision = source_sku_successor(&sku, &old, next_revision_no(latest)?, name)?;
        for (field, sku, value) in changes {
            if *field != SourceField::MainImage || sku.as_deref() != Some(id) {
                continue;
            }
            let SourceValue::Images(images) = value else {
                return Err(Error::ValidationError("SKU主图类型无效".into()));
            };
            if images.len() > 1 {
                return Err(Error::ValidationError("SKU主图最多一张".into()));
            }
            revision.source_main_image_asset_id = if let Some(image) = images.first() {
                Some(self.source_image_asset(image, executor).await?)
            } else {
                None
            };
        }
        sku.attach_revision(&revision, actor)?;
        self.db.catalog().create_sku_revision(&revision, executor).await?;
        self.db.skus().update(&mut sku, executor).await?;
        Ok(())
    }
}

struct MongoResolutionStore<'a> {
    service: &'a CatalogService,
    actor: &'a AuditActor,
    id: &'a str,
    executor: &'a mut dyn Executor,
    product_version: Option<u64>,
}

#[async_trait]
impl ResolutionStore for MongoResolutionStore<'_> {
    async fn authorize(&mut self) -> Result<()> {
        let product =
            self.service.access().require_product(self.actor, "update", self.id, self.executor).await?;
        self.product_version = Some(product.base.version);
        Ok(())
    }
    async fn receipt(&mut self, key: &str) -> Result<Option<SourceResolutionReceipt>> {
        self.service
            .db
            .source_resolution_receipts()
            .find_by_id_including_deleted(&receipt_id(self.id, key), self.executor)
            .await
            .map_err(Into::into)
    }
    async fn facts(&mut self) -> Result<ResolutionFacts> {
        let source = self
            .service
            .db
            .product_sources()
            .find_by_id(self.id, self.executor)
            .await?
            .ok_or_else(|| Error::NotFound("商品资料来源不存在".into()))?;
        let current = self.service.source_company_content(self.id, self.executor).await?;
        Ok(ResolutionFacts {
            product_version: self
                .product_version
                .ok_or_else(|| Error::Forbidden("商品维护权限尚未核验".into()))?,
            source,
            current,
        })
    }
    async fn save(
        &mut self,
        mut source: ProductSource,
        changes: Vec<(SourceField, Option<String>, SourceValue)>,
        req: ResolveSourceDiffRequest,
    ) -> Result<ProductSourceDiffView> {
        let version = if changes.is_empty() {
            let mut product = self
                .service
                .db
                .products()
                .find_by_id(self.id, self.executor)
                .await?
                .ok_or_else(|| Error::NotFound("商品不存在".into()))?;
            self.service.db.products().update(&mut product, self.executor).await?;
            product.base.version
        } else {
            self.service.write_source_content(self.id, &changes, self.actor.id(), self.executor).await?
        };
        self.service.db.product_sources().update(&mut source, self.executor).await?;
        let audit = self.service.audit.resource_log(
            self.actor.clone(),
            "product.source_diff.resolve",
            "product",
            self.id.to_owned(),
        )?;
        self.service.audit.persist(&audit, self.executor).await?;
        let mut result = source.view(version);
        let current = self.service.source_company_content(self.id, self.executor).await?;
        result.sku_versions = current.sku_versions;
        for field in &mut result.fields {
            field.sku_no = field.sku_id.as_ref().and_then(|id| current.sku_numbers.get(id)).cloned();
        }
        let receipt = SourceResolutionReceipt {
            base: BaseModel::new(receipt_id(self.id, &req.idempotency_key)),
            product_id: self.id.into(),
            request: req,
            product_version: version,
            source_version: source.base.version,
            result: result.clone(),
        };
        self.service.db.source_resolution_receipts().create(&receipt, self.executor).await?;
        Ok(result)
    }
}
fn receipt_id(product: &str, key: &str) -> String {
    format!("{}:{}:{}", product.len(), product, key)
}

fn current_media_values(media: &[ProductRevisionMedia]) -> Vec<(SourceField, Option<String>, SourceValue)> {
    [(SourceField::CarouselMedia, MediaRole::Carousel), (SourceField::DetailMedia, MediaRole::Detail)]
        .into_iter()
        .map(|(field, role)| {
            let mut rows = media.iter().filter(|m| m.media_role == role).collect::<Vec<_>>();
            rows.sort_by_key(|m| m.sort_order);
            (
                field,
                None,
                SourceValue::Images(
                    rows.into_iter()
                        .map(|m| SourceImage {
                            url: None,
                            file_asset_id: Some(m.file_asset_id.to_string()),
                            content_fingerprint: None,
                        })
                        .collect(),
                ),
            )
        })
        .collect()
}
fn source_successor(
    id: &str,
    old: ProductRevision,
    latest: Option<u32>,
    changes: &[(SourceField, Option<String>, SourceValue)],
) -> Result<ProductRevision> {
    let mut name = old.name;
    let mut description = old.description;
    for (field, _, value) in changes {
        match (field, value) {
            (SourceField::Name, SourceValue::Text(Some(v))) => name = v.clone(),
            (SourceField::Description, SourceValue::Text(v)) => description = v.clone(),
            _ => {},
        }
    }
    Ok(ProductRevision::new(
        ProductRevisionId::new(next_id()),
        ProductRevisionData {
            product_id: ProductId::new(id),
            revision_no: next_revision_no(latest)?,
            name,
            description,
            specification: old.specification,
            category_id: old.category_id,
            brand_id: old.brand_id,
            status: old.status,
            effective_from: old.effective_from,
            effective_to: old.effective_to,
        },
    )?)
}

fn copy_source_media(
    media: Vec<ProductRevisionMedia>,
    revision: &ProductRevisionId,
) -> Result<Vec<ProductRevisionMedia>> {
    media
        .into_iter()
        .map(|m| {
            m.copy_to_revision(ProductRevisionMediaId::new(next_id()), revision.clone()).map_err(Into::into)
        })
        .collect()
}

fn source_sku_ids(changes: &[(SourceField, Option<String>, SourceValue)]) -> Result<BTreeSet<String>> {
    changes
        .iter()
        .filter(|(field, _, _)| matches!(field, SourceField::MainImage | SourceField::SkuName))
        .map(|(_, sku, _)| {
            sku.clone().ok_or_else(|| Error::ValidationError("规格展示字段必须指定SKU".into()))
        })
        .collect()
}

fn source_sku_successor(sku: &Sku, old: &SkuRevision, revision_no: u32, name: String) -> Result<SkuRevision> {
    let mut revision = old.content_successor(
        SkuRevisionId::new(next_id()),
        revision_no,
        name,
        old.description.clone(),
        old.effective_from,
        old.effective_to,
    )?;
    revision.status = sku.stable.status;
    Ok(revision)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::UnitOfMeasureId;

    use super::*;
    use crate::entity::catalog::sku::SkuData;
    use crate::entity::catalog::sku_revision::SkuRevisionData;
    use crate::entity::catalog::{EMPTY_SPEC_SIGNATURE, EnableStatus, ListingStatus};
    #[test]
    fn sku_name_and_image_changes_are_grouped_into_one_revision_per_sku() {
        let changes = vec![
            (SourceField::SkuName, Some("sku-a".into()), SourceValue::Text(Some("新名称".into()))),
            (SourceField::MainImage, Some("sku-a".into()), SourceValue::Images(Vec::new())),
            (SourceField::Name, None, SourceValue::Text(Some("商品名称".into()))),
        ];
        assert_eq!(source_sku_ids(&changes).unwrap(), BTreeSet::from(["sku-a".into()]));
        assert!(
            source_sku_ids(&[(SourceField::SkuName, None, SourceValue::Text(Some("缺SKU".into())))]).is_err()
        );
    }
    #[test]
    fn source_name_or_image_update_cannot_reactivate_a_manually_disabled_sku() {
        let mut sku = Sku::new(
            SkuId::new("sku"),
            SkuData {
                sku_no: "SKU-1".into(),
                product_id: ProductId::new("product"),
                base_unit_id: UnitOfMeasureId::new("unit"),
                specification_signature: EMPTY_SPEC_SIGNATURE.into(),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            "buyer",
        )
        .unwrap();
        let old = SkuRevision::new(
            SkuRevisionId::new("old"),
            SkuRevisionData {
                sku_id: SkuId::new("sku"),
                revision_no: 1,
                name: "旧名称".into(),
                description: None,
                specification: None,
                barcode: Some("barcode".into()),
                source_main_image_asset_id: Some(FileAssetId::new("old-image")),
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: Some("100.00".parse().unwrap()),
                bulk_price_gross: None,
                bulk_min_quantity: None,
                market_price: Some("199.00".parse().unwrap()),
                status: EnableStatus::Active,
                effective_from: BusinessDate::today(),
                effective_to: None,
            },
        )
        .unwrap();
        sku.attach_revision(&old, "buyer").unwrap();
        sku.disable("buyer").unwrap();
        let mut next = source_sku_successor(&sku, &old, 2, "新名称".into()).unwrap();
        next.source_main_image_asset_id = Some(FileAssetId::new("new-image"));
        sku.attach_revision(&next, "sync-actor").unwrap();
        assert!(!sku.is_active());
        assert_eq!(next.status, EnableStatus::Disabled);
        assert_eq!(sku.listing_status, ListingStatus::Unlisted);
        assert_eq!(next.sales_visible_price_gross, old.sales_visible_price_gross);
        assert_eq!(next.market_price, old.market_price);
        assert_eq!(next.barcode, old.barcode);
    }
}
