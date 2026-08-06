//! 域 D09 `supplier` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test supplier_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use std::str::FromStr;

use database::repository::extensions::SupplierExt;
use database::{ensure_indexes, Error, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::field_update::FieldUpdate;
use entities::ids::{
    FileAssetId, PartyId, SupplierAccountId, SupplierCapabilityId, SupplierCommercialProfileRevisionId,
    SupplierQualificationCapabilityId, SupplierQualificationId,
};
use entities::money::Rate;
use entities::supplier::{
    CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
    ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
    SupplierAccountUpdate, SupplierCapability, SupplierCapabilityData, SupplierCommercialProfileRevision,
    SupplierCommercialProfileRevisionData, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationCapabilityData, SupplierQualificationData,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 供应商角色列表筛选条件类型（经 `SupplierExt` 关联类型跨 crate 可达）。
type SupplierAccountFilter = <Database as SupplierExt>::SupplierAccountFilter;
/// 商务结算版本列表筛选条件类型。
type SupplierCommercialProfileFilter = <Database as SupplierExt>::SupplierCommercialProfileFilter;
/// 能力列表筛选条件类型。
type SupplierCapabilityFilter = <Database as SupplierExt>::SupplierCapabilityFilter;
/// 资质列表筛选条件类型。
type SupplierQualificationFilter = <Database as SupplierExt>::SupplierQualificationFilter;

/// 构造可复用的供应商角色实体。
fn sample_supplier(party_id: &PartyId, supplier_no: &str) -> SupplierAccount {
    SupplierAccount::new(
        SupplierAccountId::new(format!("supplier-{supplier_no}")),
        SupplierAccountData {
            party_id: party_id.clone(),
            supplier_no: supplier_no.to_string(),
            default_payment_term_id: Some("net30".to_string()),
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的商务结算版本实体。
fn sample_commercial_profile(
    supplier_id: &SupplierAccountId,
    revision_no: u32,
) -> SupplierCommercialProfileRevision {
    SupplierCommercialProfileRevision::new(
        SupplierCommercialProfileRevisionId::new(format!("profile-{revision_no}")),
        SupplierCommercialProfileRevisionData {
            supplier_id: supplier_id.clone(),
            revision_no,
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "月结 30 天".to_string(),
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Rate::from_str("0.13").unwrap(),
            signing_entity_party_id: PartyId::new("party-ours"),
            payment_entity_party_id: PartyId::new("party-ours"),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            change_reason: "首次签约".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的能力实体。
fn sample_capability(supplier_id: &SupplierAccountId, code: CapabilityCode) -> SupplierCapability {
    SupplierCapability::new(
        SupplierCapabilityId::new(format!("cap-{}", code.as_str())),
        SupplierCapabilityData {
            supplier_id: supplier_id.clone(),
            capability_code: code,
            service_region: Some("华东".to_string()),
            owner_user_id: "sales-1".to_string(),
            fulfillment_note: Some("48 小时发货".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            status: CapabilityStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的资质实体。
fn sample_qualification(supplier_id: &SupplierAccountId, certificate_no: &str) -> SupplierQualification {
    SupplierQualification::new(
        SupplierQualificationId::new(format!("qual-{certificate_no}")),
        SupplierQualificationData {
            supplier_id: supplier_id.clone(),
            qualification_type: QualificationType::FoodLicense,
            certificate_no: certificate_no.to_string(),
            issuer: Some("市场监督管理局".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            attachment_id: Some(FileAssetId::new("file-1")),
            status: QualificationStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_ACCOUNTS,
        &[
            "uk_supplier_accounts_party",
            "uk_supplier_accounts_supplier_no",
            "idx_supplier_accounts_status",
        ],
    )
    .await
    .expect("supplier_accounts 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_COMMERCIAL_PROFILE_REVISIONS,
        &[
            "uk_supplier_commercial_profile_revisions_supplier_revision",
            "idx_supplier_commercial_profiles_history",
        ],
    )
    .await
    .expect("supplier_commercial_profile_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_CAPABILITIES,
        &[
            "uk_supplier_capabilities_supplier_code",
            "idx_supplier_capabilities_selection",
            "idx_supplier_capabilities_supplier_status",
        ],
    )
    .await
    .expect("supplier_capabilities 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_CAPABILITY_REVISIONS,
        &["uk_supplier_capability_revisions_identity"],
    )
    .await
    .expect("supplier_capability_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_QUALIFICATIONS,
        &[
            "uk_supplier_qualifications_identity",
            "idx_supplier_qualifications_expiry",
            "idx_supplier_qualifications_supplier",
        ],
    )
    .await
    .expect("supplier_qualifications 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_QUALIFICATION_REVISIONS,
        &["uk_supplier_qualification_revisions_identity"],
    )
    .await
    .expect("supplier_qualification_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_QUALIFICATION_CAPABILITIES,
        &["uk_supplier_qualification_capabilities_link"],
    )
    .await
    .expect("supplier_qualification_capabilities 索引缺失");
    assert_indexes(
        db,
        <Database as SupplierExt>::SUPPLIER_RATING_REVISIONS,
        &[
            "uk_supplier_rating_revisions_supplier_revision",
            "idx_supplier_rating_revisions_history",
        ],
    )
    .await
    .expect("supplier_rating_revisions 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-1");
        let mut supplier = sample_supplier(&party_id, "S-2026-001");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(supplier.base.version, 1);

        let found = db
            .supplier_accounts()
            .find_by_id(&supplier.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.supplier_no, "S-2026-001");
        assert_eq!(found.party_id, party_id);
        assert_eq!(found.stable.created_by, "admin-1");

        supplier
            .update(
                SupplierAccountUpdate {
                    default_payment_term_id: FieldUpdate::Set("net60".to_string()),
                    current_commercial_profile_revision_id: FieldUpdate::Unchanged,
                    status: Some(SupplierAccountStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.supplier_accounts()
            .update(&mut supplier, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(supplier.base.version, 2, "乐观锁成功后 version 递增");

        db.supplier_accounts()
            .soft_delete(&mut supplier, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .supplier_accounts()
            .find_by_id(&supplier.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.supplier_accounts()
            .restore(&mut supplier, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .supplier_accounts()
            .find_by_id(&supplier.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_optlock").await.unwrap();
        let db = test_db.db();

        let party_id = PartyId::new("party-supp-2");
        let mut supplier = sample_supplier(&party_id, "S-2026-002");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = supplier.clone();
        supplier
            .update(
                SupplierAccountUpdate {
                    default_payment_term_id: FieldUpdate::Unchanged,
                    current_commercial_profile_revision_id: FieldUpdate::Unchanged,
                    status: Some(SupplierAccountStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.supplier_accounts()
            .update(&mut supplier, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                SupplierAccountUpdate {
                    default_payment_term_id: FieldUpdate::Unchanged,
                    current_commercial_profile_revision_id: FieldUpdate::Unchanged,
                    status: None,
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .supplier_accounts()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn unique_supplier_no_and_party_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-3");
        let supplier = sample_supplier(&party_id, "S-2026-003");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();

        let dup_no = sample_supplier(&PartyId::new("party-supp-4"), "S-2026-003");
        let error = db
            .supplier_accounts()
            .create(&dup_no, &mut NoTransaction)
            .await
            .expect_err("重复 supplier_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let dup_party = sample_supplier(&party_id, "S-2026-004");
        let error = db
            .supplier_accounts()
            .create(&dup_party, &mut NoTransaction)
            .await
            .expect_err("同一 party 第二个供应商角色必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn commercial_profile_revision_identity_is_unique() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_profile_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-5");
        let supplier = sample_supplier(&party_id, "S-2026-005");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();
        let supplier_id: SupplierAccountId = supplier.base.id.clone().into();

        let profile = sample_commercial_profile(&supplier_id, 1);
        db.supplier_commercial_profile_revisions()
            .create(&profile, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_commercial_profile(&supplier_id, 1);
        let error = db
            .supplier_commercial_profile_revisions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 (supplier_id, revision_no) 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let second = sample_commercial_profile(&supplier_id, 2);
        db.supplier_commercial_profile_revisions()
            .create(&second, &mut NoTransaction)
            .await
            .unwrap();
        let history = db
            .supplier_commercial_profile_revisions()
            .list_revision_history(&supplier_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(history.len(), 2, "商务版本为追加式历史");
        assert_eq!(history[0].revision.revision_no, 1);

        let filter = SupplierCommercialProfileFilter {
            supplier_id: Some(supplier_id.clone()),
            page: 1,
            page_size: 20,
            sort_by: Some("revision_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_commercial_profile_revisions()
            .search_commercial_profiles(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "商务版本列表按供应商筛选");
        assert_eq!(page.items[0].revision_no, 1);
        assert_eq!(page.items[0].settlement_mode, "prepayment");
        assert_eq!(page.items[0].reconciliation_cycle, "monthly");
    })
}

#[tokio::test]
#[ignore]
async fn capability_and_qualification_identity_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_cap_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-6");
        let supplier = sample_supplier(&party_id, "S-2026-006");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();
        let supplier_id: SupplierAccountId = supplier.base.id.clone().into();

        let capability = sample_capability(&supplier_id, CapabilityCode::Physical);
        db.supplier_capabilities()
            .create(&capability, &mut NoTransaction)
            .await
            .unwrap();
        let dup_capability = sample_capability(&supplier_id, CapabilityCode::Physical);
        let error = db
            .supplier_capabilities()
            .create(&dup_capability, &mut NoTransaction)
            .await
            .expect_err("重复 (supplier_id, capability_code) 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let qualification = sample_qualification(&supplier_id, "FS-2026-001");
        db.supplier_qualifications()
            .create(&qualification, &mut NoTransaction)
            .await
            .unwrap();
        let dup_qualification = sample_qualification(&supplier_id, "FS-2026-001");
        let error = db
            .supplier_qualifications()
            .create(&dup_qualification, &mut NoTransaction)
            .await
            .expect_err("重复 (供应商, 资质类型, 证书编号) 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn qualification_capability_link_unique_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_link_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-7");
        let supplier = sample_supplier(&party_id, "S-2026-007");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();
        let supplier_id: SupplierAccountId = supplier.base.id.clone().into();
        let capability = sample_capability(&supplier_id, CapabilityCode::Physical);
        db.supplier_capabilities()
            .create(&capability, &mut NoTransaction)
            .await
            .unwrap();
        let qualification = sample_qualification(&supplier_id, "FS-2026-007");
        db.supplier_qualifications()
            .create(&qualification, &mut NoTransaction)
            .await
            .unwrap();

        let link = SupplierQualificationCapability::new(
            SupplierQualificationCapabilityId::new("link-1"),
            SupplierQualificationCapabilityData {
                qualification_id: qualification.base.id.clone().into(),
                capability_id: capability.base.id.clone().into(),
            },
        )
        .unwrap();
        db.supplier_qualification_capabilities()
            .create(&link, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = SupplierQualificationCapability::new(
            SupplierQualificationCapabilityId::new("link-2"),
            SupplierQualificationCapabilityData {
                qualification_id: qualification.base.id.clone().into(),
                capability_id: capability.base.id.clone().into(),
            },
        )
        .unwrap();
        let error = db
            .supplier_qualification_capabilities()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复资质-能力关联必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn create_supplier_with_initial_profile_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-8");
        let supplier = sample_supplier(&party_id, "S-2026-008");
        let supplier_id: SupplierAccountId = supplier.base.id.clone().into();
        let profile = sample_commercial_profile(&supplier_id, 1);

        let db_clone = db.clone();
        let supplier_for_tx = supplier.clone();
        let profile_for_tx = profile.clone();
        test_db
            .client()
            .with_transaction::<_, (), Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier()
                        .create_supplier_with_initial_profile(&supplier_for_tx, &profile_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let supplier_found = db
            .supplier_accounts()
            .find_by_id(&supplier.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("事务提交后供应商角色必须可见");
        assert_eq!(supplier_found.supplier_no, "S-2026-008");
        let profile_found = db
            .supplier_commercial_profile_revisions()
            .find_by_supplier_and_revision(&supplier_id, 1, &mut NoTransaction)
            .await
            .unwrap()
            .expect("事务提交后商务版本必须可见");
        assert_eq!(profile_found.invoice_tax_rate, Rate::from_str("0.13").unwrap());
    })
}

#[tokio::test]
#[ignore]
async fn create_supplier_with_initial_profile_rolls_back_on_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-9");
        let existing = sample_supplier(&party_id, "S-2026-009");
        db.supplier_accounts()
            .create(&existing, &mut NoTransaction)
            .await
            .unwrap();

        let conflicting = sample_supplier(&PartyId::new("party-supp-10"), "S-2026-009");
        let conflicting_id: SupplierAccountId = conflicting.base.id.clone().into();
        let profile = sample_commercial_profile(&conflicting_id, 1);

        let db_clone = db.clone();
        let conflicting_for_tx = conflicting.clone();
        let profile_for_tx = profile.clone();
        let result: std::result::Result<(), Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .supplier()
                        .create_supplier_with_initial_profile(&conflicting_for_tx, &profile_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(Error::DuplicateKey(_))),
            "重复 supplier_no 触发唯一索引冲突，事务必须回滚，实际为 {result:?}"
        );

        let profile_found = db
            .supplier_commercial_profile_revisions()
            .find_by_supplier_and_revision(&conflicting_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(profile_found.is_none(), "回滚后商务版本不得残留");
        let supplier_found = db
            .supplier_accounts()
            .find_by_party(&conflicting.party_id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(supplier_found.is_none(), "回滚后供应商角色不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn create_supplier_with_initial_profile_no_transaction_is_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_tx_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-11");
        let existing = sample_supplier(&party_id, "S-2026-011");
        db.supplier_accounts()
            .create(&existing, &mut NoTransaction)
            .await
            .unwrap();

        let conflicting = sample_supplier(&PartyId::new("party-supp-12"), "S-2026-011");
        let conflicting_id: SupplierAccountId = conflicting.base.id.clone().into();
        let profile = sample_commercial_profile(&conflicting_id, 1);

        let error = db
            .supplier()
            .create_supplier_with_initial_profile(&conflicting, &profile, &mut NoTransaction)
            .await
            .expect_err("NoTransaction 下写入冲突必须透出错误");
        assert!(matches!(error, Error::DuplicateKey(_)));

        let profile_found = db
            .supplier_commercial_profile_revisions()
            .find_by_supplier_and_revision(&conflicting_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            profile_found.is_some(),
            "NoTransaction 下版本已单独提交（非原子，行为可预期）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn supplier_list_projection_pagination_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.supplier_accounts()
            .create(
                &sample_supplier(&PartyId::new("party-supp-14"), "S-2026-014"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let mut disabled = sample_supplier(&PartyId::new("party-supp-13"), "S-2026-013");
        disabled.stable.status = SupplierAccountStatus::Disabled;
        db.supplier_accounts()
            .create(&disabled, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierAccountFilter {
            keyword: Some("2026-01".to_string()),
            party_id: None,
            status: Some(SupplierAccountStatus::Active),
            page: 1,
            page_size: 1,
            sort_by: Some("supplier_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_accounts()
            .search_supplier_accounts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "启用且编号含 2026-01 只有一条");
        let row = &page.items[0];
        assert_eq!(row.supplier_no, "S-2026-014");
        assert_eq!(row.status, SupplierAccountStatus::Active);
        assert_eq!(row.default_payment_term_id.as_deref(), Some("net30"));
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let fallback = SupplierAccountFilter {
            keyword: None,
            party_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("not_whitelisted".to_string()),
            sort_ascending: false,
        };
        let page = db
            .supplier_accounts()
            .search_supplier_accounts(&fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "白名单外排序字段回落默认 created_at 降序");
        assert_eq!(
            page.items[0].supplier_no, "S-2026-013",
            "created_at 降序首条为最新创建"
        );
    })
}

#[tokio::test]
#[ignore]
async fn capability_and_qualification_search_with_expiry_warning() {
    require_mongo!(async {
        let test_db = TestDb::new("supp_expiry").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-supp-15");
        let supplier = sample_supplier(&party_id, "S-2026-015");
        db.supplier_accounts()
            .create(&supplier, &mut NoTransaction)
            .await
            .unwrap();
        let supplier_id: SupplierAccountId = supplier.base.id.clone().into();

        db.supplier_capabilities()
            .create(
                &sample_capability(&supplier_id, CapabilityCode::Physical),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let mut virtual_cap = sample_capability(&supplier_id, CapabilityCode::Virtual);
        virtual_cap.stable.status = CapabilityStatus::Disabled;
        db.supplier_capabilities()
            .create(&virtual_cap, &mut NoTransaction)
            .await
            .unwrap();

        let capability_filter = SupplierCapabilityFilter {
            supplier_id: Some(supplier_id.clone()),
            capability_code: None,
            status: Some(CapabilityStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: Some("capability_code".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_capabilities()
            .search_supplier_capabilities(&capability_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "停用的虚拟能力被状态筛选排除");
        assert_eq!(page.items[0].capability_code, CapabilityCode::Physical);
        assert_eq!(page.items[0].owner_user_id, "sales-1");

        let warning = db
            .supplier_capabilities()
            .list_active_for_expiry_warning(CapabilityCode::Physical, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(warning.len(), 1, "启用能力进入到期预警列表");

        db.supplier_qualifications()
            .create(
                &sample_qualification(&supplier_id, "FS-2026-015"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let qualification_filter = SupplierQualificationFilter {
            supplier_id: Some(supplier_id),
            qualification_type: Some(QualificationType::FoodLicense),
            status: Some(QualificationStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: Some("valid_to".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_qualifications()
            .search_supplier_qualifications(&qualification_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].certificate_no, "FS-2026-015");
        assert_eq!(page.items[0].valid_to.as_deref(), Some("2026-12-31"));

        let warning = db
            .supplier_qualifications()
            .list_active_for_expiry_warning(&mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(warning.len(), 1, "有效资质进入到期预警列表");
    })
}
