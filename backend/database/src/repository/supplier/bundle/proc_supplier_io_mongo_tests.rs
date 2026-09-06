use std::str::FromStr;

use entities::common::time::BusinessDate;
use entities::ids::{
    PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId, SupplierCommercialProfileRevisionId,
    SupplierQualificationCapabilityId, SupplierQualificationId, SupplierRatingRevisionId,
};
use entities::money::Rate;
use entities::party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
use entities::supplier::{
    CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
    ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
    SupplierCapability, SupplierCapabilityData, SupplierCommercialProfileRevision,
    SupplierCommercialProfileRevisionData, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationCapabilityData, SupplierQualificationData, SupplierRating, SupplierRatingRevision,
    SupplierRatingRevisionData,
};
use mongodb::bson::doc;
use test_support::{require_mongo, TestDb};

use super::super::{SUPPLIER_ACCOUNTS, SUPPLIER_CAPABILITIES};
use super::{SupplierListSearchInput, SupplierQualificationHealthFilter};
use crate::{ensure_indexes, NoTransaction, PartyExt, SupplierExt, Transactional};

/// 列表与详情验收的业务日。
const AS_OF: &str = "2026-08-31";

/// 构造带当前修订的主体。
///
/// # 参数
/// * `id` - 主体稳定 ID
/// * `revision_id` - 当前修订 ID
/// * `legal_name` - 法定名称
///
/// # 返回
/// 返回主体与其当前修订。
fn party_with_revision(id: &str, revision_id: &str, legal_name: &str) -> (Party, PartyRevision) {
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
    .expect("主体构造失败");
    let revision = PartyRevision::new(
        PartyRevisionId::new(revision_id),
        PartyRevisionData {
            party_id: PartyId::new(id),
            revision_no: 1,
            legal_name: legal_name.to_string(),
            short_name: None,
            change_reason: "初始登记".to_string(),
        },
    )
    .expect("主体修订构造失败");
    party.stable.current_revision_id = Some(revision_id.to_string());
    (party, revision)
}

/// 构造供应商角色。
///
/// # 参数
/// * `id` - 供应商稳定 ID
/// * `party_id` - 所属主体 ID
/// * `supplier_no` - 供应商编号
/// * `profile_id` - 当前商务资料 ID；`None` 表示无当前指针
///
/// # 返回
/// 返回未删除的启用供应商角色。
fn supplier_account(
    id: &str,
    party_id: &str,
    supplier_no: &str,
    profile_id: Option<&str>,
) -> SupplierAccount {
    SupplierAccount::new(
        SupplierAccountId::new(id),
        SupplierAccountData {
            party_id: PartyId::new(party_id),
            supplier_no: supplier_no.to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: profile_id.map(SupplierCommercialProfileRevisionId::new),
            status: SupplierAccountStatus::Active,
        },
        "test",
    )
    .expect("供应商角色构造失败")
}

/// 构造首版商务资料。
///
/// # 参数
/// * `id` - 修订 ID
/// * `supplier_id` - 所属供应商 ID
/// * `party_id` - 签约与付款主体 ID
///
/// # 返回
/// 返回修订号为 1 的商务资料。
fn commercial_profile(id: &str, supplier_id: &str, party_id: &str) -> SupplierCommercialProfileRevision {
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
            signing_entity_party_id: PartyId::new(party_id),
            payment_entity_party_id: PartyId::new(party_id),
            change_reason: "初始登记".to_string(),
        },
    )
    .expect("商务资料构造失败")
}

/// 构造长期有效的启用能力。
///
/// # 参数
/// * `id` - 能力稳定 ID
/// * `supplier_id` - 所属供应商 ID
/// * `code` - 能力代码
///
/// # 返回
/// 返回自 2026-01-01 生效的启用能力。
fn capability(id: &str, supplier_id: &str, code: CapabilityCode) -> SupplierCapability {
    SupplierCapability::new(
        SupplierCapabilityId::new(id),
        SupplierCapabilityData {
            supplier_id: SupplierAccountId::new(supplier_id),
            capability_code: code,
            service_region: None,
            owner_user_id: "test".to_string(),
            fulfillment_note: None,
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            status: CapabilityStatus::Active,
        },
        "test",
    )
    .expect("供应商能力构造失败")
}

