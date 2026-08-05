//! 事务回滚与编号连续性集成测试。
//!
//! 验证：取号成功即消费序号；事务回滚后序号不回收（正式序列产生跳号是
//! 预期行为），计数器自增不随调用方事务回滚。
//!
//! 需要真实 MongoDB 副本集（多文档事务依赖副本集），连接串来自环境变量
//! `ERP_TEST_MONGO_URI`；未设置时跳过并打印原因。

mod common;

use chrono::NaiveDate;
use database::{Error as DatabaseError, Transactional};
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, NoTransaction};
use mongodb::Client;

/// 测试事务错误类型：携带被回滚事务取到的编号。
#[derive(Debug)]
enum TestTxnError {
    /// 主动触发回滚，并携带事务内取到的编号。
    Abort(String),

    /// 事务内真实错误（测试只关心回滚路径，不读取具体错误）。
    Inner,
}

impl From<id_generator::Error> for TestTxnError {
    fn from(_error: id_generator::Error) -> Self {
        Self::Inner
    }
}

impl From<DatabaseError> for TestTxnError {
    fn from(_error: DatabaseError) -> Self {
        Self::Inner
    }
}

/// 事务内取号后回滚，再取号：序号不回收、不回收到同一号，正式序列跳号。
///
/// # 说明
/// 取号成功即消费序号；计数器自增按自动提交执行，不随调用方事务回滚。
/// 回滚后再次取号必须得到 2 号而非重新取到 1 号，即 1 号已被烧掉——
/// 这是"防重复优先于防跳号"的预期行为（业务单号不可复用）。
#[tokio::test]
#[ignore]
async fn number_taken_in_aborted_transaction_is_not_recycled() {
    let Some(uri) = common::mongo_uri() else {
        return;
    };
    let client = Client::with_uri_str(&uri)
        .await
        .expect("should connect to MongoDB");
    let db = client.database(&common::unique_database_name("id_generator_rollback"));
    let generator = DocumentNumberGenerator::new(db.clone());
    let kind = DocumentNumberKind::SalesOrder;
    let date = NaiveDate::from_ymd_opt(2026, 7, 1).expect("valid date");

    let outcome = client
        .with_transaction::<_, String, TestTxnError>({
            let generator = generator.clone();
            move |session| {
                Box::pin(async move {
                    let number = generator.next_number(kind, date, session).await?;
                    Err(TestTxnError::Abort(number))
                })
            }
        })
        .await;
    let first = match outcome {
        Err(TestTxnError::Abort(first)) => first,
        other => panic!("事务应因业务失败而回滚，实际返回: {other:?}"),
    };

    let mut executor = NoTransaction;
    let second = generator
        .next_number(kind, date, &mut executor)
        .await
        .expect("take should succeed after rollback");

    assert_ne!(first, second, "回滚事务已消费的序号不得回收复用");
    assert_eq!(common::seq_of(&first), 1, "事务内取号应消费 1 号");
    assert_eq!(
        common::seq_of(&second),
        2,
        "回滚后序号不回收，下一次取号应为 2 号（正式序列出现 1 号空缺）"
    );
    assert!(
        first.starts_with("SO20260701-") && second.starts_with("SO20260701-"),
        "编号必须使用预期前缀与日期段"
    );

    let third = generator
        .next_number(kind, date, &mut executor)
        .await
        .expect("take should succeed");
    assert_eq!(common::seq_of(&third), 3, "计数器必须继续前进，不因回滚回退");

    db.drop().await.expect("test database should be dropped");
}
