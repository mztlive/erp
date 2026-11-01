//! 财务明确授予的有限额自动余额执行权限。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{add, positive, required};

/// 自动执行授权与出纳本次明确执行分别保存，不相互扩权。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DepositAuthorizationMode {
    #[default]
    Automatic,
    Manual {
        work_item_id: String,
        task_version: u64,
    },
}

/// 由已授权出纳任务冻结的一次执行范围；金额来自服务器执行预算。
pub struct ManualDepositAuthorizationData {
    pub account_id: String,
    pub purchase_order_id: String,
    pub work_item_id: String,
    pub task_version: u64,
    pub actor_id: String,
    pub approval_reference: String,
    pub amount: Amount,
    pub now: Instant,
}

/// 有效授权范围；空采购范围表示本账户内已逐单批准采购。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierDepositAuthorization {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub authorized_by: String,
    pub approval_reference: String,
    /// 授权条款版本；累计占用的 CAS 更新不改变该版本。
    pub grant_version: u64,
    pub valid_from: Instant,
    pub valid_until: Instant,
    pub per_execution_limit: Amount,
    pub cumulative_limit: Amount,
    pub committed_amount: Amount,
    pub purchase_order_ids: Vec<String>,
    pub revoked: bool,
    #[serde(default)]
    pub mode: DepositAuthorizationMode,
}

impl SupplierDepositAuthorization {
    /// 为原指定出纳已明确提交的单执行组建立有限使用依据。
    /// # 参数
    /// * `id` - 稳定原命令与执行组形成的授权身份。
    /// * `data` - 服务器核验后的任务、采购及全额预算。
    /// # 返回
    /// 单采购、单组全额、24小时内有效的人工执行授权。
    /// # 错误
    /// 缺少原任务、批准依据、金额或时间越界时拒绝。
    pub fn manual(id: String, data: ManualDepositAuthorizationData) -> Result<Self> {
        positive(data.amount)?;
        if data.task_version == 0 {
            return Err(Error::from("人工余额执行须绑定有效付款任务版本"));
        }
        let valid_until =
            data.now.unix_secs().checked_add(86_400).ok_or_else(|| Error::from("人工授权期限越界"))?;
        Ok(Self {
            base: BaseModel::new(required(id, "人工执行授权")?),
            account_id: required(data.account_id, "预存款账户")?,
            authorized_by: required(data.actor_id, "原任务出纳")?,
            approval_reference: required(data.approval_reference, "原付款申请批准依据")?,
            grant_version: 1,
            valid_from: data.now,
            valid_until: Instant::try_from_unix_secs(valid_until)?,
            per_execution_limit: data.amount,
            cumulative_limit: data.amount,
            committed_amount: Amount::zero(),
            purchase_order_ids: vec![required(data.purchase_order_id, "原采购")?],
            revoked: false,
            mode: DepositAuthorizationMode::Manual {
                work_item_id: required(data.work_item_id, "原付款执行任务")?,
                task_version: data.task_version,
            },
        })
    }

