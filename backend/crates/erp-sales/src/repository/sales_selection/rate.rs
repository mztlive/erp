//! 公开限流窗口仓储。

use crate::repository::SalesSelectionExt;

/// 公开限流窗口仓储。
pub struct SalesSelectionRateRepository<'a> {
    db: &'a mongodb::Database,
}

impl<'a> SalesSelectionRateRepository<'a> {
    /// 创建限流仓储。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回仓储。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a mongodb::Database) -> Self {
        Self { db }
    }

    /// 原子占用一分钟窗口配额。
    ///
    /// # 参数
    /// * `key` - 限流键
    /// * `limit` - 每分钟上限
    /// * `window_id` - 分钟窗口
    ///
    /// # 返回
    /// 未超限返回 `Ok(())`。
    ///
    /// # 错误
    /// 超限返回业务冲突由调用方映射；仓储失败返回存储错误。
    pub async fn admit(&self, key: &str, limit: i64, window_id: i64) -> persistence_core::Result<bool> {
        let id = format!("{key}:{window_id}");
        let collection = self.db.collection::<mongodb::bson::Document>(
            <mongodb::Database as SalesSelectionExt>::SALES_SELECTION_RATE_WINDOWS,
        );
        let result = collection
            .find_one_and_update(
                mongodb::bson::doc! { "_id": &id },
                mongodb::bson::doc! {
                    "$inc": { "count": 1_i64 },
                    "$setOnInsert": { "expires_at": mongodb::bson::DateTime::from_millis(
                        window_id.saturating_add(2).saturating_mul(60_000)) }
                },
            )
            .upsert(true)
            .return_document(mongodb::options::ReturnDocument::After)
            .await?;
        let count = result.as_ref().and_then(|doc| doc.get_i64("count").ok()).unwrap_or(1);
        Ok(count <= limit)
    }
}
