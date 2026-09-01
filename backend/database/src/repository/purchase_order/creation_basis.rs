//! 采购创建依据批量事实加载（PROC-R08）。
//!
//! 一次批量返回任务涉及 SKU 的 ACTIVE 供给、供给当前商业条款修订、实时可供
//! 投影、供应商角色、当前商务资料修订与当前法定名称。合格性筛选（条款有效期、
//! AVAILABLE、零库存、每供应商稳定选一条）、任务归属、RBAC 与事务内重算由
//! Service 基于本事实解释执行；本模块只做持久化读取，查询次数与任务数、销售
//! 行数及供给数无关，不得出现逐行 N+1。

use entities::ids::{SkuId, SupplierAccountId, SupplierOfferingId};
use entities::purchase_order::CreationBasisFacts;
use entities::supplier_offering::OfferingStatus;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::Database;

use crate::executor::Executor;
use crate::repository::extensions::{SupplierExt, SupplierOfferingExt};
use crate::Result;

/// 批量加载采购创建依据计算所需的最小持久化事实。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `sku_ids` - 任务责任范围内仍有剩余量的销售目标行 SKU 集合
/// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回 ACTIVE 供给、当前修订、可供投影、供应商结算事实与法定名称；SKU 集合
/// 为空时返回空事实集合。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误；供给修订、可供投影或供应商关联缺失
/// 时以缺键形式表达，由 Service 按合格性语义解释。
///
/// # 约束
/// 查询次数与输入规模无关：供给、修订、可供投影、供应商、商务资料修订与法定
/// 名称各一次批量读取，不得出现逐行 N+1。供给只读取 ACTIVE 且未删除行，并按
/// SKU、供应商与供给 ID 稳定排序，保证每供应商稳定选一条的语义与逐 SKU 查询
/// 完全一致。
pub async fn load_creation_basis_facts(
    db: &Database,
    sku_ids: &[SkuId],
    executor: &mut dyn Executor,
) -> Result<CreationBasisFacts> {
    let sku_ids = unique_sku_ids(sku_ids);
    if sku_ids.is_empty() {
        return Ok(CreationBasisFacts::default());
    }
    let offerings = db
        .supplier_offerings()
        .find_many_sorted(
            doc! {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "sku_id": { "$in": sku_ids.iter().map(ToString::to_string).collect::<Vec<_>>() },
                "status": OfferingStatus::Active.as_str(),
            },
            doc! { "sku_id": 1, "supplier_id": 1, "id": 1 },
            executor,
        )
        .await?;
    let revision_ids = offerings
        .iter()
        .filter_map(|offering| offering.stable.current_revision_id.clone())
        .collect::<Vec<_>>();
    let revisions = db
        .supplier_offering_revisions()
        .list_by_ids(&revision_ids, executor)
        .await?;
    let offering_ids = offerings
        .iter()
        .map(|offering| SupplierOfferingId::new(offering.base.id.clone()))
        .collect::<Vec<_>>();
    let availabilities = db
        .supplier_offering_availabilities()
        .find_by_offering_ids(&offering_ids, executor)
        .await?;
    let supplier_ids = unique_supplier_ids(&offerings);
    let suppliers = db
        .supplier_accounts()
        .find_accounts_by_ids(&supplier_ids, executor)
        .await?;
    let profile_revision_ids = suppliers
        .iter()
        .filter_map(|supplier| supplier.current_commercial_profile_revision_id.clone())
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let commercial_profiles = db
        .supplier()
        .list_commercial_profiles_by_ids(&profile_revision_ids, executor)
        .await?;
    let supplier_names = db
        .supplier()
        .current_legal_names_by_account_ids(&supplier_ids, executor)
        .await?;
    Ok(CreationBasisFacts {
        revisions: revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect(),
        availabilities: availabilities
            .into_iter()
            .map(|availability| (availability.supplier_offering_id.to_string(), availability))
            .collect(),
        suppliers: suppliers
            .into_iter()
            .map(|supplier| (supplier.base.id.clone(), supplier))
            .collect(),
        commercial_profiles: commercial_profiles
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect(),
        supplier_names,
        offerings,
    })
}

/// 去重 SKU 集合且保持首次出现顺序。
///
/// # 参数
/// * `sku_ids` - 原始 SKU 集合，允许重复
///
/// # 返回
/// 返回无重复且顺序稳定的 SKU 集合。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重只用于缩小 `$in` 范围，不改变任何业务语义。
fn unique_sku_ids(sku_ids: &[SkuId]) -> Vec<SkuId> {
    let mut seen = std::collections::HashSet::new();
    sku_ids
        .iter()
        .filter(|sku_id| seen.insert(sku_id.to_string()))
        .cloned()
        .collect()
}

