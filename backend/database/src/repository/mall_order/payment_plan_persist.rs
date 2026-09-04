//! 消费入账支付图批量持久化（INT-R07 仓储所有权）。
//!
//! 逐条多集合写入只归属本模块的批量 primitive；事务仍由 Service 开启，
//! 本模块只使用调用方传入的执行器，不管理事务生命周期。

use entities::cost::{CostAllocation, CostEntry};
use entities::mall_order::{
    MallConsumptionCostAssessment, MallConsumptionEntry, MallItemFundingAllocation, MallOrderItem,
    MallPaymentSource,
};

use super::{MallConsumptionCostAssessmentRepository, MallConsumptionEntryRepository};
use crate::executor::Executor;
use crate::repository::Repository;
use crate::Result;
use serde::Serialize;

/// 会话感知的有序批量插入（INT-R07 确定性写入）。
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

impl<'a> Repository<'a, MallOrderItem> {
    /// 批量创建商品明细（INT-R07）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建明细。
    ///
    /// # 参数
    /// * `self` - 商品明细仓储
    /// * `items` - 待写入明细；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突（透出 `DuplicateKey`）或 MongoDB 写入失败时返回错误；
    /// 任一失败由调用方事务整体回滚，本方法不做部分提交。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        items: &[MallOrderItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), items.to_vec(), executor).await?;
        Ok(())
    }
}

impl<'a> Repository<'a, MallPaymentSource> {
    /// 批量创建支付来源（INT-R07）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建来源。
    ///
    /// # 参数
    /// * `self` - 支付来源仓储
    /// * `sources` - 待写入来源；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误，由调用方事务整体回滚。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        sources: &[MallPaymentSource],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if sources.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), sources.to_vec(), executor).await?;
        Ok(())
    }
}

impl<'a> Repository<'a, MallItemFundingAllocation> {
    /// 批量创建分摊记录（INT-R07）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建分摊。
    ///
    /// # 参数
    /// * `self` - 分摊仓储
    /// * `allocations` - 待写入分摊；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误，由调用方事务整体回滚。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        allocations: &[MallItemFundingAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if allocations.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), allocations.to_vec(), executor).await?;
        Ok(())
    }
}

impl MallConsumptionEntryRepository<'_> {
    /// 批量创建消费事实（INT-R07）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建消费。
    ///
    /// # 参数
    /// * `self` - 消费事实只读追加仓储
    /// * `entries` - 待写入消费；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误，由调用方事务整体回滚。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        entries: &[MallConsumptionEntry],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), entries.to_vec(), executor).await?;
        Ok(())
    }
}

impl MallConsumptionCostAssessmentRepository<'_> {
    /// 批量创建成本评估（INT-R07）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建评估。
    ///
    /// # 参数
    /// * `self` - 成本评估只读追加仓储
    /// * `assessments` - 待写入评估；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误，由调用方事务整体回滚。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        assessments: &[MallConsumptionCostAssessment],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if assessments.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), assessments.to_vec(), executor).await?;
        Ok(())
    }
}

impl<'a> Repository<'a, CostEntry> {
    /// 批量创建成本事实（INT-R07 支付图成本段）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建成本事实。
    ///
    /// # 参数
    /// * `self` - 成本事实仓储
    /// * `entries` - 待写入成本事实；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误，由调用方事务整体回滚。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        entries: &[CostEntry],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), entries.to_vec(), executor).await?;
        Ok(())
    }
}

impl<'a> Repository<'a, CostAllocation> {
    /// 批量创建成本分配（INT-R07 支付图成本段）。
    ///
    /// # 用途
    /// 以一次有序批量写入替代支付计划逐条创建成本分配。
    ///
    /// # 参数
    /// * `self` - 成本分配仓储
    /// * `allocations` - 待写入成本分配；为空时直接返回，不访问数据库
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误，由调用方事务整体回滚。
    ///
    /// # 关键约束
    /// 不开事务、不提交事务；保持调用方传入顺序的确定性写入。
    pub async fn create_many_ordered(
        &self,
        allocations: &[CostAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if allocations.is_empty() {
            return Ok(());
        }
        insert_many_ordered(&self.collection(), allocations.to_vec(), executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::executor::Executor;
    use crate::repository::extensions::{CostExt, MallOrderExt};

    /// 空批量不得触碰执行器的断言执行器。
    ///
    /// 任何会话获取尝试直接 panic；空集合用例以此证明零数据库访问。
    struct NeverTouchExecutor;

    impl Executor for NeverTouchExecutor {
        /// 获取会话（空批量路径不可达）。
        ///
        /// # 返回
        /// 永不返回，直接 panic。
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            panic!("空批量不得访问执行器或数据库");
        }
    }

    /// 构造未连接的测试数据库句柄（懒客户端，不建立连接）。
    ///
    /// # 返回
    /// 返回指向隔离库名的数据库句柄。
    async fn unit_db() -> mongodb::Database {
        let options = mongodb::options::ClientOptions::parse("mongodb://127.0.0.1:27017")
            .await
            .expect("测试客户端选项解析失败");
        let client = mongodb::Client::with_options(options).expect("测试客户端构造失败");
        client.database("int_r07_unit")
    }

    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("payment_plan_persist.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 显式有序写入守卫：7 个批量入口全部经显式 `ordered(true)` 写入。
    ///
    /// 锁定有序 helper 为唯一写入路径；依赖驱动默认的通用批量写入不得回潮。
    #[test]
    fn batch_writes_declare_ordered_inserts_explicitly() {
        let source = production_source();
        assert!(
            source.contains(".ordered(true)"),
            "批量写入必须显式声明 ordered(true)，不得依赖驱动默认"
        );
        assert_eq!(
            source.matches("insert_many_ordered(&self.collection()").count(),
            7,
            "7 个集合批量入口必须全部经有序 helper 写入"
        );
        assert!(!source.contains("mongo_ops::insert_many"), "默认顺序写入不得回潮");
    }

    /// 空集合批量写入直接返回，不访问数据库（0 条写入验收维度，无 I/O 单测）。
    ///
    /// 断言执行器证明空路径零数据库访问；1/多条有序写入与重复键整体回滚需
    /// 真实副本集，由 Quality Mongo 门禁覆盖，不在本 `--lib` 门禁内断言。
    #[tokio::test]
    async fn empty_batches_succeed_without_database_access() {
        let db = unit_db().await;
        db.mall_order_items()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空明细批量必须成功");
        db.mall_payment_sources()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空来源批量必须成功");
        db.mall_item_funding_allocations()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空分摊批量必须成功");
        db.mall_consumption_entries()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空消费批量必须成功");
        db.mall_consumption_cost_assessments()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空评估批量必须成功");
        db.cost_entries()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空成本事实批量必须成功");
        db.cost_allocations()
            .create_many_ordered(&[], &mut NeverTouchExecutor)
            .await
            .expect("空成本分配批量必须成功");
    }
}
