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
pub(super) trait DraftSnapshotStore: Send {
    async fn delete_evidence(&mut self, difference_ids: &[String], executor: &mut dyn Executor)
    -> Result<()>;
    async fn delete_differences(&mut self, item_ids: &[String], executor: &mut dyn Executor) -> Result<()>;
    async fn delete_items(&mut self, statement_id: &str, executor: &mut dyn Executor) -> Result<()>;
    async fn update_statement(
        &mut self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn insert_items(
        &mut self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<()>;
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
            items.to_vec(),
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
            differences.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 依原条件与次序物理替换；空新明细仍调用原 insert_many provider。
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