/// 提取供给涉及的供应商集合且保持首次出现顺序。
///
/// # 参数
/// * `offerings` - 已按供应商稳定排序的供给
///
/// # 返回
/// 返回无重复的供应商主键集合。
///
/// # 错误
/// 无。
///
/// # 约束
/// 供应商集合只用于批量读取结算事实，不承担任何业务过滤。
fn unique_supplier_ids(
    offerings: &[entities::supplier_offering::SupplierOffering],
) -> Vec<SupplierAccountId> {
    let mut seen = std::collections::HashSet::new();
    offerings
        .iter()
        .filter(|offering| seen.insert(offering.supplier_id.to_string()))
        .map(|offering| offering.supplier_id.clone())
        .collect()
}

#[cfg(test)]
mod isolation_tests {
    use std::str::FromStr;

    use entities::common::time::Instant;
    use entities::ids::{
        PartyId, SkuId, SupplierAccountId, SupplierCommercialProfileRevisionId,
        SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId,
    };
    use entities::money::{Quantity, Rate, UnitPrice};
    use entities::party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
    use entities::supplier::{
        InvoiceType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData,
        SupplierAccountStatus, SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
    };
    use entities::supplier_offering::{
        AvailabilityStatus, OfferingSourceType, OfferingStatus, PrefillSourceRefs, SupplierOffering,
        SupplierOfferingAvailability, SupplierOfferingAvailabilityData, SupplierOfferingData,
        SupplierOfferingRevision, SupplierOfferingRevisionData,
    };
    use test_support::{require_mongo, TestDb};

    use crate::ensure_indexes;
    use crate::repository::extensions::{PartyExt, SupplierExt, SupplierOfferingExt};
    use crate::{NoTransaction, Transactional};

    use super::load_creation_basis_facts;

    /// 构造供给稳定身份。
    fn offering(id: &str, sku_id: &str, supplier_id: &str) -> SupplierOffering {
        SupplierOffering::new(
            SupplierOfferingId::new(id),
            SupplierOfferingData {
                sku_id: SkuId::new(sku_id),
                supplier_id: SupplierAccountId::new(supplier_id),
                supplier_product_code: None,
                supplier_sku_code: format!("SKU-{id}"),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造供给商业条款修订。
    fn offering_revision(id: &str, offering_id: &str) -> SupplierOfferingRevision {
        SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new(id),
            SupplierOfferingRevisionData::from_gross_prices(
                SupplierOfferingId::new(offering_id),
                1,
                UnitPrice::from_str("6").unwrap(),
                UnitPrice::from_str("5").unwrap(),
                Rate::from_str("0.13").unwrap(),
                None,
                None,
                None,
                Quantity::from_str("1").unwrap(),
                vec!["全国".to_string()],
                Vec::new(),
                entities::common::time::BusinessDate::from_str("2026-01-01").unwrap(),
                None,
                PrefillSourceRefs {
                    input_tax_rate: None,
                    supply_region: None,
                    valid_from_date: None,
                    valid_from_timezone: None,
                    valid_from_calendar_version: None,
                },
            ),
        )
        .unwrap()
    }

    /// 构造供给实时可供投影。
    fn availability(id: &str, offering_id: &str) -> SupplierOfferingAvailability {
        SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new(id),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: SupplierOfferingId::new(offering_id),
                availability_status: AvailabilityStatus::Available,
                available_quantity: Some(Quantity::from_str("8").unwrap()),
                source_updated_at: Instant::from_unix_secs(1_800_000_000),
                received_at: Instant::from_unix_secs(1_800_000_000),
                source_revision_token: None,
                updated_by: "test".to_string(),
            },
        )
        .unwrap()
    }

