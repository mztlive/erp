use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use ::image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
use erp_supply::ports::connector::offer::{Offer, OfferDetails};

use super::*;

struct Images {
    downloads: Mutex<VecDeque<DownloadedSourceImage>>,
    count: AtomicUsize,
}

#[async_trait]
impl SourceImageTransport for Images {
    async fn download(&self, _url: &str) -> Result<DownloadedSourceImage> {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.downloads
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| Error::BusinessLogicError("source unavailable".into()))
    }
}

#[derive(Default)]
struct Stored(Mutex<Vec<Vec<u8>>>, Mutex<Vec<String>>);

#[async_trait]
impl SourceImageStore for Stored {
    async fn save(&self, _key: &str, bytes: &[u8], content_type: &str) -> Result<()> {
        assert_eq!(content_type, "image/png");
        self.0.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.1.lock().unwrap().push(key.into());
        Ok(())
    }
}

fn png(value: u8) -> DownloadedSourceImage {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([value, 0, 0])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    DownloadedSourceImage { bytes: bytes.into_inner(), content_type: Some("image/png".into()) }
}

fn source(url: &str) -> CatalogImage {
    CatalogImage { url: url.into(), content_fingerprint: None, file_asset_id: None }
}

fn item(images: Option<Vec<CatalogImage>>, details: Option<Vec<CatalogImage>>) -> SupplierCatalogItem {
    SupplierCatalogItem::observed(
        "connection",
        "supplier",
        "job",
        Snapshot {
            value: Offer {
                sku: SupplierSku { product_id: Some("product".into()), spec_id: "spec".into() },
                name: "name".into(),
                specification: "spec".into(),
                unit: Some("个".into()),
                quote: None,
                details: OfferDetails { images, detail_images: details, ..Default::default() },
            },
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(10),
            },
        },
    )
    .unwrap()
}

fn preparer(downloads: Vec<DownloadedSourceImage>) -> (SourceMediaPreparer, Arc<Images>, Arc<Stored>) {
    let transport = Arc::new(Images { downloads: Mutex::new(downloads.into()), count: AtomicUsize::new(0) });
    let store = Arc::new(Stored::default());
    let preparer =
        SourceMediaPreparer::with_ports(transport.clone(), store.clone(), b"key".to_vec()).unwrap();
    (preparer, transport, store)
}

fn actor() -> AuditActor {
    AuditActor::new("actor".into(), "account".into(), AccountKind::Admin)
}

#[test]
fn archived_selection_uses_only_verified_images_without_network() {
    let mut verified = source("https://cdn.example/ok");
    verified.content_fingerprint = Some("a".repeat(64));
    verified.file_asset_id = Some("asset".into());
    let item = item(Some(vec![source("https://cdn.example/missing"), verified]), None);
    let prepared = PreparedSourceMedia::from_archived(&item);
    assert_eq!(prepared.carousel_media.len(), 1);
    assert_eq!(prepared.carousel_media[0].file_asset_id.as_ref(), "asset");
    assert_eq!(prepared.carousel_media[0].sort_order, 1);
    assert!(prepared.pending_assets.is_empty());
}

#[tokio::test]
async fn prepares_real_pending_assets_and_keeps_order_and_absent_groups() {
    let (preparer, _, stored) = preparer(vec![png(1), png(2)]);
    let mut item = item(Some(vec![source("https://cdn.example/a"), source("https://cdn.example/b")]), None);
    let prepared = preparer.prepare_source_media(&item, &actor()).await.unwrap();
    assert_eq!(stored.0.lock().unwrap().len(), 2);
    assert!(prepared.detail_images.is_none());
    assert_eq!(prepared.carousel_media.iter().map(|media| media.sort_order).collect::<Vec<_>>(), [0, 1]);
    let sources = prepared.images.as_ref().unwrap();
    assert_eq!(sources[0].url, "https://cdn.example/a");
    assert_ne!(sources[0].content_fingerprint, sources[1].content_fingerprint);
    assert!(
        sources.iter().all(|source| !source.file_asset_id.as_ref().unwrap().starts_with("pending-file:"))
    );
    let mut used = HashSet::new();
    for (index, media) in prepared.carousel_media.iter().enumerate() {
        let mut id = media.file_asset_id.clone();
        assert!(prepared.pending_assets.resolve_id(&mut id, &mut used).unwrap());
        assert_eq!(Some(id.as_ref()), sources[index].file_asset_id.as_deref());
    }
    prepared.pending_assets.ensure_all_used(&used).unwrap();
    prepared.apply_to(&mut item).unwrap();
    assert_eq!(item.images.as_ref().unwrap()[0].file_asset_id, sources[0].file_asset_id);
}

