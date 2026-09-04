//! 消费反向事实批量追加（INT-R13）。
//!
//! `receive_refund` 事务内按分配逐条插入消费冲减事实，集合写入细节泄漏到
//! Service。本文件提供调用方 executor 下的批量追加 primitive：空集合不访问
//! 数据库；非空按输入顺序一次 `insert_many`，任一重复键整体失败由调用方事务
//! 回滚。追加顺序确定，与旧逐条循环一致。

use entities::mall_order::MallConsumptionEntry;
use serde::Serialize;

use super::super::extensions::MallOrderExt;
use super::MallAfterSalesRepository;
use crate::executor::Executor;
use crate::Result;

/// 会话感知的有序批量插入（INT-R13 确定性写入）。
///
/// 与通用 `insert_many` 的区别是显式声明 `ordered(true)`：
/// 首个失败后停止剩余写入，写入顺序与调用方传入顺序一致，不依赖驱动默认值。
/// 唯一冲突透出后由调用方事务整体回滚，本函数不做部分提交.
///
/// # 参数
/// * `collection` - 目标集合
/// * `documents` - 待插入文档；为空时直接返回，不访问数据库
/// * `executor` - 数据访问执行器，必须位于调用方事务中
///
/// # 返回
/// 写入成功返回 `Ok(())`。
///
/// # 错误
/// 唯一索引冲突（透出 `DuplicateKey`）或 MongoDB 写入失败时返回错误。
///
/// # 关键约束
/// 不开事务、不提交事务；有序写入显式声明，不依赖驱动默认。
async fn insert_many_ordered<T>(
    collection: &mongodb::Collection<T>,
    documents: Vec<T>,
    executor: &mut dyn Executor,
) -> Result<()>
where
    T: Serialize + Send + Sync,
{
    if documents.is_empty() {
        return Ok(());
    }
    match executor.session() {
        Some(session) => {
            collection
                .insert_many(documents)
                .ordered(true)
                .session(session)
                .await?;
        }
        None => {
            collection.insert_many(documents).ordered(true).await?;
        }
    }
    Ok(())
}

/// `mall_consumption_entries` 集合名（单一来源：`MallOrderExt`）。
const MALL_CONSUMPTION_ENTRIES: &str = <mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES;

impl<'a> MallAfterSalesRepository<'a> {
    /// 批量追加消费反向事实（INT-R13）。
    ///
    /// 空输入直接返回成功且不访问数据库；非空按调用方给定顺序一次写入，
    /// 保持确定性写入顺序与 `append-only` 错误透出（重复键透出
    /// [`crate::Error::DuplicateKey`]）。必须收到事务执行器；本方法不构成
    /// 原子边界，`NoTransaction` 下各笔各自提交，Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `entries` - 待追加的消费反向事实（顺序即写入顺序）
    /// * `executor` - 数据访问执行器，必须位于写入事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`；空输入亦返回 `Ok(())`。
    ///
    /// # 错误
    /// 当唯一索引冲突或 MongoDB 写入失败时返回错误；失败时是否部分可见由
    /// 调用方事务决定。
    ///
    /// # 约束
    /// 不开启或提交事务；不返回 services DTO、HTTP View 或授权结论。
    pub async fn create_consumption_reversals(
        &self,
        entries: &[MallConsumptionEntry],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        insert_many_ordered(
            &self
                .db
                .collection::<MallConsumptionEntry>(MALL_CONSUMPTION_ENTRIES),
            entries.to_vec(),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::MallAfterSalesRepository;
    use crate::NoTransaction;

    /// 空输入直接成功且不访问数据库（INT-R13）。
    ///
    /// 测试使用不可达 Mongo 地址构造仓储句柄：空集合在任何 I/O 前短路，
    /// 因此不断开连接；非空路径单批次 `insert_many` 保持输入顺序，
    /// 重复键整体失败由调用方事务回滚（见方法约束）。
    #[tokio::test]
    async fn empty_reversals_succeed_without_touching_database() {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap();
        let database = client.database("repository_after_sales_empty_reversals");
        let repository = MallAfterSalesRepository::new(&database);

        repository
            .create_consumption_reversals(&[], &mut NoTransaction)
            .await
            .unwrap();
    }

    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("reversal_entry_batch.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 显式有序写入守卫：批量追加必须经显式 `ordered(true)` 写入。
    ///
    /// 锁定有序 helper 为唯一写入路径；依赖驱动默认的通用批量写入不得回潮。
    #[test]
    fn reversal_batch_declares_ordered_insert_explicitly() {
        let source = production_source();
        assert!(
            source.contains(".ordered(true)"),
            "批量追加必须显式声明 ordered(true)，不得依赖驱动默认"
        );
        assert!(!source.contains("mongo_ops::insert_many"), "默认顺序写入不得回潮");
    }
}