/// 构造资质。
///
/// # 参数
/// * `id` - 资质稳定 ID
/// * `supplier_id` - 所属供应商 ID
/// * `qualification_type` - 资质类型
/// * `certificate_no` - 证书编号
/// * `valid_to` - 失效日；`None` 表示长期有效
///
/// # 返回
/// 返回自 2026-01-01 生效的启用资质。
fn qualification(
    id: &str,
    supplier_id: &str,
    qualification_type: QualificationType,
    certificate_no: &str,
    valid_to: Option<BusinessDate>,
) -> SupplierQualification {
    SupplierQualification::new(
        SupplierQualificationId::new(id),
        SupplierQualificationData {
            supplier_id: SupplierAccountId::new(supplier_id),
            qualification_type,
            certificate_no: certificate_no.to_string(),
            issuer: None,
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to,
            attachment_id: None,
            status: QualificationStatus::Active,
        },
        "test",
    )
    .expect("供应商资质构造失败")
}

/// 写入列表与详情验收夹具。
///
/// sup-a（Physical 能力 + 10 天内到期的有效食品资质 + 第二份合同资质 +
/// 评级 + 商务资料）；sup-b（Physical 能力，无资质）；sup-c（Api 能力 +
/// 已过期食品资质）；sup-orphan（主体缺失）。
///
/// # 参数
/// * `db` - 隔离测试库
///
/// # 错误
/// 任一夹具写入失败时 panic。
async fn seed_supplier_io_fixture(db: &mongodb::Database) {
    for (party_id, revision_id, legal_name) in [
        ("party-a", "partyrev-a", "供应商甲"),
        ("party-b", "partyrev-b", "供应商乙"),
        ("party-c", "partyrev-c", "供应商丙"),
    ] {
        let (party, revision) = party_with_revision(party_id, revision_id, legal_name);
        db.parties()
            .create(&party, &mut NoTransaction)
            .await
            .expect("主体写入失败");
        db.party_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .expect("主体修订写入失败");
    }
    for (id, party_id, supplier_no, profile_id) in [
        ("sup-a", "party-a", "SUP-A", Some("profile-a")),
        ("sup-b", "party-b", "SUP-B", None),
        ("sup-c", "party-c", "SUP-C", None),
        ("sup-orphan", "party-missing", "SUP-ORPHAN", None),
    ] {
        db.supplier_accounts()
            .create(
                &supplier_account(id, party_id, supplier_no, profile_id),
                &mut NoTransaction,
            )
            .await
            .expect("供应商角色写入失败");
    }
    db.supplier_commercial_profile_revisions()
        .create(
            &commercial_profile("profile-a", "sup-a", "party-a"),
            &mut NoTransaction,
        )
        .await
        .expect("商务资料写入失败");
    for (id, supplier_id, code) in [
        ("cap-a1", "sup-a", CapabilityCode::Physical),
        ("cap-b1", "sup-b", CapabilityCode::Physical),
        ("cap-c1", "sup-c", CapabilityCode::Api),
    ] {
        db.supplier_capabilities()
            .create(&capability(id, supplier_id, code), &mut NoTransaction)
            .await
            .expect("供应商能力写入失败");
    }
    let expiring = BusinessDate::from_ymd(2026, 9, 10).unwrap();
    let expired = BusinessDate::from_ymd(2026, 6, 1).unwrap();
    for (id, supplier_id, qualification_type, certificate_no, valid_to) in [
        (
            "qual-a",
            "sup-a",
            QualificationType::FoodLicense,
            "FOOD-A",
            Some(expiring),
        ),
        ("qual-a2", "sup-a", QualificationType::Contract, "HT-A", None),
        (
            "qual-c",
            "sup-c",
            QualificationType::FoodLicense,
            "FOOD-C",
            Some(expired),
        ),
    ] {
        db.supplier_qualifications()
            .create(
                &qualification(id, supplier_id, qualification_type, certificate_no, valid_to),
                &mut NoTransaction,
            )
            .await
            .expect("供应商资质写入失败");
    }
    for (id, qualification_id, capability_id) in
        [("link-a1", "qual-a", "cap-a1"), ("link-a2", "qual-a2", "cap-a1")]
    {
        db.supplier_qualification_capabilities()
            .create(
                &SupplierQualificationCapability::new(
                    SupplierQualificationCapabilityId::new(id),
                    SupplierQualificationCapabilityData {
                        qualification_id: SupplierQualificationId::new(qualification_id),
                        capability_id: SupplierCapabilityId::new(capability_id),
                    },
                )
                .expect("资质关联构造失败"),
                &mut NoTransaction,
            )
            .await
            .expect("资质关联写入失败");
    }
    db.supplier_rating_revisions()
        .create(
            &SupplierRatingRevision::new(
                SupplierRatingRevisionId::new("rating-a"),
                SupplierRatingRevisionData {
                    supplier_id: SupplierAccountId::new("sup-a"),
                    revision_no: 1,
                    initial_score: Some(80),
                    rating: SupplierRating::A,
                    current_score: 85,
                    valid_from: BusinessDate::from_ymd(2026, 8, 1).unwrap(),
                    valid_to: None,
                    change_reason: "初始评级".to_string(),
                },
            )
            .expect("供应商评级构造失败"),
            &mut NoTransaction,
        )
        .await
        .expect("供应商评级写入失败");
}

