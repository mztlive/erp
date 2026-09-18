//! 跨集合多步骤事务写入：处理单/退货单与其明细必须同事务可见。

use mongodb::Database;
use persistence_core::{Executor, Result, mongo_ops};

use super::super::extensions::ReturnsExt;
use crate::entity::returns::{PurchaseReturnLine, PurchaseReturnOrder, SalesReturnCase, SalesReturnLine};

/// `sales_return_line` 集合名（单一来源：`ReturnsExt` 关联常量）。
const SALES_RETURN_LINES: &str = <mongodb::Database as ReturnsExt>::SALES_RETURN_LINES;
/// `purchase_return_line` 集合名（单一来源：`ReturnsExt` 关联常量）。
const PURCHASE_RETURN_LINES: &str = <mongodb::Database as ReturnsExt>::PURCHASE_RETURN_LINES;

/// D21 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`persistence_core::Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `ReturnsExt::returns()` 访问。
pub struct ReturnsRepository<'a> {
    db: &'a Database,
}

impl<'a> ReturnsRepository<'a> {
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

    /// 建立销售退货处理单与其明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `sales_return_cases` 与 `sales_return_lines`，保证「处理单 + 明细」
    /// 原子可见（数据模型 §6.11）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有处理单没有明细的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `case_entity` - 待写入的处理单
    /// * `line` - 待写入的明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sales_return_with_line(
        &self,
        case_entity: &SalesReturnCase,
        line: &SalesReturnLine,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SalesReturnCase>(<mongodb::Database as ReturnsExt>::SALES_RETURN_CASES),
            case_entity,
            executor,
        )
        .await?;
        mongo_ops::insert_one(&self.db.collection::<SalesReturnLine>(SALES_RETURN_LINES), line, executor)
            .await?;
        Ok(())
    }

    /// 建立采购退货单与其明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `purchase_return_orders` 与 `purchase_return_lines`，保证
    /// 「退货单 + 明细」原子可见（数据模型 §6.11）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有退货单没有明细的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `order` - 待写入的采购退货单
    /// * `line` - 待写入的明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_purchase_return_with_line(
        &self,
        order: &PurchaseReturnOrder,
        line: &PurchaseReturnLine,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseReturnOrder>(<mongodb::Database as ReturnsExt>::PURCHASE_RETURN_ORDERS),
            order,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<PurchaseReturnLine>(PURCHASE_RETURN_LINES),
            line,
            executor,
        )
        .await?;
        Ok(())
    }
}
