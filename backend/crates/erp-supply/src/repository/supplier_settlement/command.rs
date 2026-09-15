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
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 原子创建结算单与全部结算明细。
    ///
    /// 依次写入 `supplier_settlement_statements` 与 `supplier_settlement_items`，
    /// 保证「结算单 + 明细」同事务可见（§6.20：完成、取消和退款事实均参与结算，
    /// 结算单确认与成本差额、应付账户及原始应付分录在同一事务完成，P3 编排）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有结算单没有明细的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `statement` - 待写入的结算单
    /// * `items` - 待写入的全部结算明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
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
            items.to_vec(),
            executor,
        )
        .await?;
        if !differences.is_empty() {
            mongo_ops::insert_many(
                &self.db.collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
                differences.to_vec(),
                executor,
            )
            .await?;
        }
        Ok(())
    }

    /// 原子替换尚未提交复核的草稿快照。
    ///
    /// 旧明细与差异仅属于可变草稿试算；服务层在事务内重验版本和状态后物理替换，
    /// 旧差异补证随其草稿差异一并移除，新快照及审计同时可见。已提交复核或终态
    /// 不得调用。
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
