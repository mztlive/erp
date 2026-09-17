//! 供应商结算单域准备、查询和事务内写入；跨域根事务由 supply_settlement 组合。
pub mod access;
pub mod difference;
pub mod draft;
pub mod evidence;
mod item;
pub mod query;
pub mod review;
pub mod shared;
pub mod source;
mod void;
use std::sync::Arc;

pub use access::{SettlementAccess, settlement_scope};
use mongodb::Database;
use persistence_core::Executor;
use shared::*;

use crate::dto::supplier_settlement as dto;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementItem, SupplierSettlementStatement,
};
use crate::ports::{FailClosedSettlementDataScopePort, SettlementDataScopePort};
use crate::repository::SupplierSettlementExt;
use crate::{Error, Result};

/// 供应商结算本域服务，调用者提供原事务执行器。
pub struct SupplierSettlementService {
    db: Database,
    data_scope: Arc<dyn SettlementDataScopePort>,
}
impl SupplierSettlementService {
    /// 创建供应商结算服务实例；范围 Port 缺省失败关闭。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db, data_scope: FailClosedSettlementDataScopePort::shared() }
    }

    /// 注入结算范围 Port。
    ///
    /// # 参数
    /// * `data_scope` - 组合层装配的公共解析 adapter
    ///
    /// # 返回
    /// 返回绑定范围 Port 的服务。
    pub fn with_data_scope(mut self, data_scope: Arc<dyn SettlementDataScopePort>) -> Self {
        self.data_scope = data_scope;
        self
    }

    /// 构造本域范围访问器。
    pub fn access(&self) -> SettlementAccess {
        SettlementAccess::new(self.db.clone(), self.data_scope.clone())
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
        self.db.supplier_settlement_statements().update(statement, executor).await?;
        Ok(())
    }
}

mod evidence_posting;
