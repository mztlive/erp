//! 结算单创建及草稿快照原写序。

mod draft_write;

use mongodb::Database;
use persistence_core::{Executor, Result, mongo_ops};

use super::{SUPPLIER_SETTLEMENT_DIFFERENCES, SUPPLIER_SETTLEMENT_ITEMS, SUPPLIER_SETTLEMENT_STATEMENTS};
use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};

/// D33 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SupplierSettlementExt::supplier_settlement()` 访问。
pub struct SupplierSettlementRepository<'a> {
    pub(super) db: &'a Database,
}

impl<'a> SupplierSettlementRepository<'a> {
    /// 绑定供应商结算集合所在的数据库，供调用方通过仓储访问本域记录。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 依次写入结算单、全部明细，以及非空的结算差异。
    /// 写入顺序为 `supplier_settlement_statements`、`supplier_settlement_items`，再在 `differences` 非空
    /// 时写入 `supplier_settlement_differences`。本方法不构成原子边界，传入 `NoTransaction` 时各笔各自自
    /// 动提交，中途失败会留下已经成功的前序写入；Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    /// 已执行的写入全部成功时无返回值。
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射为冲突语义）或
    /// MongoDB 写入失败时返回错误。
    /// `differences`：空切片不写差异集合。
    /// `executor`：数据访问执行器，必须位于事务中。
    pub async fn create_statement_with_items(
        &self,
        statement: &SupplierSettlementStatement,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierSettlementStatement>(SUPPLIER_SETTLEMENT_STATEMENTS),
            statement,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<SupplierSettlementItem>(SUPPLIER_SETTLEMENT_ITEMS),
            items,
            executor,
        )
        .await?;

        if !differences.is_empty() {
            mongo_ops::insert_many(
                &self.db.collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
                differences,
                executor,
            )
            .await?;
        }
        Ok(())
    }

    /// 按原次序物理替换尚未提交复核的草稿快照。
    /// 本方法不校验版本或状态，也不开启事务；服务层须先在事务内重验。已提交复核或终态不得调用。空的新明
    /// 细仍会交给替换流程。
    /// 替换步骤全部成功时无返回值。
    /// 任一步写入失败时返回对应错误，不再执行后续步骤。
    ///
    /// `old_item_ids`：空则跳过按明细删除差异。
    /// `old_difference_ids`：空则跳过删除补证。
    /// `differences`：空则不插入。
    pub async fn replace_draft_snapshot(
        &self,
        statement: &mut SupplierSettlementStatement,
        old_item_ids: &[String],
        old_difference_ids: &[String],
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        draft_write::replace_snapshot(
            &mut draft_write::MongoDraftSnapshotStore { db: self.db },
            statement,
            old_item_ids,
            old_difference_ids,
            items,
            differences,
            executor,
        )
        .await
    }
}