    /// 构造供应商角色与当前商务资料修订。
    fn supplier(id: &str, party_id: &str) -> SupplierAccount {
        SupplierAccount::new(
            SupplierAccountId::new(id),
            SupplierAccountData {
                party_id: PartyId::new(party_id),
                supplier_no: format!("SN-{id}"),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: None,
                status: SupplierAccountStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造商务资料修订。
    fn commercial_profile(id: &str, supplier_id: &str) -> SupplierCommercialProfileRevision {
        SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new(id),
            SupplierCommercialProfileRevisionData {
                supplier_id: SupplierAccountId::new(supplier_id),
                revision_no: 1,
                settlement_mode: SettlementMode::PayAfterUse,
                reconciliation_cycle: ReconciliationCycle::Monthly,
                payment_term_snapshot: "NET-30".to_string(),
                business_category: Some("经营类目".to_string()),
                invoice_type: InvoiceType::VatSpecial,
                invoice_tax_rate: Rate::from_str("0.13").unwrap(),
                signing_entity_party_id: PartyId::new("party-a"),
                payment_entity_party_id: PartyId::new("party-a"),
                change_reason: "初始登记".to_string(),
            },
        )
        .unwrap()
    }

    /// 构造企业主体及其当前修订。
    fn party(id: &str, revision_id: &str) -> (Party, PartyRevision) {
        let mut party = Party::new(
            PartyId::new(id),
            PartyData {
                party_no: format!("P-{id}"),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "test",
        )
        .unwrap();
        let revision = PartyRevision::new(
            entities::ids::PartyRevisionId::new(revision_id),
            PartyRevisionData {
                party_id: PartyId::new(id),
                revision_no: 1,
                legal_name: "供应商名称".to_string(),
                short_name: None,
                change_reason: "初始登记".to_string(),
            },
        )
        .unwrap();
        party.stable.current_revision_id = Some(revision_id.to_string());
        (party, revision)
    }

    /// 写入两 SKU、两供应商的完整创建依据夹具。
    async fn seed_creation_basis_fixture(db: &mongodb::Database) {
        // SKU-1 → 供应商 A（offering-1）与供应商 B（offering-2）；SKU-2 → 供应商 A（offering-3）。
        let (party_a, party_a_revision) = party("party-a", "partyrev-a");
        let (party_b, party_b_revision) = party("party-b", "partyrev-b");
        db.parties()
            .create(&party_a, &mut NoTransaction)
            .await
            .expect("主体写入失败");
        db.party_revisions()
            .create(&party_a_revision, &mut NoTransaction)
            .await
            .expect("主体修订写入失败");
        db.parties()
            .create(&party_b, &mut NoTransaction)
            .await
            .expect("主体写入失败");
        db.party_revisions()
            .create(&party_b_revision, &mut NoTransaction)
            .await
            .expect("主体修订写入失败");
        let mut supplier_a = supplier("sup-a", "party-a");
        supplier_a.current_commercial_profile_revision_id =
            Some(SupplierCommercialProfileRevisionId::new("profile-a"));
        let mut supplier_b = supplier("sup-b", "party-b");
        supplier_b.current_commercial_profile_revision_id =
            Some(SupplierCommercialProfileRevisionId::new("profile-b"));
        db.supplier_accounts()
            .create(&supplier_a, &mut NoTransaction)
            .await
            .expect("供应商写入失败");
        db.supplier_accounts()
            .create(&supplier_b, &mut NoTransaction)
            .await
            .expect("供应商写入失败");
        db.supplier_commercial_profile_revisions()
            .create(&commercial_profile("profile-a", "sup-a"), &mut NoTransaction)
            .await
            .expect("商务资料写入失败");
        db.supplier_commercial_profile_revisions()
            .create(&commercial_profile("profile-b", "sup-b"), &mut NoTransaction)
            .await
            .expect("商务资料写入失败");
        for (offering_id, sku_id, supplier_id) in [
            ("offering-1", "sku-1", "sup-a"),
            ("offering-2", "sku-1", "sup-b"),
            ("offering-3", "sku-2", "sup-a"),
        ] {
            let mut offering = offering(offering_id, sku_id, supplier_id);
            offering.stable.current_revision_id = Some(format!("offrev-{offering_id}"));
            db.supplier_offerings()
                .create(&offering, &mut NoTransaction)
                .await
                .expect("供给写入失败");
            db.supplier_offering_revisions()
                .create(
                    &offering_revision(&format!("offrev-{offering_id}"), offering_id),
                    &mut NoTransaction,
                )
                .await
                .expect("供给修订写入失败");
            db.supplier_offering_availabilities()
                .create(
                    &availability(&format!("avail-{offering_id}"), offering_id),
                    &mut NoTransaction,
                )
                .await
                .expect("可供投影写入失败");
        }
    }

    /// 空 SKU 集合直接返回空事实集合，不发任何查询。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言各事实集合全部为空。
    ///
    /// # 错误
    /// MongoDB 连接或事实加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 维度：空输入直接短路，不允许空 `$in` 查询。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn empty_sku_ids_return_empty_facts() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_basis_empty_skus")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let facts = load_creation_basis_facts(fixture.db(), &[], &mut NoTransaction)
                .await
                .expect("事实加载失败");
            assert!(facts.offerings.is_empty());
            assert!(facts.revisions.is_empty());
            assert!(facts.availabilities.is_empty());
            assert!(facts.suppliers.is_empty());
            assert!(facts.commercial_profiles.is_empty());
            assert!(facts.supplier_names.is_empty());
        });
    }

