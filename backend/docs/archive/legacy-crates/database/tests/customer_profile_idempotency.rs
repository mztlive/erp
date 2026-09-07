//! 客户资料根命令幂等身份与唯一查询的真实 MongoDB 验收。

use database::{ensure_indexes, CustomerExt, NoTransaction};
use entities::{
    common::time::BusinessDate,
    customer::{
        CustomerProfileCommand, CustomerProfileCommandResultData, CustomerProfileOperation,
        CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
    },
};
use mongodb::bson::{doc, Bson, Document};
use test_support::{require_mongo, TestDb};

const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ONE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const COMMAND_COLLECTION: &str = <mongodb::Database as CustomerExt>::CUSTOMER_PROFILE_COMMANDS;
const COMMAND_INDEX: &str = "uk_customer_profile_commands_idempotency_key";

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_profile_commands_replay_exactly_and_use_unique_key_index() {
    require_mongo!(async {
        let fixture = TestDb::new("customer_profile_idempotency")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let context = replay_context("profile-key-1", ZERO_DIGEST);
        let first = command("command-1", &context);
        let second = command("command-2", &context);
        let first_db = fixture.db().clone();
        let second_db = fixture.db().clone();
        let create_first = async move {
            first_db
                .customer_profile_commands()
                .create(&first, &mut NoTransaction)
                .await
        };
        let create_second = async move {
            second_db
                .customer_profile_commands()
                .create(&second, &mut NoTransaction)
                .await
        };

        let (first_result, second_result) = tokio::join!(create_first, create_second);
        assert_eq!(
            usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
            1,
            "同一幂等键并发写入必须恰有一个成功：first={first_result:?}, second={second_result:?}"
        );
        assert_eq!(count(fixture.db()).await, 1);

        let stored = fixture
            .db()
            .customer_profile_commands()
            .find_by_idempotency_key(context.idempotency_key(), &mut NoTransaction)
            .await
            .expect("按幂等键重查失败")
            .expect("并发唯一竞争后必须保留一条命令");
        stored
            .ensure_replay_matches(&context)
            .expect("同载荷重放必须返回已提交结果");
        assert_eq!(stored.customer_id, "customer-1");
        assert_eq!(stored.customer_no, "KH-1");
        assert_eq!(stored.revision_id, "revision-2");

        let different_payload = replay_context("profile-key-1", ONE_DIGEST);
        assert!(
            stored.ensure_replay_matches(&different_payload).is_err(),
            "同一幂等键的异载荷必须稳定拒绝"
        );

        let explain = fixture
            .db()
            .run_command(doc! {
                "explain": {
                    "find": COMMAND_COLLECTION,
                    "filter": { "idempotency_key": context.idempotency_key() },
                    "limit": 1_i64,
                },
                "verbosity": "executionStats",
            })
            .await
            .expect("客户资料幂等键查询 explain 失败");
        assert_explain_uses_index(&explain, COMMAND_INDEX);
        let examined = numeric_field(
            explain
                .get_document("executionStats")
                .expect("explain 缺少 executionStats"),
            "totalDocsExamined",
        );
        assert!(examined <= 1, "精确幂等查询最多读取一行，实际 {examined}");
    });
}

fn replay_context(idempotency_key: &str, digest: &str) -> CustomerProfileReplayContext {
    CustomerProfileReplayContext::new(
        idempotency_key,
        CustomerProfileOperation::Update,
        Some("customer-1".to_string()),
        "admin-1",
        CustomerProfileRequestFingerprint::parse_compatible(digest).expect("测试指纹必须合法"),
    )
    .expect("测试重放上下文必须合法")
}

fn command(id: &str, context: &CustomerProfileReplayContext) -> CustomerProfileCommand {
    CustomerProfileCommand::record_success(
        id,
        context,
        CustomerProfileCommandResultData {
            customer_id: "customer-1".to_string(),
            customer_no: "KH-1".to_string(),
            party_id: "party-1".to_string(),
            revision_id: "revision-2".to_string(),
            revision_no: 2,
            customer_version: 2,
            party_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 8, 31).expect("测试日期必须合法"),
            change_reason: "资料修订".to_string(),
        },
    )
    .expect("测试命令必须合法")
}

async fn count(db: &mongodb::Database) -> u64 {
    db.collection::<Document>(COMMAND_COLLECTION)
        .count_documents(doc! {})
        .await
        .expect("命令集合计数失败")
}

fn assert_explain_uses_index(explain: &Document, index_name: &str) {
    let rendered = format!("{explain:?}");
    assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
    assert!(
        !rendered.contains("COLLSCAN"),
        "explain 出现 COLLSCAN：{rendered}"
    );
    assert!(
        rendered.contains(index_name),
        "explain 未命中索引 {index_name}：{rendered}"
    );
}

fn numeric_field(document: &Document, field: &str) -> i64 {
    match document
        .get(field)
        .unwrap_or_else(|| panic!("缺少数值字段 {field}"))
    {
        Bson::Int32(value) => i64::from(*value),
        Bson::Int64(value) => *value,
        value => panic!("字段 {field} 不是整数：{value:?}"),
    }
}
