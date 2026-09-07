//! 供应商结算单域准备、查询和事务内写入；跨域根事务由 supply_settlement 组合。
pub mod difference;
pub mod draft;
pub mod evidence;
mod item;
pub mod query;
pub mod review;
pub mod shared;
pub mod source;
mod void;
use crate::dto::supplier_settlement as dto;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use crate::repository::SupplierSettlementExt;
use crate::{Error, Result};
use mongodb::Database;
use persistence_core::Executor;
use shared::*;
/// 供应商结算本域服务，调用者提供原事务执行器。
pub struct SupplierSettlementService {
    db: Database,
}
impl SupplierSettlementService {
    /// 创建供应商结算服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 按 ID 加载未删除结算单。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回结算单实体。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    pub async fn load_statement(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatement> {
        self.db
            .supplier_settlement_statements()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))
    }
    /// 加载结算单全部冻结明细。
    pub async fn load_statement_items(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        shared::load_statement_items(&self.db, id, executor).await
    }
    /// 加载结算明细关联的全部正式差异。
    pub async fn load_statement_differences(
        &self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifference>> {
        shared::load_statement_differences(&self.db, items, executor).await
    }
    /// 按原 CAS 更新结算单，不开启事务或追加外域副作用。
    pub async fn persist_statement(
        &self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_settlement_statements()
            .update(statement, executor)
            .await?;
        Ok(())
    }
}

mod evidence_posting;
