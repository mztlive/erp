//! 供应商来源图片的下载、内容核验与事务外对象准备。

use std::collections::HashSet;
use std::mem::take;
use std::sync::Arc;
use std::time::Duration;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_catalog::{EmptyPendingAttachments, PendingAttachmentBatch, ProductMediaInput};
use erp_core::ids::FileAssetId;
use erp_supply::entity::supplier_catalog::{CatalogImage, SupplierCatalogItem};
use erp_support::{
    PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass, SensitivityClass, content_fingerprint,
};
use id_generator::next_id;
use storage::S3Storage;
use tokio::task::spawn_blocking;
use tokio::time::{Instant, timeout_at};

use crate::adapters::CatalogPendingAttachments;
use crate::attachments::PendingFileAssets;
use crate::{Error, Result};

mod cleanup;
mod image;
mod transport;

pub use cleanup::SourceMediaCleanup;
pub use transport::HttpSourceImages;

const MAX_SOURCE_IMAGES: usize = 100;
const MEDIA_PREPARATION_TIMEOUT: Duration = Duration::from_secs(60);

/// 实际下载所得字节与服务器声明的内容类型；不包含鉴权凭据。
pub struct DownloadedSourceImage {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}

/// 可替换的来源读取边界；生产实现必须限制请求时长及响应体。
#[async_trait]
pub trait SourceImageTransport: Send + Sync {
    /// 按供应商提供的链接读取图片。
    /// # 参数
    /// `url` 为供应商响应中的 HTTP/HTTPS 图片地址。
    /// # 返回
    /// 有界图片字节和内容类型。
    /// # 错误
    /// 无效地址、网络错误及超限时返回明确错误。
    async fn download(&self, url: &str) -> Result<DownloadedSourceImage>;
}

/// 对象字节写入边界；文件资产元数据由调用方业务事务提交。
#[async_trait]
pub trait SourceImageStore: Send + Sync {
    /// 保存已经完整核验的图片字节。
    /// # 参数
    /// 安全相对对象键、真实图片字节及检测得到的 MIME。
    /// # 返回
    /// 对象已写入。
    /// # 错误
    /// 存储写入失败时返回错误，不登记资产。
    async fn save(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<()>;
    /// 删除本次上传且已证明未被提交引用的对象。
    /// # 参数
    /// `key` 仅来自本次准备过程生成的安全对象键。
    /// # 返回
    /// 对象已删除或不存在。
    /// # 错误
    /// 存储删除失败时返回错误。
    async fn delete(&self, key: &str) -> Result<()>;
}

struct S3SourceImages(S3Storage);

#[async_trait]
impl SourceImageStore for S3SourceImages {
    async fn save(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<()> {
        self.0
            .save_with_content_type(key, bytes, Some(content_type))
            .await
            .map_err(|_| Error::Internal("保存供应商商品图片失败，请重试".into()))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.0.delete(key).await.map_err(|_| Error::Internal("清理未使用的供应商图片失败".into()))
    }
}

/// 来源图和商品媒体共享的准备结果；资产元数据尚未提交。
pub struct PreparedSourceMedia {
    pub pending_assets: Arc<dyn PendingAttachmentBatch>,
    pub carousel_media: Vec<ProductMediaInput>,
    pub detail_media: Vec<ProductMediaInput>,
    pub images: Option<Vec<CatalogImage>>,
    pub detail_images: Option<Vec<CatalogImage>>,
    pub verification_errors: Vec<String>,
    cleanup: SourceMediaCleanup,
}

impl PreparedSourceMedia {
    /// 构造无需外部请求的空媒体结果。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未提供图片组及空资产批次。
    /// # 错误
    /// 不返回错误。
    pub fn empty() -> Self {
        Self {
            pending_assets: Arc::new(EmptyPendingAttachments),
            carousel_media: Vec::new(),
            detail_media: Vec::new(),
            images: None,
            detail_images: None,
            verification_errors: Vec::new(),
            cleanup: SourceMediaCleanup::default(),
        }
    }

    /// 把已核验图片写入来源观察，保留未提供与明确清空的区别。
    /// # 参数
    /// `item` 为本次准备对应的目录观察。
    /// # 返回
    /// 更新来源媒体和内容摘要，不修改商品匹配或采购资料。
    /// # 错误
    /// 来源摘要无法序列化时返回错误。
    pub fn apply_to(&self, item: &mut SupplierCatalogItem) -> Result<()> {
        item.images = self.images.clone();
        item.detail_images = self.detail_images.clone();
        item.refresh_fingerprint()?;
        Ok(())
    }

