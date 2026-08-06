//! 域 D05 `file_asset` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test file_asset_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::FileAssetExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::file_asset::{
    content_fingerprint, AttachmentUsage, ContentHmac, DocumentAttachment, DocumentAttachmentData, FileAsset,
    FileAssetData, RetentionClass, SecurityScanStatus, SensitivityClass,
};
use entities::ids::{BusinessDocumentId, DocumentAttachmentId, FileAssetId};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 文件资产列表筛选条件类型（经 `FileAssetExt` 关联类型跨 crate 可达）。
type FileAssetFilter = <Database as FileAssetExt>::FileAssetFilter;

/// 构造可复用的文件资产实体。
fn sample_asset(id: &str, storage_key: &str) -> FileAsset {
    FileAsset::new(
        FileAssetId::new(id),
        FileAssetData {
            storage_object_key: storage_key.to_string(),
            file_name: "导入清单.xlsx".to_string(),
            content_type: "application/vnd.ms-excel".to_string(),
            byte_size: 2048,
            content_hmac: ContentHmac::parse(content_fingerprint("content", b"secret-key")).unwrap(),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::ThirtyDays,
            expires_at: Some(entities::common::time::Instant::from_unix_secs(1_703_260_800)),
            created_by: "admin-1".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的附件关联实体。
fn sample_attachment(id: &str, document_id: &str, asset_id: &str) -> DocumentAttachment {
    DocumentAttachment::new(
        DocumentAttachmentId::new(id),
        DocumentAttachmentData {
            document_id: BusinessDocumentId::new(document_id),
            file_asset_id: FileAssetId::new(asset_id),
            usage: AttachmentUsage::Attachment,
            created_by: "admin-1".to_string(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as FileAssetExt>::FILE_ASSETS,
        &[
            "uk_file_assets_storage_key",
            "idx_file_assets_scan_retention",
            "idx_file_assets_expires_at",
        ],
    )
    .await
    .expect("file_assets 索引缺失");
    assert_indexes(
        db,
        <Database as FileAssetExt>::DOCUMENT_ATTACHMENTS,
        &[
            "uk_document_attachments_id",
            "idx_document_attachments_document",
            "idx_document_attachments_asset",
        ],
    )
    .await
    .expect("document_attachments 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_read_roundtrip_and_scan_transition() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut asset = sample_asset("fa-1", "obj/2025/08/abc123");
        db.file_assets().create(&asset, &mut NoTransaction).await.unwrap();
        assert_eq!(asset.base.version, 1);

        let found = db
            .file_assets()
            .find_by_id(&asset.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.storage_object_key, "obj/2025/08/abc123");
        assert_eq!(found.file_name, "导入清单.xlsx");
        assert_eq!(found.byte_size, 2048);
        assert_eq!(found.security_scan_status, SecurityScanStatus::Pending);
        assert_eq!(
            found.content_hmac,
            ContentHmac::parse(content_fingerprint("content", b"secret-key")).unwrap()
        );

        let by_key = db
            .file_assets()
            .find_by_storage_key("obj/2025/08/abc123", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按对象键应命中");
        assert_eq!(by_key.base.id, "fa-1");

        asset.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        db.file_assets()
            .update(&mut asset, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(asset.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(asset.security_scan_status, SecurityScanStatus::Passed);
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_storage_key_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.file_assets()
            .create(&sample_asset("fa-1", "obj/2025/08/abc123"), &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_asset("fa-2", "obj/2025/08/abc123");
        let error = db
            .file_assets()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复对象键必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_and_restore_match_deleted_state() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_soft").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut asset = sample_asset("fa-1", "obj/2025/08/abc123");
        db.file_assets().create(&asset, &mut NoTransaction).await.unwrap();

        db.file_assets()
            .soft_delete(&mut asset, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .file_assets()
            .find_by_id(&asset.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        let rebind = sample_asset("fa-2", "obj/2025/08/abc123");
        let error = db
            .file_assets()
            .create(&rebind, &mut NoTransaction)
            .await
            .expect_err("软删除后对象键身份仍被占用，不得复用");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "软删除身份复用必须返回 DuplicateKey，实际为 {error:?}"
        );

        db.file_assets()
            .restore(&mut asset, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .file_assets()
            .find_by_id(&asset.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut asset = sample_asset("fa-1", "obj/2025/08/abc123");
        db.file_assets().create(&asset, &mut NoTransaction).await.unwrap();
        let mut stale = asset.clone();

        asset.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        db.file_assets()
            .update(&mut asset, &mut NoTransaction)
            .await
            .unwrap();

        stale.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        let error = db
            .file_assets()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn search_respects_pagination_and_projection_without_object_key() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.file_assets()
            .create(&sample_asset("fa-1", "obj/2025/08/a1"), &mut NoTransaction)
            .await
            .unwrap();
        let mut long_term = sample_asset("fa-2", "obj/2025/08/a2");
        long_term.retention_class = RetentionClass::LongTerm;
        long_term.expires_at = None;
        db.file_assets()
            .create(&long_term, &mut NoTransaction)
            .await
            .unwrap();

        let filter = FileAssetFilter {
            file_name: Some("导入清单".to_string()),
            security_scan_status: Some(SecurityScanStatus::Pending),
            retention_class: None,
            sensitivity_class: None,
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .file_assets()
            .search_file_assets(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "文件名模糊命中两条");
        assert_eq!(page.items.len(), 1, "第一页一条");
        let row = &page.items[0];
        assert_eq!(row.file_name, "导入清单.xlsx");
        assert_eq!(row.security_scan_status, SecurityScanStatus::Pending);
        assert_eq!(row.sensitivity_class, SensitivityClass::Sensitive);
        assert_eq!(row.byte_size, 2048);
        let debug = format!("{row:?}");
        assert!(!debug.contains("obj/2025"), "投影行 Debug 不得泄漏对象键");

        let second = FileAssetFilter {
            page: 2,
            page_size: 1,
            ..filter.clone()
        };
        let page_two = db
            .file_assets()
            .search_file_assets(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page_two.items.len(), 1, "第二页一条");
        let mut retention_classes = vec![page.items[0].retention_class, page_two.items[0].retention_class];
        retention_classes.sort_unstable_by_key(|class| class.as_str());
        assert_eq!(
            retention_classes,
            vec![RetentionClass::LongTerm, RetentionClass::ThirtyDays],
            "同一秒创建的两条资产顺序不确定，两页必须各占一个保留策略"
        );
        let mut expires_at = vec![page.items[0].expires_at, page_two.items[0].expires_at];
        expires_at.sort_unstable();
        assert_eq!(
            expires_at,
            vec![None, Some(1_703_260_800)],
            "长期保留无到期时间，30 天保留携带到期时间"
        );
    })
}

#[tokio::test]
#[ignore]
async fn attachment_lookup_by_document_and_asset() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_attach_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.file_assets()
            .create(&sample_asset("fa-1", "obj/2025/08/a1"), &mut NoTransaction)
            .await
            .unwrap();
        db.document_attachments()
            .create(&sample_attachment("da-1", "order-1", "fa-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.document_attachments()
            .create(&sample_attachment("da-2", "order-1", "fa-1"), &mut NoTransaction)
            .await
            .unwrap();

        let by_document = db
            .document_attachments()
            .list_by_document(&BusinessDocumentId::new("order-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_document.len(), 2, "按单据批量取回附件");
        assert_eq!(by_document[0].usage, AttachmentUsage::Attachment);
        assert_eq!(by_document[0].file_asset_id, FileAssetId::new("fa-1"));
    })
}

#[tokio::test]
#[ignore]
async fn attach_document_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let asset = sample_asset("fa-1", "obj/2025/08/a1");
        let attachment = sample_attachment("da-1", "order-1", "fa-1");

        let db_clone = db.clone();
        let asset_for_tx = asset.clone();
        let attachment_for_tx = attachment.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .file_asset()
                        .attach_document(&asset_for_tx, &attachment_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let asset_found = db
            .file_assets()
            .find_by_id(&asset.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(asset_found.is_some(), "事务提交后资产可见");
        let attachment_found = db
            .document_attachments()
            .find_by_id(&attachment.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(attachment_found.is_some(), "事务提交后关联可见");
    })
}

#[tokio::test]
#[ignore]
async fn attach_document_rolls_back_both_collections_on_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.file_assets()
            .create(&sample_asset("fa-0", "obj/2025/08/occupied"), &mut NoTransaction)
            .await
            .unwrap();

        let conflicting = sample_asset("fa-1", "obj/2025/08/occupied");
        let attachment = sample_attachment("da-1", "order-1", "fa-1");

        let db_clone = db.clone();
        let asset_for_tx = conflicting.clone();
        let attachment_for_tx = attachment.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .file_asset()
                        .attach_document(&asset_for_tx, &attachment_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "对象键冲突必须整体回滚并透出 DuplicateKey，实际为 {result:?}"
        );

        let attachment_found = db
            .document_attachments()
            .find_by_id(&attachment.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(attachment_found.is_none(), "冲突回滚后关联不得残留");
        let asset_found = db
            .file_assets()
            .find_by_id(&conflicting.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(asset_found.is_none(), "冲突回滚后资产不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn attach_document_no_transaction_leaves_partial_write() {
    require_mongo!(async {
        let test_db = TestDb::new("fileasset_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.document_attachments()
            .create(&sample_attachment("da-1", "order-0", "fa-0"), &mut NoTransaction)
            .await
            .unwrap();

        let asset = sample_asset("fa-1", "obj/2025/08/a1");
        let duplicated_attachment = sample_attachment("da-1", "order-1", "fa-1");
        let error = db
            .file_asset()
            .attach_document(&asset, &duplicated_attachment, &mut NoTransaction)
            .await
            .expect_err("第二笔写入冲突必须返回错误");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let asset_found = db
            .file_assets()
            .find_by_id(&asset.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            asset_found.is_some(),
            "NoTransaction 下第一笔已自动提交，留下半成品（方法注释已声明该行为）"
        );
    })
}
