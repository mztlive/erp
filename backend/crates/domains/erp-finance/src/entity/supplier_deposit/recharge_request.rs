//! 充值申请与实际银行付款、供应商到账各阶段分离。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{positive, required};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DepositRechargeStatus {
    Requested,
    Approved,
    BankPaid,
    Received,
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositRechargeRequest {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_no: String,
    pub account_id: String,
    pub operation_key: String,
    pub payload_fingerprint: String,
    pub amount: Amount,
    pub reason: String,
    pub requested_by: String,
    pub status: DepositRechargeStatus,
    pub approved_by: Option<String>,
    pub approved_at: Option<Instant>,
    pub approval_reference: Option<String>,
    pub supplier_payment_id: Option<String>,
    pub received_batch_id: Option<String>,
}

impl SupplierDepositRechargeRequest {
    /// 创建尚不可用的充值申请。
    /// 返回待独立批准申请。
    /// 金额、理由或身份非法时拒绝。
    ///
    /// 输入约定：
    /// - `id` / `account` / `operation` / `fingerprint`：服务端身份和稳定命令。
    /// - `amount` / `reason` / `actor`：申请金额、用途及申请人。
    pub fn new(
        id: String,
        account: String,
        operation: String,
        fingerprint: String,
        amount: Amount,
        reason: String,
        actor: String,
    ) -> Result<Self> {
        positive(amount)?;
        let id = required(id, "充值申请")?;
        Ok(Self {
            request_no: format!("CZSQ-{id}"),
            base: BaseModel::new(id),
            account_id: required(account, "预存款账户")?,
            operation_key: required(operation, "充值操作")?,
            payload_fingerprint: fingerprint,
            amount,
            reason: required(reason, "充值说明")?,
            requested_by: required(actor, "充值申请人")?,
            status: DepositRechargeStatus::Requested,
            approved_by: None,
            approved_at: None,
            approval_reference: None,
            supplier_payment_id: None,
            received_batch_id: None,
        })
    }

    /// 由具备财务批准权限的另一位人员批准，批准不形成现金或到账。
    /// 结果：申请进入待银行付款。
    /// 非待批准、自己批准或依据不合法时拒绝。
    ///
    /// 输入约定：
    /// - `actor` / `reference` / `now`：实际批准人、批准依据及时间。
    pub fn approve(&mut self, actor: &str, reference: String, now: Instant) -> Result<()> {
        if self.status != DepositRechargeStatus::Requested || self.requested_by == actor {
            return Err(Error::from("充值申请需由另一位财务人员批准"));
        }
        let reference = required(reference, "充值批准依据")?;
        let actor = required(actor.into(), "充值批准人")?;

        self.status = DepositRechargeStatus::Approved;
        self.approved_by = Some(actor);
        self.approved_at = Some(now);
        self.approval_reference = Some(reference);
        Ok(())
    }
}
