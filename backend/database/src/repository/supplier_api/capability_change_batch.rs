//! 已验证能力变更批量持久化（INT-R32）。
//!
//! `apply_capability_changes` 在 Service 内逐项 `update`/`create`，集合写入细节
//! 与逐条访问次数泄漏到编排层。本文件提供调用方 executor 下的批量持久化
//! primitive：已更新实体逐个 CAS 写回（保持乐观锁），新增实体一次有序批量插入；
//! 空输入不访问数据库。版本决策、采购确认覆盖与事务仍归 Service。

use entities::supplier_api::SupplierApiCapability;
use serde::Serialize;

use super::{SupplierApiRepository, SUPPLIER_API_CAPABILITIES};
use crate::executor::Executor;
use crate::Result;

/// 会话感知的有序批量插入（确定性写入）。
///
/// 与通用 `insert_many` 的区别是显式声明 `ordered(true)`：
/// 首个失败后停止剩余写入，写入顺序与调用方传入顺序一致，不依赖驱动默认值。
/// 唯一冲突透出后由调用方事务整体回滚，本函数不做部分提交。
///
/// # 参数
/// * `collection` - 目标集合
/// * `documents` - 待插入文档；为空时直接返回，不访问数据库
/// * `executor` - 数据访问执行器，必须位于调用方事务中
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入失败时返回错误。
///
/// # 约束
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

impl<'a> SupplierApiRepository<'a> {
    /// 批量持久化已验证的能力变更（INT-R32）。
    ///
    /// 已更新实体按调用方给定顺序逐个 CAS 写回（`id + version` 命中，未命中报
    /// 乐观锁冲突并推进内存版本）；新增实体按给定顺序一次有序批量插入。
    /// 两组输入同时为空时直接返回成功且不访问数据库。必须收到事务执行器；
    /// 本方法不构成原子边界，`NoTransaction` 下各笔各自提交，Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话，任一失败整体回滚。
    ///
    /// # 参数
    /// * `updates` - 待写回的已更新能力实体（顺序即写入顺序；内存版本随写回推进）
    /// * `creates` - 待插入的新增能力实体（顺序即写入顺序）
    /// * `executor` - 数据访问执行器，必须位于写入事务中
    ///
    /// # 返回
    /// 持久化成功返回 `Ok(())`；两组输入同时为空亦返回 `Ok(())`。
    ///
    /// # 错误
    /// 当乐观锁冲突、唯一索引冲突或 MongoDB 读写失败时返回错误；失败时是否部分
    /// 可见由调用方事务决定。
    ///
    /// # 约束
    /// 不开启或提交事务；更新保持逐文档 CAS（批量语义不得替代乐观锁）；
    /// 不返回 services DTO、HTTP View 或授权结论。
    pub async fn persist_capability_changes(
        &self,
        updates: &mut [SupplierApiCapability],
        creates: &[SupplierApiCapability],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if updates.is_empty() && creates.is_empty() {
            return Ok(());
        }
        let capabilities = crate::Repository::new(self.db, SUPPLIER_API_CAPABILITIES);
        for capability in updates.iter_mut() {
            capabilities.update(capability, executor).await?;
        }
        insert_many_ordered(
            &self
                .db
                .collection::<SupplierApiCapability>(SUPPLIER_API_CAPABILITIES),
            creates.to_vec(),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::SupplierApiRepository;
    use crate::NoTransaction;

    /// 空输入直接成功且不访问数据库（INT-R32）。
    ///
    /// 测试使用不可达 Mongo 地址构造仓储句柄：空集合在任何 I/O 前短路，
    /// 因此不断开连接；非空路径更新保持逐文档 CAS、新增保持单次有序批量，
    /// 任一失败整体回滚由调用方事务保证（见方法约束）。
    #[tokio::test]
    async fn empty_capability_changes_succeed_without_touching_database() {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap();
        let database = client.database("repository_supplier_api_empty_capability_changes");
        let repository = SupplierApiRepository::new(&database);

        repository
            .persist_capability_changes(&mut [], &[], &mut NoTransaction)
            .await
            .unwrap();
    }

    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("capability_change_batch.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 显式有序写入守卫：新增批量必须经显式 `ordered(true)` 写入。
    ///
    /// 锁定有序 helper 为唯一新增写入路径；更新保持逐文档 CAS，不得为批量
    /// 而放宽乐观锁。
    #[test]
    fn capability_create_batch_declares_ordered_insert_explicitly() {
        let source = production_source();
        assert!(
            source.contains(".ordered(true)"),
            "新增批量必须显式声明 ordered(true)，不得依赖驱动默认"
        );
        assert!(
            source.contains("capabilities.update(capability, executor)"),
            "已更新实体必须保持逐文档 CAS 写回"
        );
    }
}