/// 构造列表搜索输入。
///
/// # 参数
/// * `build` - 输入调整闭包，由调用方设置筛选维度
///
/// # 返回
/// 返回业务日固定为验收日的搜索输入。
fn list_input(build: impl FnOnce(&mut SupplierListSearchInput)) -> SupplierListSearchInput {
    let mut input = SupplierListSearchInput {
        keyword: None,
        party_id: None,
        status: None,
        capability_codes: Vec::new(),
        qualification_types: Vec::new(),
        qualification_health: None,
        as_of: AS_OF.to_string(),
        page: 1,
        page_size: 20,
        sort_by: Some("created_at".to_string()),
        sort_ascending: false,
    };
    build(&mut input);
    input
}

/// 返回事实束页中的供应商 ID 集合（字典序）。
///
/// # 参数
/// * `bundle` - 列表事实束
///
/// # 返回
/// 返回当前页供应商 ID 的稳定排序集合。
fn bundle_ids(bundle: &super::SupplierListBundle) -> Vec<String> {
    let mut ids: Vec<String> = bundle.page.items.iter().map(|row| row.id.clone()).collect();
    ids.sort();
    ids
}

/// 关键词、能力、资质健康状态均在分页计数前生效，且总数不受页大小影响。
///
/// # 参数
/// 无，内部创建隔离库。
///
/// # 返回
/// 组合筛选、`NotRegistered` 排除集、能力与资质交集及分页总数全部符合预期时通过。
///
/// # 错误
/// 任一筛选总数、分页总数或水合事实与预期不一致时测试失败。
///
/// # 约束
/// `#[ignore]` 由 Quality 在隔离副本集执行；总数断言覆盖分页前生效语义。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn supplier_list_bundle_applies_all_prefilters_before_paging() {
    require_mongo!(async {
        let fixture = TestDb::new("proc_supplier_io_list")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_supplier_io_fixture(fixture.db()).await;

        let bundle = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(&list_input(|_| {}), &mut NoTransaction)
            .await
            .expect("列表事实束加载失败");
        assert_eq!(bundle.page.total, 4, "无筛选时总数应覆盖全部供应商");
        assert_eq!(bundle_ids(&bundle), vec!["sup-a", "sup-b", "sup-c", "sup-orphan"]);

        let paged = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.page_size = 1;
                }),
                &mut NoTransaction,
            )
            .await
            .expect("分页事实束加载失败");
        assert_eq!(paged.page.total, 4, "总数不得随页大小变化");
        assert_eq!(paged.page.items.len(), 1);

        let keyword = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.keyword = Some("供应商甲".to_string());
                }),
                &mut NoTransaction,
            )
            .await
            .expect("关键词事实束加载失败");
        assert_eq!(bundle_ids(&keyword), vec!["sup-a"], "主体名称命中应在分页前生效");

        let capability = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.capability_codes = vec![CapabilityCode::Physical];
                }),
                &mut NoTransaction,
            )
            .await
            .expect("能力筛选事实束加载失败");
        assert_eq!(bundle_ids(&capability), vec!["sup-a", "sup-b"]);

        let valid = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.qualification_types = vec![QualificationType::FoodLicense];
                    input.qualification_health = Some(SupplierQualificationHealthFilter::Valid);
                }),
                &mut NoTransaction,
            )
            .await
            .expect("有效资质事实束加载失败");
        assert_eq!(bundle_ids(&valid), vec!["sup-a"]);

        let expiring = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.qualification_types = vec![QualificationType::FoodLicense];
                    input.qualification_health = Some(SupplierQualificationHealthFilter::Expiring30);
                }),
                &mut NoTransaction,
            )
            .await
            .expect("临期资质事实束加载失败");
        assert_eq!(bundle_ids(&expiring), vec!["sup-a"]);

        let expired = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.qualification_types = vec![QualificationType::FoodLicense];
                    input.qualification_health = Some(SupplierQualificationHealthFilter::Expired);
                }),
                &mut NoTransaction,
            )
            .await
            .expect("失效资质事实束加载失败");
        assert_eq!(bundle_ids(&expired), vec!["sup-c"]);

        let not_registered = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.qualification_types = vec![QualificationType::FoodLicense];
                    input.qualification_health = Some(SupplierQualificationHealthFilter::NotRegistered);
                }),
                &mut NoTransaction,
            )
            .await
            .expect("未登记资质事实束加载失败");
        assert_eq!(
            bundle_ids(&not_registered),
            vec!["sup-b", "sup-orphan"],
            "未登记分支应返回排除集，命中集合为空"
        );

        let intersection = fixture
            .db()
            .supplier()
            .load_supplier_list_bundle(
                &list_input(|input| {
                    input.capability_codes = vec![CapabilityCode::Physical];
                    input.qualification_types = vec![QualificationType::FoodLicense];
                    input.qualification_health = Some(SupplierQualificationHealthFilter::Valid);
                }),
                &mut NoTransaction,
            )
            .await
            .expect("交集筛选事实束加载失败");
        assert_eq!(bundle_ids(&intersection), vec!["sup-a"]);

        assert!(
            bundle.parties.iter().any(|party| party.base.id == "party-a"),
            "水合事实应包含命中主体"
        );
        assert!(
            bundle
                .revisions
                .iter()
                .any(|revision| revision.base.id == "partyrev-a"),
            "水合事实应包含主体当前修订"
        );
        assert!(
            bundle
                .profiles
                .iter()
                .any(|profile| profile.base.id == "profile-a"),
            "水合事实应包含当前商务资料"
        );
    });
}