    /// 重验已占用执行的授权，不再次消费累计限额。
    /// # 参数
    /// * `account` / `purchase` - 原资金归属与采购来源。
    /// * `version` - 原授权条款版本。
    /// * `now` - 当前服务器时间。
    /// # 返回
    /// 有效时允许尚未发送的外部写动作。
    /// # 错误
    /// 撤销、到期、条款变化或跨范围时拒绝。
    pub fn ensure_active(&self, account: &str, purchase: &str, version: u64, now: Instant) -> Result<()> {
        self.validate_mode()?;
        if matches!(self.mode, DepositAuthorizationMode::Manual { .. })
            && self.committed_amount != self.cumulative_limit
        {
            return Err(Error::from("人工余额执行尚未绑定原执行组全额预算"));
        }
        if self.account_id != account
            || self.revoked
            || self.grant_version != version
            || now < self.valid_from
            || now >= self.valid_until
            || (!self.purchase_order_ids.is_empty()
                && !self.purchase_order_ids.iter().any(|id| id == purchase))
        {
            return Err(Error::from("预存款自动执行授权已失效或超出范围"));
        }
        Ok(())
    }
    /// 校验有限授权并记录本次预算；授权不替代逐单付款审批。
    /// # 参数
    /// * `account` / `purchase` - 当前资金账号及采购来源。
    /// * `amount` - 本次全额预算。
    /// * `version` - 冻结授权版本。
    /// * `now` - 服务端当前时间。
    /// # 返回
    /// 成功增加累计授予额度，释放资金不恢复累计自动执行额度。
    /// # 错误
    /// 归属、状态、时间、版本、限额或授权依据不合法时拒绝。
    pub fn commit(
        &mut self,
        account: &str,
        purchase: &str,
        amount: Amount,
        version: u64,
        now: Instant,
    ) -> Result<()> {
        self.validate_mode()?;
        if matches!(self.mode, DepositAuthorizationMode::Manual { .. }) && amount != self.per_execution_limit
        {
            return Err(Error::from("人工余额执行只能占用原执行组全额预算"));
        }
        positive(amount)?;
        required(self.authorized_by.clone(), "授权人")?;
        required(self.approval_reference.clone(), "财务授权依据")?;
        if self.account_id != account
            || self.revoked
            || self.grant_version != version
            || now < self.valid_from
            || now >= self.valid_until
            || self.valid_from >= self.valid_until
            || self.per_execution_limit <= Amount::zero()
            || self.cumulative_limit <= Amount::zero()
            || amount > self.per_execution_limit
            || (!self.purchase_order_ids.is_empty()
                && !self.purchase_order_ids.iter().any(|id| id == purchase))
        {
            return Err(Error::from("预存款自动执行授权缺失、失效或超出范围"));
        }
        let total = add(self.committed_amount, amount)?;
        if self.committed_amount < Amount::zero() || total > self.cumulative_limit {
            return Err(Error::from("预存款自动执行累计授权额度不足"));
        }
        self.committed_amount = total;
        Ok(())
    }

    fn validate_mode(&self) -> Result<()> {
        if let DepositAuthorizationMode::Manual { work_item_id, task_version } = &self.mode {
            required(work_item_id.clone(), "原人工付款任务")?;
            if *task_version == 0
                || self.purchase_order_ids.len() != 1
                || self.per_execution_limit != self.cumulative_limit
            {
                return Err(Error::from("人工余额执行不得扩展为跨采购或多次自动使用授权"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual() -> SupplierDepositAuthorization {
        SupplierDepositAuthorization::manual(
            "manual-grant".into(),
            ManualDepositAuthorizationData {
                account_id: "account".into(),
                purchase_order_id: "purchase".into(),
                work_item_id: "task".into(),
                task_version: 3,
                actor_id: "cashier".into(),
                approval_reference: "approved-request-and-task".into(),
                amount: "100".parse().unwrap(),
                now: Instant::from_unix_secs(10),
            },
        )
        .unwrap()
    }

    #[test]
    fn manual_execution_is_one_full_group_and_never_general_automatic_authority() {
        let mut grant = manual();
        assert_eq!(
            grant.mode,
            DepositAuthorizationMode::Manual { work_item_id: "task".into(), task_version: 3 }
        );
        assert_eq!(grant.valid_until.unix_secs(), 86_410);
        assert!(grant.ensure_active("account", "purchase", 1, Instant::from_unix_secs(11)).is_err());
        assert!(
            grant
                .commit("account", "purchase", "50".parse().unwrap(), 1, Instant::from_unix_secs(11))
                .is_err()
        );
        assert!(
            grant
                .commit("account", "other-purchase", "100".parse().unwrap(), 1, Instant::from_unix_secs(11))
                .is_err()
        );
        grant.commit("account", "purchase", "100".parse().unwrap(), 1, Instant::from_unix_secs(11)).unwrap();
        grant.ensure_active("account", "purchase", 1, Instant::from_unix_secs(12)).unwrap();
        assert!(
            grant
                .commit("account", "purchase", "100".parse().unwrap(), 1, Instant::from_unix_secs(12))
                .is_err()
        );
        assert!(grant.ensure_active("account", "purchase", 1, grant.valid_until).is_err());
        let mut expanded = grant.clone();
        expanded.purchase_order_ids.push("another".into());
        assert!(expanded.ensure_active("account", "purchase", 1, Instant::from_unix_secs(12)).is_err());
    }

    #[test]
    fn legacy_grants_keep_automatic_mode_and_manual_identity_roundtrips() {
        let original = manual();
        let wire = serde_json::to_value(&original).unwrap();
        assert_eq!(serde_json::from_value::<SupplierDepositAuthorization>(wire.clone()).unwrap(), original);
        let mut legacy = wire;
        legacy.as_object_mut().unwrap().remove("mode");
        let restored: SupplierDepositAuthorization = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.mode, DepositAuthorizationMode::Automatic);
    }
}
