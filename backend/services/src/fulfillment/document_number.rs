//! 履约草稿的可展示业务编号。
//!
//! 自动生成的入库、发货草稿不得把内部主键拼进 `*_no`。编号走
//! [`DocumentNumberGenerator`]，格式为 `DN20260826-000001` 这类日期序号。

use chrono::{FixedOffset, TimeZone, Utc};
use database::NoTransaction;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind};
use mongodb::Database;

use crate::errors::{Error, Result};

/// 为履约发货单取下一个可展示单号。
///
/// # 参数
/// * `db` - 业务库；计数器集合与业务数据同库
///
/// # 返回
/// 返回 `DNYYYYMMDD-000001` 形态的发货单号。
///
/// # 错误
/// 时区无法形成或计数器写入失败时返回内部错误。
///
/// # 关键业务约束
/// 计数器自增不加入调用方事务；序号一经消费不因履约草稿回滚而回收。
pub(crate) async fn next_delivery_no(db: &Database) -> Result<String> {
    next_kind_no(db, DocumentNumberKind::Delivery).await
}

/// 为采购入库单取下一个可展示单号。
///
/// # 参数
/// * `db` - 业务库；计数器集合与业务数据同库
///
/// # 返回
/// 返回 `GRNYYYYMMDD-000001` 形态的入库单号。
///
/// # 错误
/// 时区无法形成或计数器写入失败时返回内部错误。
///
/// # 关键业务约束
/// 计数器自增不加入调用方事务；序号一经消费不因入库草稿回滚而回收。
pub(crate) async fn next_purchase_receipt_no(db: &Database) -> Result<String> {
    next_kind_no(db, DocumentNumberKind::PurchaseReceipt).await
}

/// 为客户验收单取下一个可展示单号。
///
/// # 参数
/// * `db` - 业务库；计数器集合与业务数据同库
///
/// # 返回
/// 返回 `CAYYYYMMDD-000001` 形态的客户验收单号。
///
/// # 错误
/// 时区无法形成或计数器写入失败时返回内部错误。
///
/// # 关键业务约束
/// 单号由服务端在登记时取得；浏览器提交的操作号不得充当业务单号。
pub(crate) async fn next_customer_acceptance_no(db: &Database) -> Result<String> {
    next_kind_no(db, DocumentNumberKind::CustomerAcceptance).await
}

/// 按单据种类取当天下一个可展示编号。
///
/// # 参数
/// * `db` - 业务库
/// * `kind` - 履约单据种类
///
/// # 返回
/// 返回该种类的业务编号。
///
/// # 错误
/// 时区无法形成或计数器写入失败时返回内部错误。
///
/// # 关键业务约束
/// 业务日按 Asia/Shanghai 切日，与工作台统计日一致。
async fn next_kind_no(db: &Database, kind: DocumentNumberKind) -> Result<String> {
    let generator = DocumentNumberGenerator::new(db.clone());
    generator
        .next_number(kind, shanghai_today()?, &mut NoTransaction)
        .await
        .map_err(|error| Error::Internal(error.to_string()))
}

/// 返回当前 Asia/Shanghai 业务日。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回上海时区的当天日期。
///
/// # 错误
/// 固定东八区偏移无法构造时返回内部错误。
///
/// # 关键业务约束
/// 编号日期段必须与工作台业务日一致，不得使用 UTC 日期造成跨日跳号。
fn shanghai_today() -> Result<chrono::NaiveDate> {
    let timezone = FixedOffset::east_opt(8 * 60 * 60)
        .ok_or_else(|| Error::Internal("无法形成 Asia/Shanghai 时区".to_string()))?;
    Ok(timezone.from_utc_datetime(&Utc::now().naive_utc()).date_naive())
}