/// 详情事实束一次批量返回全部历史集合，缺失指针有明确语义。
///
/// # 参数
/// 无，内部创建隔离库。
///
/// # 返回
/// 资质与关联批量齐套、商务名称齐套、主体缺失与供应商缺失语义正确时通过。
///
/// # 错误
/// 任一集合缺失、关联不齐或缺失语义不符时测试失败。
///
/// # 约束
/// `#[ignore]` 由 Quality 在隔离副本集执行；关联一次批量读取，不断言查询次数。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn supplier_detail_bundle_returns_batch_facts_and_missing_pointer_semantics() {
    require_mongo!(async {
        let fixture = TestDb::new("proc_supplier_io_detail")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_supplier_io_fixture(fixture.db()).await;

        let bundle = fixture
            .db()
            .supplier()
            .load_supplier_detail_bundle(&SupplierAccountId::new("sup-a"), &mut NoTransaction)
            .await
            .expect("详情事实束加载失败")
            .expect("sup-a 事实束缺失");
        assert_eq!(bundle.supplier.base.id, "sup-a");
        assert_eq!(bundle.party.as_ref().expect("主体缺失").base.id, "party-a");
        assert_eq!(
            bundle.party_revision.as_ref().expect("当前修订缺失").base.id,
            "partyrev-a"
        );
        assert_eq!(bundle.capabilities.len(), 1);
        assert_eq!(bundle.qualifications.len(), 2);
        assert_eq!(
            bundle.qualification_links.len(),
            2,
            "两份资质的适用关联应一次批量读回"
        );
        assert_eq!(bundle.ratings.len(), 1);
        assert_eq!(bundle.commercial_profiles.len(), 1);
        assert_eq!(
            bundle.commercial_party_names.get("party-a").map(String::as_str),
            Some("供应商甲")
        );

        let orphan = fixture
            .db()
            .supplier()
            .load_supplier_detail_bundle(&SupplierAccountId::new("sup-orphan"), &mut NoTransaction)
            .await
            .expect("孤儿事实束加载失败")
            .expect("sup-orphan 事实束缺失");
        assert!(orphan.party.is_none(), "主体缺失时应为 None");
        assert!(orphan.party_revision.is_none(), "主体缺失时修订应为 None");
        assert!(orphan.capabilities.is_empty());
        assert!(orphan.qualifications.is_empty());

        let missing = fixture
            .db()
            .supplier()
            .load_supplier_detail_bundle(&SupplierAccountId::new("sup-missing"), &mut NoTransaction)
            .await
            .expect("缺失供应商查询失败");
        assert!(missing.is_none(), "供应商缺失时应返回 None");
    });
}