    /// 保留跨事务移动前的本次新对象补偿清单。
    /// # 参数
    /// 无。
    /// # 返回
    /// 仅包含本次上传、不包含复用资产的补偿句柄。
    /// # 错误
    /// 不返回错误。
    pub fn cleanup_handle(&self) -> SourceMediaCleanup {
        self.cleanup.clone()
    }
}

/// 可复用的媒体准备器；不在数据库事务内调用。
#[derive(Clone)]
pub struct SourceMediaPreparer {
    transport: Arc<dyn SourceImageTransport>,
    store: Arc<dyn SourceImageStore>,
    secret: Arc<[u8]>,
}

struct MediaPreparation<'a> {
    previous: Vec<&'a CatalogImage>,
    requests: Vec<PendingFileAssetRequest>,
    verification_errors: Vec<String>,
    deadline: Instant,
    tolerate_unverified: bool,
    cleanup: SourceMediaCleanup,
}

impl SourceMediaPreparer {
    /// 使用 HTTP 客户端与现有对象存储装配来源图片准备器。
    /// # 参数
    /// 对象存储及内容 HMAC 密钥。
    /// # 返回
    /// 来源图片准备器，不执行网络请求。
    /// # 错误
    /// 空密钥或 HTTP 客户端初始化失败时返回错误。
    pub fn new(storage: S3Storage, secret: Vec<u8>) -> Result<Self> {
        Self::with_ports(Arc::new(HttpSourceImages::new()?), Arc::new(S3SourceImages(storage)), secret)
    }

    /// 从已验证的读取与存储边界构造准备器。
    /// # 参数
    /// 来源传输、对象存储及内容 HMAC 密钥。
    /// # 返回
    /// 可注入替身的完整生产编排。
    /// # 错误
    /// 空密钥时拒绝。
    pub fn with_ports(
        transport: Arc<dyn SourceImageTransport>,
        store: Arc<dyn SourceImageStore>,
        secret: Vec<u8>,
    ) -> Result<Self> {
        if secret.is_empty() {
            return Err(Error::ValidationError("来源图片内容校验密钥未配置".into()));
        }
        Ok(Self { transport, store, secret: secret.into() })
    }

    /// 下载并准备一条来源目录的两组图片。
    /// # 参数
    /// `item` 是受授权目录观察，`actor` 用于资产审计；调用方须在事务外执行。
    /// # 返回
    /// 临时商品媒体、真实来源资产身份、内容摘要及事务内待提交批次。
    /// # 错误
    /// 超过一百张、任一下载/核验/上传失败或资产准备失败时拒绝，不清空原图片。
    pub async fn prepare_source_media(
        &self,
        item: &SupplierCatalogItem,
        actor: &AuditActor,
    ) -> Result<PreparedSourceMedia> {
        self.prepare(item, Some(item), actor, false).await
    }

    /// 核验持续同步图片；单图失败保留未核验来源，其他文字及图片仍可更新。
    /// # 参数
    /// 新观察、此前已归档目录及审计操作人；此前资料只用于按真实内容复用资产。
    /// # 返回
    /// 带待核验说明的准备结果；失败图片不携带资产或内容证明。
    /// # 错误
    /// 超出图片数量上限或文件批次无法准备时返回错误。
    pub async fn prepare_observation_media(
        &self,
        item: &SupplierCatalogItem,
        previous: Option<&SupplierCatalogItem>,
        actor: &AuditActor,
    ) -> Result<PreparedSourceMedia> {
        self.prepare(item, previous, actor, true).await
    }

    async fn prepare(
        &self,
        item: &SupplierCatalogItem,
        previous: Option<&SupplierCatalogItem>,
        actor: &AuditActor,
        tolerate_unverified: bool,
    ) -> Result<PreparedSourceMedia> {
        let count =
            item.images.as_ref().map_or(0, Vec::len) + item.detail_images.as_ref().map_or(0, Vec::len);
        if count > MAX_SOURCE_IMAGES {
            return Err(Error::ValidationError("供应商商品图片超过 100 张，请分批处理".into()));
        }
        let previous = previous
            .into_iter()
            .flat_map(|item| item.images.iter().chain(item.detail_images.iter()).flatten())
            .collect::<Vec<_>>();
        let mut preparation = MediaPreparation {
            previous,
            requests: Vec::new(),
            verification_errors: Vec::new(),
            deadline: Instant::now() + MEDIA_PREPARATION_TIMEOUT,
            tolerate_unverified,
            cleanup: SourceMediaCleanup::new(self.store.clone()),
        };
        let result = self.prepare_groups(item, actor, &mut preparation).await;
        if result.is_err() {
            preparation.cleanup.discard_uncommitted().await;
        }
        result
    }

