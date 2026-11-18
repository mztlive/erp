//! 客户写命令在原执行器内装载客户与主体。
use erp_core::ids::PartyId;
use erp_customer::repository::prelude::*;
use erp_customer::{CustomerAccount, CustomerExt};
use erp_party::repository::prelude::*;
use erp_party::{Party, PartyExt};
use persistence_core::Executor;

use super::CustomerProfileProcess;
use crate::{Error, Result};
impl CustomerProfileProcess {
    // 按主键加载客户角色。
    //
    // `id`：客户角色主键。
    //
    // 返回找到的客户角色。
    //
    // 客户不存在时返回未找到；查询失败时返回仓储错误。
    pub(super) async fn load_customer(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerAccount> {
        self.db
            .customer_accounts()
            .find_customer(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))
    }

    // 加载客户关联的主体。
    //
    // `party_id`：客户角色上的主体主键。
    //
    // 返回对应主体。
    //
    // 主体不存在时返回未找到；查询失败时返回仓储错误。
    pub(super) async fn load_party(&self, party_id: &PartyId, executor: &mut dyn Executor) -> Result<Party> {
        self.db
            .parties()
            .find_party(party_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户关联主体不存在".to_string()))
    }
}
