//! 并发取号集成测试。
//!
//! 需要真实 MongoDB，连接串来自环境变量 `ERP_TEST_MONGO_URI`；未设置时跳过
//! 并打印原因。运行方式：
//! `ERP_TEST_MONGO_URI=mongodb://127.0.0.1:27017 cargo test -p id-generator -- --include-ignored`

mod common;

use std::collections::HashSet;

use chrono::NaiveDate;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, NoTransaction};
use mongodb::Client;

/// 并发任务数。
const TASKS: usize = 50;

/// 每任务取号次数。
const TAKES_PER_TASK: usize = 20;

/// 期望取号总数（50 × 20 = 1000）。
const TOTAL_TAKES: i64 = 1000;

/// 并发 1000 次取号必须全部唯一，且序号段为 1..=1000 的连续排列（无跳号）。
///
/// # 说明
/// 50 个任务各取 20 个号；所有任务共享同一计数器文档，原子 `$inc` 保证
/// MongoDB 侧串行化，任何重复或跳号都说明取号实现存在并发缺陷。
#[tokio::test]
#[ignore]
async fn concurrent_takes_are_unique_and_contiguous() {
    let Some(uri) = common::mongo_uri() else {
        return;
    };
    let client = Client::with_uri_str(&uri)
        .await
        .expect("should connect to MongoDB");
    let db = client.database(&common::unique_database_name("id_generator_concurrency"));
    let generator = DocumentNumberGenerator::new(db.clone());
    let kind = DocumentNumberKind::SalesOrder;
    let date = NaiveDate::from_ymd_opt(2026, 7, 1).expect("valid date");

    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..TASKS {
        let generator = generator.clone();
        tasks.spawn(async move {
            let mut numbers = Vec::with_capacity(TAKES_PER_TASK);
            for _ in 0..TAKES_PER_TASK {
                let mut executor = NoTransaction;
                numbers.push(
                    generator
                        .next_number(kind, date, &mut executor)
                        .await
                        .expect("take should succeed"),
                );
            }
            numbers
        });
    }

    let mut numbers = Vec::with_capacity(TASKS * TAKES_PER_TASK);
    while let Some(batch) = tasks.join_next().await {
        numbers.extend(batch.expect("task should not panic"));
    }

    let unique: HashSet<&str> = numbers.iter().map(String::as_str).collect();
    assert_eq!(unique.len(), numbers.len(), "并发取号必须全部唯一");

    let mut seqs: Vec<i64> = numbers.iter().map(|number| common::seq_of(number)).collect();
    seqs.sort_unstable();
    let expected: Vec<i64> = (1..=TOTAL_TAKES).collect();
    assert_eq!(seqs, expected, "序号段必须是 1..=1000 的连续排列（无跳号）");

    assert!(
        numbers.iter().all(|number| number.starts_with("SO20260701-")),
        "编号必须使用预期前缀与日期段"
    );

    db.drop().await.expect("test database should be dropped");
}