    /// 一次批量返回全部 SKU 的 ACTIVE 供给与关联结算事实。
    ///
    /// # 参数
    /// 无，内部创建隔离库并写入完整夹具。
    ///
    /// # 返回
    /// 断言供给按 SKU/供应商稳定排序且修订、投影、供应商与名称齐备。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入或事实加载失败时测试失败。
    ///
    /// # 约束
    /// Batch 维度：一次调用返回全部来源，查询次数与行数无关。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn loads_active_offerings_and_settlement_facts_in_one_batch() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_basis_facts").await.expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_creation_basis_fixture(fixture.db()).await;

            let facts = load_creation_basis_facts(
                fixture.db(),
                &[SkuId::new("sku-1"), SkuId::new("sku-2")],
                &mut NoTransaction,
            )
            .await
            .expect("事实加载失败");
            assert_eq!(facts.offerings.len(), 3, "全部 ACTIVE 供给应被加载");
            assert_eq!(
                facts.offerings[0].sku_id.to_string(),
                "sku-1",
                "供给应按 SKU 稳定排序"
            );
            assert_eq!(
                facts.offerings[0].supplier_id.to_string(),
                "sup-a",
                "同 SKU 内应按供应商稳定排序"
            );
            assert_eq!(facts.offerings[1].supplier_id.to_string(), "sup-b");
            assert_eq!(facts.offerings[2].sku_id.to_string(), "sku-2");
            assert_eq!(facts.revisions.len(), 3, "全部当前修订应被加载");
            assert_eq!(facts.availabilities.len(), 3, "全部可供投影应被加载");
            assert_eq!(facts.suppliers.len(), 2, "涉及供应商应被加载");
            assert_eq!(facts.commercial_profiles.len(), 2, "商务资料修订应被加载");
            assert_eq!(facts.supplier_names.len(), 2, "法定名称应被加载");
            assert_eq!(
                facts.commercial_profiles["profile-a"].effective_payment_term_code(),
                "POSTPAY_NET30"
            );
            assert_eq!(
                facts.commercial_profiles["profile-a"]
                    .effective_business_category()
                    .as_deref(),
                Some("经营类目")
            );
        });
    }

    /// 停用供给及其修订、投影不进入事实集合。
    ///
    /// # 参数
    /// 无，内部创建隔离库并写入含停用供给的夹具。
    ///
    /// # 返回
    /// 断言停用供给被过滤，其余事实不受影响。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 软删除与停用过滤由 Repository 查询条件保证。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn inactive_offerings_are_excluded() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_basis_inactive")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_creation_basis_fixture(fixture.db()).await;
            let mut inactive = offering("offering-inactive", "sku-1", "sup-a");
            inactive.stable.status = OfferingStatus::Paused;
            fixture
                .db()
                .supplier_offerings()
                .create(&inactive, &mut NoTransaction)
                .await
                .expect("停用供给写入失败");

            let facts = load_creation_basis_facts(fixture.db(), &[SkuId::new("sku-1")], &mut NoTransaction)
                .await
                .expect("事实加载失败");
            assert_eq!(facts.offerings.len(), 2, "停用供给不得进入事实集合");
            assert!(facts
                .offerings
                .iter()
                .all(|offering| offering.base.id != "offering-inactive"));
        });
    }

    /// 事务内调用复用调用方 session，读取自身未提交写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 断言事务内可读到同一 session 刚写入的供给事实。
    ///
    /// # 错误
    /// MongoDB 连接、夹具写入、事务或事实加载失败时测试失败。
    ///
    /// # 约束
    /// 事务内重验必须复用调用方 executor，不得另开连接或独立事务。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn transaction_reads_own_writes_with_same_session() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_basis_txn").await.expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_creation_basis_fixture(fixture.db()).await;

            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    Box::pin(async move {
                        let mut fresh = offering("offering-txn", "sku-1", "sup-a");
                        fresh.stable.current_revision_id = Some("offrev-txn".to_string());
                        db.supplier_offerings().create(&fresh, session).await?;
                        db.supplier_offering_revisions()
                            .create(&offering_revision("offrev-txn", "offering-txn"), session)
                            .await?;
                        db.supplier_offering_availabilities()
                            .create(&availability("avail-txn", "offering-txn"), session)
                            .await?;
                        let facts = load_creation_basis_facts(&db, &[SkuId::new("sku-1")], session).await?;
                        assert!(
                            facts
                                .offerings
                                .iter()
                                .any(|offering| offering.base.id == "offering-txn"),
                            "事务内应能 read-your-writes"
                        );
                        assert!(facts.revisions.contains_key("offrev-txn"));
                        assert!(facts.availabilities.contains_key("offering-txn"));
                        Ok(())
                    })
                })
                .await
                .expect("事务内事实加载失败");
        });
    }
}
