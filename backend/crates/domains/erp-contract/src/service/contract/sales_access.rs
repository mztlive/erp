//! 合同明确共享只影响本合同读取与选用，不交接客户、销售单或历史业绩。

use application_core::AuditActor;
use validator::Validate;

use super::{ContractService, TxAuthorization, execute_authorized_transaction};
use crate::dto::contract::{ContractDetailView, ContractSalesAccessRequest};
use crate::error::{Error, Result};
use crate::repository::ContractExt;

impl ContractService {
    /// 在合同更新范围内维护明确共享销售。
    ///
    /// # 参数
    /// * `id` - 合同身份
    /// * `request` - 期望版本、共享人员和原因
    /// * `actor` - 已证明合同更新及客户分配功能权限的操作人
    /// # 返回
    /// 返回更新后的合同详情。
    /// # 错误
    /// 权限失效、版本冲突、人员无效或事务失败时拒绝。
    pub async fn set_sales_access(
        &self,
        id: &str,
        request: ContractSalesAccessRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        request.validate()?;
        let db = self.db.clone();
        let access = self.access();
        let audit = self.audit.resource_log(actor.clone(), "contract.share", "contract", id.to_string())?;
        let accounts = self.accounts.clone();
        let actor_id = actor.id().to_string();
        let contract_id = id.to_string();
        execute_authorized_transaction(
            &db,
            &self.audit,
            &audit,
            TxAuthorization::Use(contract_id.clone()),
            access,
            actor.clone(),
            Box::new(move |db, executor| {
                Box::pin(async move {
                    let mut contract = db
                        .contracts()
                        .find_by_id(&contract_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("合同不存在或无权修改".into()))?;
                    if !contract.matches_version(request.version) {
                        return Err(Error::ConflictError("合同已被修改，请刷新后重新核对共享人员".into()));
                    }
                    contract.set_shared_sales(request.shared_sales_user_ids)?;
                    contract.sales_access_change_reason = Some(request.change_reason.trim().to_string());
                    contract.stable.updated_by = actor_id;
                    accounts.ensure_enabled(&contract.shared_sales_user_ids, executor).await?;
                    db.contracts().update(&mut contract, executor).await?;
                    Ok(())
                })
            }),
        )
        .await?;
        self.contract_detail(id, actor).await
    }
}
