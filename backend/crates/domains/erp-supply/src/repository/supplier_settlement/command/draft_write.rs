//! 草稿物理替换的真实写入端口；沿调用方执行器逐步完成，首错停止。

use async_trait::async_trait;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use super::super::{
    SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE, SUPPLIER_SETTLEMENT_DIFFERENCES, SUPPLIER_SETTLEMENT_ITEMS,
    SUPPLIER_SETTLEMENT_STATEMENTS,
};
use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementDifferenceEvidence, SupplierSettlementItem,
    SupplierSettlementStatement,
};
use crate::repository::owned::SupplierSettlementStatementRepository;

/// 本域草稿替换的六个数据库步骤，不拥有事务或业务状态校验。
#[async_trait]
#[allow(async_fn_in_trait)]
pub(super) trait DraftSnapshotStore: Send {
    /// 删除给定差异主键下的补证。
    ///
    /// # 参数
    /// * `difference_ids` - 差异主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 删除完成时无返回值。
    ///
    /// # 错误
    /// 删除失败时返回对应错误。
    async fn delete_evidence(&mut self, difference_ids: &[String], executor: &mut dyn Executor)
    -> Result<()>;
    /// 删除给定明细主键下的差异。
    ///
    /// # 参数
    /// * `item_ids` - 结算明细主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 删除完成时无返回值。
    ///
    /// # 错误
    /// 删除失败时返回对应错误。
    async fn delete_differences(&mut self, item_ids: &[String], executor: &mut dyn Executor) -> Result<()>;
    /// 删除给定结算单下的明细。
    ///
    /// # 参数
    /// * `statement_id` - 结算单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 删除完成时无返回值。
    ///
    /// # 错误
    /// 删除失败时返回对应错误。
    async fn delete_items(&mut self, statement_id: &str, executor: &mut dyn Executor) -> Result<()>;
    /// 写回调用方已经修改的结算单。
    ///
    /// # 参数
    /// * `statement` - 待写回的结算单
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 写回成功时无返回值。
    ///
    /// # 错误
    /// 写回失败时返回对应错误。
    async fn update_statement(
        &mut self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 插入新明细。
    ///
    /// # 参数
    /// * `items` - 待插入的明细
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入成功时无返回值。
    ///
    /// # 错误
    /// 插入失败时返回对应错误。
    async fn insert_items(
        &mut self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 插入新差异。
    ///
    /// # 参数
    /// * `differences` - 待插入的差异
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入成功时无返回值。
    ///
    /// # 错误
    /// 插入失败时返回对应错误。
    async fn insert_differences(
        &mut self,
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// 唯一生产 Store，保留原集合、物理过滤器及 owned CAS provider。
pub(super) struct MongoDraftSnapshotStore<'a> {
    pub(super) db: &'a Database,
}

#[async_trait]
impl DraftSnapshotStore for MongoDraftSnapshotStore<'_> {
    async fn delete_evidence(
        &mut self,
        difference_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::delete_many(
            &self
                .db
                .collection::<SupplierSettlementDifferenceEvidence>(SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE),
            doc! { "difference_id": { "$in": difference_ids } },
            executor,
        )
        .await?;
        Ok(())
    }

    async fn delete_differences(&mut self, item_ids: &[String], executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::delete_many(
            &self.db.collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
            doc! { "statement_item_id": { "$in": item_ids } },
            executor,
        )
        .await?;
        Ok(())
    }

    async fn delete_items(&mut self, statement_id: &str, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::delete_many(
            &self.db.collection::<SupplierSettlementItem>(SUPPLIER_SETTLEMENT_ITEMS),
            doc! { "statement_id": statement_id },
            executor,
        )
        .await?;
        Ok(())
    }

    async fn update_statement(
        &mut self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        SupplierSettlementStatementRepository::new(self.db, SUPPLIER_SETTLEMENT_STATEMENTS)
            .update(statement, executor)
            .await
    }

    async fn insert_items(
        &mut self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self.db.collection::<SupplierSettlementItem>(SUPPLIER_SETTLEMENT_ITEMS),
            items,
            executor,
        )
        .await?;
        Ok(())
    }

    async fn insert_differences(
        &mut self,
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self.db.collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
            differences,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 按固定次序物理替换草稿快照；任一步失败则停止后续写入。
///
/// 次序为：非空 `old_difference_ids` 时删补证，非空 `old_item_ids` 时删差异，
/// 然后删除该结算单明细、写回结算单、插入 `items`（即使为空也调用），
/// 最后在 `differences` 非空时插入差异。本函数不开启事务。
///
/// # 参数
/// * `store` - 草稿替换的写入端口
/// * `statement` - 待写回的结算单
/// * `old_item_ids` - 旧明细主键；空则跳过按明细删除差异
/// * `old_difference_ids` - 旧差异主键；空则跳过删除补证
/// * `items` - 新明细；即使为空也会交给 `store`
/// * `differences` - 新差异；空则不插入
/// * `executor` - 贯穿各步的同一执行器
///
/// # 返回
/// 已执行步骤全部成功时无返回值。
///
/// # 错误
/// 任一步失败时返回该步错误，不再执行后续步骤。
pub(super) async fn replace_snapshot<S: DraftSnapshotStore>(
    store: &mut S,
    statement: &mut SupplierSettlementStatement,
    old_item_ids: &[String],
    old_difference_ids: &[String],
    items: &[SupplierSettlementItem],
    differences: &[SupplierSettlementDifference],
    executor: &mut dyn Executor,
) -> Result<()> {
    if !old_difference_ids.is_empty() {
        store.delete_evidence(old_difference_ids, executor).await?;
    }
    if !old_item_ids.is_empty() {
        store.delete_differences(old_item_ids, executor).await?;
    }
    store.delete_items(&statement.base.id, executor).await?;
    store.update_statement(statement, executor).await?;
    store.insert_items(items, executor).await?;
    if !differences.is_empty() {
        store.insert_differences(differences, executor).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
