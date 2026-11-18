//! 供应商结算单域准备、查询和事务内写入；跨域根事务由 supply_settlement 组合。
pub mod access;
pub(crate) mod authorization;
pub mod difference;
pub mod draft;
pub mod evidence;
mod item;
pub mod query;
pub mod review;
pub mod shared;
pub mod source;
mod void;
pub use access::SettlementAccess;
pub use authorization::{SettlementAction, SettlementActionAccess};
use mongodb::Database;
use persistence_core::Executor;
use shared::*;

use crate::dto::supplier_settlement as dto;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use crate::repository::SupplierSettlementExt;
use crate::{Error, Result};

/// 供应商结算本域服务，调用者提供原事务执行器。
pub struct SupplierSettlementService {
    db: Database,
    data_scope: SettlementAccess,
}
impl SupplierSettlementService {
    /// 创建供应商结算服务实例；授权访问器 缺省失败关闭。
    pub fn new(db: Database) -> Self {
        Self { db: db.clone(), data_scope: SettlementAccess::unwired(db.clone()) }
    }

    /// 注入结算授权访问器。
    /// 返回绑定授权访问器 的服务。
    /// `data_scope`：组合层装配的本域授权访问器。
    pub fn with_data_scope(mut self, data_scope: SettlementAccess) -> Self {
        self.data_scope = data_scope;
        self
    }

    /// 构造本域范围访问器。
    /// 返回绑定当前数据库和授权访问器 的 `SettlementAccess`。
    pub fn access(&self) -> SettlementAccess {
        self.data_scope.clone()
    }
    /// 按 ID 加载未删除结算单。
    /// 返回结算单实体。
    /// 失败条件：
    /// * `NotFound` - 结算单不存在
    /// 仓储读取失败时向上传播错误。
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
    /// 返回该结算单的明细；没有明细时为空向量。
    /// 仓储读取失败时向上传播错误。
    pub async fn load_statement_items(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        shared::load_statement_items(&self.db, id, executor).await
    }
    /// 加载结算明细关联的全部正式差异。
    /// 返回这些明细上的差异；没有差异时为空向量。
    /// 仓储读取失败时向上传播错误。
    pub async fn load_statement_differences(
        &self,
        items: &[SupplierSettlementItem],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifference>> {
        shared::load_statement_differences(&self.db, items, executor).await
    }
    /// 按原 CAS 更新结算单，不开启事务或追加外域副作用。
    /// 结算单已写回。
    /// 仓储更新失败时向上传播错误。
    /// `statement`：待按 CAS 写回的结算单。
    pub async fn persist_statement(
        &self,
        statement: &mut SupplierSettlementStatement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_settlement_statements().update(statement, executor).await?;
        Ok(())
    }
}

mod evidence_posting;