/// 列表候选约束查询的执行计划必须命中唯一索引且无集合扫描。
///
/// # 参数
/// 无，内部创建隔离库。
///
/// # 返回
/// `explain` 命中 `uk_supplier_accounts_id` 的 `IXSCAN` 且无 `COLLSCAN` 时通过。
///
/// # 错误
/// 索引未命中或出现集合扫描时测试失败。
///
/// # 约束
/// 不使用 `hint`；`#[ignore]` 由 Quality 在隔离副本集执行。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn supplier_list_candidate_query_uses_index_without_collscan() {
    require_mongo!(async {
        let fixture = TestDb::new("proc_supplier_io_explain")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_supplier_io_fixture(fixture.db()).await;

        let explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": SUPPLIER_ACCOUNTS,
                    "filter": {
                        "id": { "$in": ["sup-a", "sup-b"] },
                        "deleted_at": 0_i64,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("候选约束查询 explain 失败");
        let rendered = format!("{explain:?}");
        assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
        assert!(
            rendered.contains("uk_supplier_accounts_id"),
            "explain 未命中 uk_supplier_accounts_id：{rendered}"
        );
        assert!(
            !rendered.contains("COLLSCAN"),
            "explain 出现 COLLSCAN：{rendered}"
        );

        let capability_explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": SUPPLIER_CAPABILITIES,
                    "filter": {
                        "supplier_id": "sup-a",
                        "deleted_at": 0_i64,
                    },
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("能力查询 explain 失败");
        let capability_rendered = format!("{capability_explain:?}");
        assert!(
            capability_rendered.contains("IXSCAN"),
            "能力查询 explain 未使用 IXSCAN：{capability_rendered}"
        );
        assert!(
            !capability_rendered.contains("COLLSCAN"),
            "能力查询 explain 出现 COLLSCAN：{capability_rendered}"
        );
    });
}

/// 事实束查询复用调用方执行器，事务内可见同一会话写入。
///
/// # 参数
/// 无，内部创建隔离库。
///
/// # 返回
/// 事务内写入的供应商在同一会话的列表与详情事实束中均可见时通过。
///
/// # 错误
/// 事务内重验不可见或提交失败时测试失败。
///
/// # 约束
/// 事务内重验必须复用调用方 executor，不得另开连接或独立事务；
/// `#[ignore]` 由 Quality 在隔离副本集执行。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn supplier_bundles_see_same_session_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("proc_supplier_io_txn")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_supplier_io_fixture(fixture.db()).await;

        let (party, revision) = party_with_revision("party-txn", "partyrev-txn", "供应商丁");
        let supplier = supplier_account("sup-txn", "party-txn", "SUP-TXN-1", None);
        let trans_capability = capability("cap-txn", "sup-txn", CapabilityCode::Physical);

        let db = fixture.db().clone();
        let client = db.client().clone();
        client
            .with_transaction::<_, (), crate::errors::Error>(move |session| {
                let db = db.clone();
                let party = party.clone();
                let revision = revision.clone();
                let supplier = supplier.clone();
                let trans_capability = trans_capability.clone();
                Box::pin(async move {
                    db.parties().create(&party, session).await?;
                    db.party_revisions().create(&revision, session).await?;
                    db.supplier_accounts().create(&supplier, session).await?;
                    db.supplier_capabilities()
                        .create(&trans_capability, session)
                        .await?;
                    let bundle = db
                        .supplier()
                        .load_supplier_list_bundle(
                            &SupplierListSearchInput {
                                keyword: Some("SUP-TXN-1".to_string()),
                                party_id: None,
                                status: None,
                                capability_codes: Vec::new(),
                                qualification_types: Vec::new(),
                                qualification_health: None,
                                as_of: AS_OF.to_string(),
                                page: 1,
                                page_size: 20,
                                sort_by: Some("created_at".to_string()),
                                sort_ascending: false,
                            },
                            session,
                        )
                        .await?;
                    assert_eq!(bundle.page.total, 1, "事务内应能 read-your-writes");
                    let detail = db
                        .supplier()
                        .load_supplier_detail_bundle(&SupplierAccountId::new("sup-txn"), session)
                        .await?;
                    let detail = detail.expect("事务内详情事实束缺失");
                    assert_eq!(detail.capabilities.len(), 1);
                    Ok(())
                })
            })
            .await
            .expect("同一会话事务读写失败");
    });
}
