//! Consumer snapshots and port for party facts needed by supplier queries.

use std::collections::HashMap;

use async_trait::async_trait;
use erp_core::ids::PartyId;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// 主体启停状态快照；JSON 与原 `PartyStatus` 相同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyStatusFact {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl PartyStatusFact {
    /// 主体是否启用。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 从属事实行启停状态快照；JSON 与原 `EffectiveRecordStatus` 相同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveRecordStatusFact {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl EffectiveRecordStatusFact {
    /// 事实行是否启用。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 地址类型快照；JSON 与原 `AddressType` 相同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressTypeFact {
    /// 注册地址。
    Registered,
    /// 经营地址。
    Operating,
    /// 履约地址。
    Fulfillment,
}

/// 列表/详情所需的主体稳定事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyListFact {
    /// 主体稳定 ID。
    pub id: String,
    /// 主体编号。
    pub party_no: String,
    /// 启停状态。
    pub status: PartyStatusFact,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 当前修订 ID。
    pub current_revision_id: Option<String>,
}

/// 主体当前修订的法定名称事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyRevisionFact {
    /// 修订稳定 ID。
    pub id: String,
    /// 所属主体 ID。
    pub party_id: String,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
}

/// 联系人响应事实；字段与原 `PartyContactView` JSON 对齐。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyContactFact {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 电话。
    pub telephone: Option<String>,
    /// 手机号掩码；列表与详情均不返回明文。
    pub mobile_masked: String,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatusFact,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 地址响应事实；字段与原 `PartyAddressView` JSON 对齐。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyAddressFact {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 地址类型。
    pub address_type: AddressTypeFact,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认地址。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatusFact,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 税务资料响应事实；字段与原 `PartyTaxProfileView` JSON 对齐。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyTaxProfileFact {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 纳税人识别号。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatusFact,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 银行账户响应事实；字段与原 `PartyBankAccountView` JSON 对齐。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyBankAccountFact {
    /// 实体主键。
    pub id: String,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 户名。
    pub account_name: String,
    /// 银行名称。
    pub bank_name: String,
    /// 银行账号掩码；列表与详情均不返回明文。
    pub account_number_masked: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatusFact,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 选择当前默认且启用的事实行；无默认时回落到第一条启用记录。
///
/// # Parameters
/// * `items` - 已加载事实行
/// * `is_default` - 默认标记
/// * `is_active` - 启用判定
///
/// # Returns
/// 命中的事实行；全部停用时返回 `None`。
pub fn select_current_default<T>(
    items: &[T],
    is_default: impl Fn(&T) -> bool,
    is_active: impl Fn(&T) -> bool,
) -> Option<&T> {
    items
        .iter()
        .find(|item| is_default(item) && is_active(item))
        .or_else(|| items.iter().find(|item| is_active(item)))
}

/// 供应商查询所需的主体只读事实。
///
/// 实现位于同时依赖供应商与主体的组合层；供应商仓储不得直查主体集合。
#[async_trait]
pub trait PartyFactsPort: Send + Sync {
    /// 按当前法定名称模糊匹配未删除主体 ID。
    ///
    /// # 参数
    /// * `keyword` - 已规范化的名称关键词
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 返回
    /// 返回命中的主体 ID；无命中时为空集合。
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn matching_current_party_ids_by_name(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>>;

    /// 按主体 ID 批量读取主体及其当前修订。
    ///
    /// # 参数
    /// * `party_ids` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 返回
    /// 返回主体稳定事实与当前修订事实。
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn list_with_current_revisions(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<(Vec<PartyListFact>, Vec<PartyRevisionFact>)>;

    /// 读取主体及其当前修订。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 返回
    /// 主体不存在时返回 `None`。
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn find_with_current_revision(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<(PartyListFact, Option<PartyRevisionFact>)>>;

    /// 读取主体联系人历史。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn list_contacts(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyContactFact>>;

    /// 读取主体地址历史。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn list_addresses(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyAddressFact>>;

    /// 读取主体税务资料历史。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn list_tax_profiles(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyTaxProfileFact>>;

    /// 读取主体银行账户历史。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn list_bank_accounts(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccountFact>>;

    /// 按主体 ID 批量读取当前法定名称。
    ///
    /// # 参数
    /// * `party_ids` - 主体 ID
    /// * `executor` - 调用方选择的执行器
    ///
    /// # 返回
    /// 返回主体 ID 字符串到法定名称的映射；缺少当前修订时无键。
    ///
    /// # 错误
    /// 主体查询失败时返回仓储或映射错误。
    async fn current_legal_names_by_party_ids(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;
}

/// Empty party facts used by isolated supplier unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyPartyFacts;

#[async_trait]
impl PartyFactsPort for EmptyPartyFacts {
    async fn matching_current_party_ids_by_name(
        &self,
        _keyword: &str,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>> {
        Ok(Vec::new())
    }

    async fn list_with_current_revisions(
        &self,
        _party_ids: &[PartyId],
        _executor: &mut dyn Executor,
    ) -> Result<(Vec<PartyListFact>, Vec<PartyRevisionFact>)> {
        Ok((Vec::new(), Vec::new()))
    }

    async fn find_with_current_revision(
        &self,
        _party_id: &PartyId,
        _executor: &mut dyn Executor,
    ) -> Result<Option<(PartyListFact, Option<PartyRevisionFact>)>> {
        Ok(None)
    }

    async fn list_contacts(
        &self,
        _party_id: &PartyId,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<PartyContactFact>> {
        Ok(Vec::new())
    }

    async fn list_addresses(
        &self,
        _party_id: &PartyId,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<PartyAddressFact>> {
        Ok(Vec::new())
    }

    async fn list_tax_profiles(
        &self,
        _party_id: &PartyId,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<PartyTaxProfileFact>> {
        Ok(Vec::new())
    }

    async fn list_bank_accounts(
        &self,
        _party_id: &PartyId,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccountFact>> {
        Ok(Vec::new())
    }

    async fn current_legal_names_by_party_ids(
        &self,
        _party_ids: &[PartyId],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::{select_current_default, EffectiveRecordStatusFact, PartyContactFact};

    fn contact(id: &str, is_default: bool, active: bool) -> PartyContactFact {
        PartyContactFact {
            id: id.to_string(),
            party_id: "party-1".to_string(),
            contact_name: id.to_string(),
            title: None,
            telephone: None,
            mobile_masked: "****0000".to_string(),
            email: None,
            valid_from: "2026-01-01".to_string(),
            valid_to: None,
            is_default,
            status: if active {
                EffectiveRecordStatusFact::Active
            } else {
                EffectiveRecordStatusFact::Disabled
            },
            version: 1,
            created_at: 1,
        }
    }

    #[test]
    fn select_current_default_prefers_active_default() {
        let items = vec![
            contact("c1", false, true),
            contact("c2", true, true),
            contact("c3", true, false),
        ];
        let selected =
            select_current_default(&items, |item| item.is_default, |item| item.status.is_active()).unwrap();
        assert_eq!(selected.id, "c2");
    }
}
