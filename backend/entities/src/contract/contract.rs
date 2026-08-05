//! `contract`：合同稳定主表（数据模型 §6.4 / W04）。
//!
//! 合同是「稳定基础资料」，组合 [`StableBase`]；ERP 不产生空合同草稿，合同只能
//! 通过上传已签署 PDF 归档（W04 禁止新建、编辑、提交生效），首个不可变版本与
//! PDF 关联在事务内同时形成。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::{Error, Result};
use crate::ids::{ContractId, CustomerAccountId, PartyId};
use crate::validation::normalize_required_text;

/// 合同编号最大长度。
const CONTRACT_NO_MAX_LEN: usize = 64;

/// 合同状态（数据模型 §6.4：生效、终止、到期；W04 禁止产生 `DRAFT` 合同）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ContractStatus {
    /// 生效。
    Effective,
    /// 终止。
    Terminated,
    /// 到期。
    Expired,
}

impl ContractStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Effective => "生效",
            Self::Terminated => "终止",
            Self::Expired => "到期",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Effective => "EFFECTIVE",
            Self::Terminated => "TERMINATED",
            Self::Expired => "EXPIRED",
        }
    }
}

impl DocumentState for ContractStatus {
    /// 生效后可终止或到期；终止/到期为终态（W04 无重新激活入口）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Effective => &[Self::Terminated, Self::Expired],
            Self::Terminated | Self::Expired => &[],
        }
    }
}

/// 合同创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractData {
    /// 合同编号（唯一，大小写规范由服务端统一）。
    pub contract_no: String,
    /// 客户。
    pub customer_id: CustomerAccountId,
    /// 结算主体。
    pub settlement_party_id: PartyId,
}

/// 合同更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContractUpdate {
    /// 客户；`None` 表示不修改。
    pub customer_id: Option<CustomerAccountId>,
    /// 结算主体；`None` 表示不修改。
    pub settlement_party_id: Option<PartyId>,
}

/// 合同实体（稳定基础资料，数据模型 §6.4）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Contract {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<ContractStatus>,
    /// 合同编号（唯一，创建后不可修改）。
    pub contract_no: String,
    /// 客户。
    pub customer_id: CustomerAccountId,
    /// 结算主体。
    pub settlement_party_id: PartyId,
}

impl PartialEq for Contract {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.contract_no == other.contract_no
            && self.customer_id == other.customer_id
            && self.settlement_party_id == other.settlement_party_id
    }
}

impl Eq for Contract {}

