//! 公开限流窗口仓储。

use crate::repository::SalesSelectionExt;

/// 公开限流窗口仓储。
pub struct SalesSelectionRateRepository<'a> {
    db: &'a mongodb::Database,
}

impl<'a> SalesSelectionRateRepository<'a> {
    /// 将公开选品限流窗口仓储绑定到当前数据库，以便在验证访问前消费其配额。
    /// 返回仓储。
    pub fn new(db: &'a mongodb::Database) -> Self {
        Self { db }
    }

    /// 原子占用一分钟窗口配额。
    /// 结果：未超限返回 `Ok(())`。
    /// 失败条件：超限返回业务冲突由调用方映射；仓储失败返回存储错误。
    ///
    /// 输入约定：
    /// - `key`：限流键
    /// - `limit`：每分钟上限
    /// - `window_id`：分钟窗口
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
