//! 默认收款账户联行号在资料修订事务内读取、校验与保存。

use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;
use erp_party::repository::prelude::*;
use erp_party::{PartyBankAccount, PartyExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub(super) struct BankRoutingChange {
    pub(super) party_id: PartyId,
    pub(super) value: String,
    pub(super) actor_id: String,
}

impl BankRoutingChange {
    /// 以同一事务内的默认账户事实保存联行号，保留版本冲突和原资料审计。
    ///
    /// # 参数
    /// * `db` / `executor` - 当前数据库与供应商资料修订执行器。
    /// # 返回
    /// 完成更新；内容一致时不写入账户。
    /// # 错误
    /// 默认账户缺失或不唯一、格式非法或账户版本变化时拒绝。
    pub(super) async fn persist(self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        let accounts =
            db.party_bank_accounts().list_current_on(&self.party_id, BusinessDate::today(), executor).await?;
        let Some(account) = PartyBankAccount::resolve_current_default(&accounts)? else {
            if self.value.trim().is_empty() {
                return Ok(());
            }
            return Err(Error::ValidationError("请先维护默认收款银行账户".into()));
        };
        if account.bank_routing_number.as_deref().unwrap_or("") == self.value.trim() {
            return Ok(());
        }
        let mut account = account.clone();
        account.set_routing_number(Some(self.value), self.actor_id)?;
        db.party_bank_accounts().update(&mut account, executor).await?;
        Ok(())
    }
}