    async fn prepare_groups(
        &self,
        item: &SupplierCatalogItem,
        actor: &AuditActor,
        preparation: &mut MediaPreparation<'_>,
    ) -> Result<PreparedSourceMedia> {
        let (images, carousel_media) = self.prepare_group(item.images.as_deref(), preparation).await?;
        let (detail_images, detail_media) =
            self.prepare_group(item.detail_images.as_deref(), preparation).await?;
        let pending_assets = CatalogPendingAttachments::from_support(
            PendingFileAssets::prepare_content_checked(take(&mut preparation.requests), actor)?.shared(),
        );
        let mut prepared = PreparedSourceMedia {
            pending_assets,
            images,
            detail_images,
            carousel_media,
            detail_media,
            verification_errors: take(&mut preparation.verification_errors),
            cleanup: preparation.cleanup.clone(),
        };
        prepared.resolve_source_ids()?;
        Ok(prepared)
    }

    async fn prepare_group(
        &self,
        images: Option<&[CatalogImage]>,
        preparation: &mut MediaPreparation<'_>,
    ) -> Result<(Option<Vec<CatalogImage>>, Vec<ProductMediaInput>)> {
        let Some(images) = images else { return Ok((None, Vec::new())) };
        let mut sources = Vec::with_capacity(images.len());
        let mut media = Vec::with_capacity(images.len());
        for (position, source) in images.iter().enumerate() {
            let checkpoint = preparation.cleanup.checkpoint();
            let result = timeout_at(
                preparation.deadline,
                self.prepare_image(
                    source,
                    &preparation.previous,
                    &mut preparation.requests,
                    &mut preparation.cleanup,
                ),
            )
            .await
            .map_err(|_| Error::BusinessLogicError("商品图片准备超时，部分图片等待重新核验".into()))
            .and_then(|result| result);
            let (source, reference) = match result {
                Ok(prepared) => prepared,
                Err(error) if preparation.tolerate_unverified => {
                    preparation.cleanup.discard_since(checkpoint).await;
                    preparation.verification_errors.push(error.to_string());
                    sources.push(CatalogImage {
                        url: source.url.clone(),
                        content_fingerprint: None,
                        file_asset_id: None,
                    });
                    continue;
                },
                Err(error) => return Err(error),
            };
            sources.push(source);
            media.push(ProductMediaInput {
                file_asset_id: reference,
                sort_order: i32::try_from(position).expect("来源图片最多一百张"),
                alt_text: None,
            });
        }
        Ok((Some(sources), media))
    }

    async fn prepare_image(
        &self,
        source: &CatalogImage,
        previous: &[&CatalogImage],
        requests: &mut Vec<PendingFileAssetRequest>,
        cleanup: &mut SourceMediaCleanup,
    ) -> Result<(CatalogImage, FileAssetId)> {
        let downloaded = self.transport.download(&source.url).await?;
        let image = spawn_blocking(move || image::validate(downloaded))
            .await
            .map_err(|_| Error::Internal("供应商商品图片核验中断，请重试".into()))??;
        if let Some(id) = previous.iter().find_map(|previous| verified_existing(previous, &image.digest)) {
            let source = CatalogImage {
                url: source.url.clone(),
                content_fingerprint: Some(image.digest),
                file_asset_id: Some(id.into()),
            };
            return Ok((source, FileAssetId::new(id)));
        }
        let token = next_id();
        let object_key = format!("supplier-source-media/{token}.{}", image.extension);
        cleanup.track(object_key.clone());
        self.store.save(&object_key, &image.bytes, image.content_type).await?;
        let reference = format!("pending-file:{token}");
        requests.push(PendingFileAssetRequest {
            reference: reference.clone(),
            registration: RegisterFileAssetRequest {
                storage_object_key: object_key,
                file_name: format!("supplier-product.{}", image.extension),
                content_type: image.content_type.into(),
                byte_size: u64::try_from(image.bytes.len()).expect("来源图片最多 10 MiB"),
                content_hmac: content_fingerprint(&image.digest, &self.secret),
                sensitivity_class: SensitivityClass::General,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
            },
        });
        let source = CatalogImage {
            url: source.url.clone(),
            content_fingerprint: Some(image.digest),
            file_asset_id: Some(reference.clone()),
        };
        Ok((source, FileAssetId::new(reference)))
    }
}

impl PreparedSourceMedia {
    fn resolve_source_ids(&mut self) -> Result<()> {
        let mut used = HashSet::new();
        for source in self.images.iter_mut().chain(self.detail_images.iter_mut()).flatten() {
            let Some(id) = &source.file_asset_id else { continue };
            let mut id = FileAssetId::new(id);
            self.pending_assets.resolve_id(&mut id, &mut used)?;
            source.file_asset_id = Some(id.to_string());
        }
        self.pending_assets.ensure_all_used(&used)?;
        Ok(())
    }
}

fn verified_existing<'a>(source: &'a CatalogImage, digest: &str) -> Option<&'a str> {
    let id = source.file_asset_id.as_deref()?;
    (source.content_fingerprint.as_deref() == Some(digest)
        && !id.is_empty()
        && !id.starts_with("pending-file:"))
    .then_some(id)
}

#[cfg(test)]
mod tests;