#[tokio::test]
async fn same_url_changed_bytes_changes_fingerprint_and_same_content_reuses_asset() {
    let (preparer, transport, stored) = preparer(vec![png(1), png(1), png(2)]);
    let mut item = item(Some(vec![source("https://cdn.example/a")]), Some(Vec::new()));
    let first = preparer.prepare_source_media(&item, &actor()).await.unwrap();
    first.apply_to(&mut item).unwrap();
    let second = preparer.prepare_source_media(&item, &actor()).await.unwrap();
    assert!(second.pending_assets.is_empty());
    assert_eq!(first.images, second.images);
    assert_eq!(second.detail_images, Some(Vec::new()));
    let third = preparer.prepare_source_media(&item, &actor()).await.unwrap();
    assert_ne!(first.images, third.images);
    assert_eq!(transport.count.load(Ordering::Relaxed), 3);
    assert_eq!(stored.0.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn signed_url_changes_reuse_real_asset_when_content_is_identical() {
    let (preparer, _, stored) = preparer(vec![png(1), png(1)]);
    let mut previous = item(Some(vec![source("https://cdn.example/a?signature=old")]), None);
    let first = preparer.prepare_source_media(&previous, &actor()).await.unwrap();
    first.apply_to(&mut previous).unwrap();
    let current = item(Some(vec![source("https://cdn.example/a?signature=new")]), None);
    let second = preparer.prepare_observation_media(&current, Some(&previous), &actor()).await.unwrap();
    assert!(second.pending_assets.is_empty());
    assert_eq!(stored.0.lock().unwrap().len(), 1);
    let old = &first.images.as_ref().unwrap()[0];
    let new = &second.images.as_ref().unwrap()[0];
    assert_eq!(old.file_asset_id, new.file_asset_id);
    assert_eq!(old.content_fingerprint, new.content_fingerprint);
    assert_ne!(old.url, new.url);
}

#[tokio::test]
async fn observation_keeps_failed_image_unverified_and_prepares_other_images() {
    let invalid = DownloadedSourceImage { bytes: b"broken".to_vec(), content_type: Some("image/png".into()) };
    let (preparer, _, stored) = preparer(vec![png(1), invalid, png(2)]);
    let mut current = item(
        Some(vec![source("https://cdn.example/a"), source("https://cdn.example/b")]),
        Some(vec![source("https://cdn.example/detail")]),
    );
    let result = preparer.prepare_observation_media(&current, None, &actor()).await.unwrap();
    assert_eq!(result.verification_errors.len(), 1);
    assert_eq!(stored.0.lock().unwrap().len(), 2);
    assert_eq!(result.images.as_ref().unwrap().len(), 2);
    assert!(result.images.as_ref().unwrap()[0].file_asset_id.is_some());
    assert!(result.images.as_ref().unwrap()[1].file_asset_id.is_none());
    assert!(result.images.as_ref().unwrap()[1].content_fingerprint.is_none());
    assert!(result.detail_images.as_ref().unwrap()[0].file_asset_id.is_some());
    result.apply_to(&mut current).unwrap();
    assert_eq!(current.name, "name");
    assert_eq!(current.images.as_ref().unwrap()[1].url, "https://cdn.example/b");
}

#[tokio::test]
async fn retries_only_unverified_images_and_preserves_completed_assets() {
    let (first_preparer, transport, _) = preparer(vec![png(1), png(2)]);
    let mut current =
        item(Some(vec![source("https://cdn.example/a"), source("https://cdn.example/b")]), None);
    let first = first_preparer.prepare_observation_media(&current, None, &actor()).await.unwrap();
    first.apply_to(&mut current).unwrap();
    let first_id = current.images.as_ref().unwrap()[0].file_asset_id.clone();
    current.images.as_mut().unwrap()[1].file_asset_id = None;
    current.images.as_mut().unwrap()[1].content_fingerprint = None;
    let (retry, retry_transport, _) = preparer(vec![png(2)]);
    let result = retry.prepare_pending_media(&current, &actor()).await.unwrap();
    assert_eq!(transport.count.load(Ordering::Relaxed), 2);
    assert_eq!(retry_transport.count.load(Ordering::Relaxed), 1);
    assert_eq!(result.images.unwrap()[0].file_asset_id, first_id);
}

#[tokio::test]
async fn malformed_or_unavailable_image_never_becomes_empty_success() {
    let corrupt = DownloadedSourceImage {
        bytes: b"\x89PNG\r\n\x1a\nforged".to_vec(),
        content_type: Some("image/png".into()),
    };
    let (preparer, _, stored) = preparer(vec![corrupt]);
    let item = item(Some(vec![source("https://cdn.example/a")]), None);
    assert!(preparer.prepare_source_media(&item, &actor()).await.is_err());
    assert!(stored.0.lock().unwrap().is_empty());
    assert!(preparer.prepare_source_media(&item, &actor()).await.is_err());
    assert_eq!(item.images.as_ref().unwrap()[0].file_asset_id, None);
}

#[tokio::test]
async fn strict_preparation_failure_discards_prior_uploads_and_preserves_original_error() {
    let invalid = DownloadedSourceImage { bytes: b"broken".to_vec(), content_type: Some("image/png".into()) };
    let (preparer, _, stored) = preparer(vec![png(1), invalid]);
    let current = item(Some(vec![source("https://cdn.example/a"), source("https://cdn.example/b")]), None);
    let outcome = preparer.prepare_source_media(&current, &actor()).await;
    assert!(matches!(outcome, Err(Error::ValidationError(_))));
    assert_eq!(stored.0.lock().unwrap().len(), 1);
    let deleted = stored.1.lock().unwrap();
    assert_eq!(deleted.len(), 1);
    assert!(deleted[0].starts_with("supplier-source-media/"));
}

#[tokio::test]
async fn rollback_cleanup_excludes_reused_assets() {
    let (preparer, _, stored) = preparer(vec![png(1), png(1)]);
    let mut previous = item(Some(vec![source("https://cdn.example/a")]), None);
    preparer.prepare_source_media(&previous, &actor()).await.unwrap().apply_to(&mut previous).unwrap();
    let prepared = preparer.prepare_source_media(&previous, &actor()).await.unwrap();
    let rollback: Result<()> = Err(Error::ConflictError("transaction rolled back".into()));
    prepared.cleanup_handle().after_transaction(&rollback).await;
    assert_eq!(stored.0.lock().unwrap().len(), 1);
    assert!(stored.1.lock().unwrap().is_empty());
}

#[tokio::test]
async fn empty_and_limit_checks_do_not_fetch_images() {
    let (preparer, transport, _) = preparer(Vec::new());
    let result = preparer.prepare_source_media(&item(None, Some(Vec::new())), &actor()).await.unwrap();
    assert!(result.images.is_none());
    assert_eq!(result.detail_images, Some(Vec::new()));
    let large = item(Some(vec![source("https://cdn.example/a"); 101]), None);
    assert!(preparer.prepare_source_media(&large, &actor()).await.is_err());
    assert_eq!(transport.count.load(Ordering::Relaxed), 0);
}

#[test]
fn validation_rejects_mime_mismatch_and_oversized_decoded_dimensions() {
    let mut image = png(1);
    image.content_type = Some("image/jpeg".into());
    assert!(super::image::validate(image).is_err());
    let mut header_only = png(1);
    header_only.bytes.truncate(16);
    assert!(super::image::validate(header_only).is_err());
    let large = DynamicImage::new_rgb8(16385, 1);
    let mut bytes = Cursor::new(Vec::new());
    large.write_to(&mut bytes, ImageFormat::Png).unwrap();
    let large = DownloadedSourceImage { bytes: bytes.into_inner(), content_type: Some("image/png".into()) };
    assert!(super::image::validate(large).is_err());
}