impl Contract {
    /// 创建合同（首个不可变版本与 PDF 关联由 P3 事务同时形成）。
    ///
    /// 完成 contract_no 的校验与规范化（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ContractId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的合同实体，状态为 `Effective`。
    ///
    /// # 错误
    /// 当 contract_no 为空或超长时返回错误。
    pub fn new(id: ContractId, data: ContractData, created_by: impl Into<String>) -> Result<Self> {
        let contract_no = normalize_required_text(
            data.contract_no,
            "合同编号不能为空",
            CONTRACT_NO_MAX_LEN,
            "合同编号过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(ContractStatus::Effective, created_by),
            contract_no,
            customer_id: data.customer_id,
            settlement_party_id: data.settlement_party_id,
        })
    }

    /// 更新合同（复用 `new` 的校验规则）。
    ///
    /// `contract_no` 是稳定编号，不允许在通用更新中修改；已终止或已到期的合同
    /// 不允许再修改（历史版本永久保留）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 合同已终止/到期，或更新字段校验失败时返回错误。
    pub fn update(&mut self, update: ContractUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.ensure_mutable()?;
        if let Some(customer_id) = update.customer_id {
            self.customer_id = customer_id;
        }
        if let Some(settlement_party_id) = update.settlement_party_id {
            self.settlement_party_id = settlement_party_id;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 终止合同（W04：授权终止，历史销售引用保持不变）。
    ///
    /// # 参数
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 状态迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非 `Effective` 状态下调用时返回 [`Error::InvalidStateTransition`]。
    pub fn terminate(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition(ContractStatus::Terminated, updated_by)
    }

    /// 标记合同到期。
    ///
    /// # 参数
    /// * `updated_by` - 操作人（通常为系统任务）
    ///
    /// # 返回
    /// 状态迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非 `Effective` 状态下调用时返回 [`Error::InvalidStateTransition`]。
    pub fn expire(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.transition(ContractStatus::Expired, updated_by)
    }

    /// 绑定当前生效版本（归档新 PDF 版本后切换当前版本指针）。
    ///
    /// # 参数
    /// * `revision_id` - 新合同版本主键
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 无返回值；更新当前版本指针并记录更新人。
    pub fn attach_revision(&mut self, revision_id: impl Into<String>, updated_by: impl Into<String>) {
        self.stable.current_revision_id = Some(revision_id.into());
        self.stable.touch(updated_by);
    }

    /// 执行一次固定状态机迁移。
    ///
    /// # 参数
    /// * `to` - 目标状态
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 迁移合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 迁移非法时返回 [`Error::InvalidStateTransition`]。
    fn transition(&mut self, to: ContractStatus, updated_by: impl Into<String>) -> Result<()> {
        ensure_transition(self.stable.status, to)?;
        self.stable.status = to;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 校验合同是否允许修改业务字段。
    ///
    /// # 返回
    /// 可修改时返回 `Ok(())`。
    ///
    /// # 错误
    /// 已终止或已到期时返回错误。
    fn ensure_mutable(&self) -> Result<()> {
        if matches!(
            self.stable.status,
            ContractStatus::Terminated | ContractStatus::Expired
        ) {
            return Err(Error::from("已终止或已到期的合同不允许修改"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ContractId;

    fn data() -> ContractData {
        ContractData {
            contract_no: " HT-2026-0088 ".to_string(),
            customer_id: CustomerAccountId::new("cust-1"),
            settlement_party_id: PartyId::new("party-1"),
        }
    }

    #[test]
    fn new_trims_and_initializes_stable_fields() {
        let contract = Contract::new(ContractId::new("c-1"), data(), "admin-1").unwrap();

        assert_eq!(contract.contract_no, "HT-2026-0088");
        assert_eq!(contract.stable.status(), ContractStatus::Effective);
        assert_eq!(contract.stable.created_by, "admin-1");
        assert_eq!(contract.stable.updated_by, "admin-1");
        assert!(contract.stable.current_revision_id.is_none());
    }

    #[test]
    fn new_rejects_blank_and_overlong_contract_no() {
        let blank = ContractData {
            contract_no: "   ".to_string(),
            ..data()
        };
        assert!(Contract::new(ContractId::new("c-1"), blank, "admin-1").is_err());

        let overlong = ContractData {
            contract_no: "x".repeat(65),
            ..data()
        };
        assert!(Contract::new(ContractId::new("c-1"), overlong, "admin-1").is_err());
    }

    #[test]
    fn update_applies_fields_and_keeps_contract_no() {
        let mut contract = Contract::new(ContractId::new("c-1"), data(), "admin-1").unwrap();
        contract
            .update(
                ContractUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-2")),
                    settlement_party_id: None,
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(contract.customer_id, CustomerAccountId::new("cust-2"));
        assert_eq!(contract.contract_no, "HT-2026-0088");
        assert_eq!(contract.stable.updated_by, "admin-2");
    }

    #[test]
    fn terminated_or_expired_contract_rejects_update() {
        let mut contract = Contract::new(ContractId::new("c-1"), data(), "admin-1").unwrap();
        contract.terminate("admin-2").unwrap();

        assert!(contract.update(ContractUpdate::default(), "admin-3").is_err());
    }

    #[test]
    fn attach_revision_sets_current_revision_pointer() {
        let mut contract = Contract::new(ContractId::new("c-1"), data(), "admin-1").unwrap();
        contract.attach_revision("rev-1", "admin-2");

        assert_eq!(contract.stable.current_revision_id.as_deref(), Some("rev-1"));
        assert_eq!(contract.stable.updated_by, "admin-2");
    }

    #[test]
    fn status_machine_allows_legal_transitions() {
        let mut contract = Contract::new(ContractId::new("c-1"), data(), "admin-1").unwrap();
        contract.terminate("admin-2").unwrap();
        assert_eq!(contract.stable.status(), ContractStatus::Terminated);

        let mut expired = Contract::new(ContractId::new("c-2"), data(), "admin-1").unwrap();
        expired.expire("system").unwrap();
        assert_eq!(expired.stable.status(), ContractStatus::Expired);

        // 幂等迁移恒合法
        assert!(ensure_transition(ContractStatus::Terminated, ContractStatus::Terminated).is_ok());
    }

    #[test]
    fn status_machine_rejects_illegal_transitions() {
        let mut terminated = Contract::new(ContractId::new("c-1"), data(), "admin-1").unwrap();
        terminated.terminate("admin-2").unwrap();
        assert!(terminated.expire("system").is_err(), "终止后不可再到期");
        assert!(terminated.terminate("admin-3").is_ok(), "同态重复终止幂等通过");
        assert!(ensure_transition(ContractStatus::Terminated, ContractStatus::Effective).is_err());

        let error = ensure_transition(ContractStatus::Terminated, ContractStatus::Effective).unwrap_err();
        match error {
            Error::InvalidStateTransition { from, to } => {
                assert_eq!(from, "Terminated");
                assert_eq!(to, "Effective");
            }
            other => panic!("期望 InvalidStateTransition，得到 {other:?}"),
        }
    }

    #[test]
    fn terminal_states_have_no_outgoing_edges() {
        // 终态（TERMINATED/EXPIRED）用逐边定向断言：不允许任何出发边。
        assert!(ContractStatus::Terminated.allowed_next().is_empty());
        assert!(ContractStatus::Expired.allowed_next().is_empty());
        // 全部既有边定向成立。
        assert!(ensure_transition(ContractStatus::Effective, ContractStatus::Terminated).is_ok());
        assert!(ensure_transition(ContractStatus::Effective, ContractStatus::Expired).is_ok());
    }
}
