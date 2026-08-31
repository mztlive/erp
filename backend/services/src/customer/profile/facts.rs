//! 客户联系人、地址与银行账户事实的构造、差异和持久化。

use std::collections::HashMap;

use database::{NoTransaction, PartyExt};
use entities::{
    common::time::BusinessDate,
    field_update::FieldUpdate,
    ids::{PartyAddressId, PartyBankAccountId, PartyContactId, PartyId},
    party::{
        EffectiveRecordStatus, PartyAddress, PartyAddressData, PartyAddressUpdate, PartyBankAccount,
        PartyBankAccountData, PartyBankAccountUpdate, PartyContact, PartyContactData, PartyContactUpdate,
    },
};
use id_generator::next_id;
use mongodb::Database;

use crate::errors::{Error, Result};

use super::super::{
    CustomerProfileAddressInput, CustomerProfileBankAccountInput, CustomerProfileContactInput,
    SaveCustomerProfileRequest,
};
use super::{numbering::business_no, CustomerProfileService};

impl CustomerProfileService {
    /// 构造创建场景的全部从属事实并完成敏感值加密。
    pub(super) fn create_facts(
        &self,
        req: &SaveCustomerProfileRequest,
        party_id: &PartyId,
        actor: &str,
    ) -> Result<PartyFacts> {
        let contacts = req
            .contacts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|input| self.new_contact(input, party_id, req.effective_from, actor))
            .collect::<Result<Vec<_>>>()?;
        let addresses = req
            .addresses
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|input| self.new_address(input, party_id, req.effective_from, actor))
            .collect::<Result<Vec<_>>>()?;
        let bank_accounts = req
            .bank_accounts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|input| self.new_bank_account(input, party_id, req.effective_from, actor))
            .collect::<Result<Vec<_>>>()?;
        Ok(PartyFacts {
            contacts,
            addresses,
            bank_accounts,
        })
    }

    /// 为显式提交的事实集合计算保留、结束和新增差异。
    pub(super) async fn prepare_fact_changes(
        &self,
        party_id: &PartyId,
        req: &SaveCustomerProfileRequest,
        actor: &str,
    ) -> Result<PartyFactChanges> {
        let mut changes = PartyFactChanges::default();
        if let Some(inputs) = &req.contacts {
            let existing = self
                .db
                .party_contacts()
                .list_active_on(party_id, req.effective_from, &mut NoTransaction)
                .await?;
            changes.contacts = self.diff_contacts(existing, inputs, party_id, req.effective_from, actor)?;
        }
        if let Some(inputs) = &req.addresses {
            let existing = self
                .db
                .party_addresses()
                .list_active_on(party_id, req.effective_from, &mut NoTransaction)
                .await?;
            changes.addresses = self.diff_addresses(existing, inputs, party_id, req.effective_from, actor)?;
        }
        if let Some(inputs) = &req.bank_accounts {
            let existing = self
                .db
                .party_bank_accounts()
                .list_active_on(party_id, req.effective_from, &mut NoTransaction)
                .await?;
            changes.bank_accounts =
                self.diff_bank_accounts(existing, inputs, party_id, req.effective_from, actor)?;
        }
        Ok(changes)
    }

    /// 构造并加密联系人事实。
    fn new_contact(
        &self,
        input: &CustomerProfileContactInput,
        party_id: &PartyId,
        valid_from: BusinessDate,
        actor: &str,
    ) -> Result<PartyContact> {
        let mobile = required_text(input.mobile.as_deref(), "手机号")?;
        let mut contact = PartyContact::new(
            PartyContactId::new(next_id()),
            PartyContactData {
                party_id: party_id.clone(),
                contact_name: input.contact_name.clone(),
                title: input.title.clone(),
                mobile: mobile.clone(),
                telephone: input.telephone.clone(),
                email: input.email.clone(),
                valid_from,
                valid_to: None,
                is_default: input.is_default,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor,
        )?;
        contact.mobile_ciphertext = self.sensitive_data.encrypt(&mobile)?;
        Ok(contact)
    }

    /// 构造并加密地址事实。
    fn new_address(
        &self,
        input: &CustomerProfileAddressInput,
        party_id: &PartyId,
        valid_from: BusinessDate,
        actor: &str,
    ) -> Result<PartyAddress> {
        let address = required_text(input.address.as_deref(), "地址")?;
        let mut entity = PartyAddress::new(
            PartyAddressId::new(next_id()),
            PartyAddressData {
                party_id: party_id.clone(),
                address_type: input.address_type,
                contact_name: input.contact_name.clone(),
                address: address.clone(),
                valid_from,
                valid_to: None,
                is_default: input.is_default,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor,
        )?;
        entity.address_ciphertext = self.sensitive_data.encrypt(&address)?;
        Ok(entity)
    }

    /// 构造并加密银行账户事实；内部账户编号由服务端生成。
    fn new_bank_account(
        &self,
        input: &CustomerProfileBankAccountInput,
        party_id: &PartyId,
        valid_from: BusinessDate,
        actor: &str,
    ) -> Result<PartyBankAccount> {
        let account_number = required_text(input.account_number.as_deref(), "银行账号")?;
        let mut entity = PartyBankAccount::new(
            PartyBankAccountId::new(next_id()),
            PartyBankAccountData {
                bank_account_no: business_no("BA"),
                party_id: party_id.clone(),
                account_name: input.account_name.clone(),
                bank_name: input.bank_name.clone(),
                bank_branch_name: input.bank_branch_name.clone(),
                account_number: account_number.clone(),
                valid_from,
                valid_to: None,
                is_default: input.is_default,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor,
        )?;
        entity.account_number_ciphertext = self.sensitive_data.encrypt(&account_number)?;
        Ok(entity)
    }

    /// 计算联系人集合差异；既有行未携带明文且元数据未变化时原样保留。
    fn diff_contacts(
        &self,
        existing: Vec<PartyContact>,
        inputs: &[CustomerProfileContactInput],
        party_id: &PartyId,
        effective_from: BusinessDate,
        actor: &str,
    ) -> Result<EntityChanges<PartyContact>> {
        let mut current = by_id(existing, |item| item.base.id.clone());
        let mut changes = EntityChanges::default();
        for input in inputs {
            let Some(existing_id) = input.existing_id.as_deref() else {
                changes
                    .created
                    .push(self.new_contact(input, party_id, effective_from, actor)?);
                continue;
            };
            let mut entity = take_existing(&mut current, existing_id, "联系人")?;
            if contact_matches(&entity, input, self.sensitive_data.fingerprint_key()) {
                update_contact_default(&mut entity, input.is_default, actor, &mut changes.updated)?;
                continue;
            }
            let mut replacement = input.clone();
            if replacement
                .mobile
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                replacement.mobile = Some(self.sensitive_data.decrypt(&entity.mobile_ciphertext)?);
            }
            close_contact(&mut entity, effective_from, actor)?;
            changes.updated.push(entity);
            changes
                .created
                .push(self.new_contact(&replacement, party_id, effective_from, actor)?);
        }
        close_remaining_contacts(current, effective_from, actor, &mut changes.updated)?;
        Ok(changes)
    }

    /// 计算地址集合差异；既有行未携带明文且元数据未变化时原样保留。
    fn diff_addresses(
        &self,
        existing: Vec<PartyAddress>,
        inputs: &[CustomerProfileAddressInput],
        party_id: &PartyId,
        effective_from: BusinessDate,
        actor: &str,
    ) -> Result<EntityChanges<PartyAddress>> {
        let mut current = by_id(existing, |item| item.base.id.clone());
        let mut changes = EntityChanges::default();
        for input in inputs {
            let Some(existing_id) = input.existing_id.as_deref() else {
                changes
                    .created
                    .push(self.new_address(input, party_id, effective_from, actor)?);
                continue;
            };
            let mut entity = take_existing(&mut current, existing_id, "地址")?;
            if address_matches(&entity, input, self.sensitive_data.fingerprint_key()) {
                update_address_default(&mut entity, input.is_default, actor, &mut changes.updated)?;
                continue;
            }
            let mut replacement = input.clone();
            if replacement
                .address
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                replacement.address = Some(self.sensitive_data.decrypt(&entity.address_ciphertext)?);
            }
            close_address(&mut entity, effective_from, actor)?;
            changes.updated.push(entity);
            changes
                .created
                .push(self.new_address(&replacement, party_id, effective_from, actor)?);
        }
        close_remaining_addresses(current, effective_from, actor, &mut changes.updated)?;
        Ok(changes)
    }

    /// 计算银行账户集合差异；既有稳定账户只允许调整默认标记或结束。
    fn diff_bank_accounts(
        &self,
        existing: Vec<PartyBankAccount>,
        inputs: &[CustomerProfileBankAccountInput],
        party_id: &PartyId,
        effective_from: BusinessDate,
        actor: &str,
    ) -> Result<EntityChanges<PartyBankAccount>> {
        let mut current = by_id(existing, |item| item.base.id.clone());
        let mut changes = EntityChanges::default();
        for input in inputs {
            let Some(existing_id) = input.existing_id.as_deref() else {
                changes
                    .created
                    .push(self.new_bank_account(input, party_id, effective_from, actor)?);
                continue;
            };
            let mut entity = take_existing(&mut current, existing_id, "银行账户")?;
            if !bank_account_matches(&entity, input, self.sensitive_data.fingerprint_key()) {
                return Err(Error::ValidationError(
                    "既有银行账户内容不可原地修改，请结束旧账户后新增账户".to_string(),
                ));
            }
            update_bank_default(&mut entity, input.is_default, actor, &mut changes.updated)?;
        }
        close_remaining_banks(current, effective_from, actor, &mut changes.updated)?;
        Ok(changes)
    }
}

/// 创建场景的从属事实。
#[derive(Default)]
pub(super) struct PartyFacts {
    contacts: Vec<PartyContact>,
    addresses: Vec<PartyAddress>,
    bank_accounts: Vec<PartyBankAccount>,
}

impl PartyFacts {
    /// 写入全部新事实。
    pub(super) async fn persist(self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for item in &self.contacts {
            db.party_contacts().create(item, session).await?;
        }
        for item in &self.addresses {
            db.party_addresses().create(item, session).await?;
        }
        for item in &self.bank_accounts {
            db.party_bank_accounts().create(item, session).await?;
        }
        Ok(())
    }
}

/// 一类有效期事实的更新与新增差异。
struct EntityChanges<T> {
    updated: Vec<T>,
    created: Vec<T>,
}

impl<T> Default for EntityChanges<T> {
    fn default() -> Self {
        Self {
            updated: Vec::new(),
            created: Vec::new(),
        }
    }
}

/// 修订场景的 Party 事实差异。
#[derive(Default)]
pub(super) struct PartyFactChanges {
    contacts: EntityChanges<PartyContact>,
    addresses: EntityChanges<PartyAddress>,
    bank_accounts: EntityChanges<PartyBankAccount>,
}

impl PartyFactChanges {
    /// 按先结束旧事实、后写新事实的顺序持久化差异。
    pub(super) async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for item in &mut self.contacts.updated {
            db.party_contacts().update(item, session).await?;
        }
        for item in &self.contacts.created {
            db.party_contacts().create(item, session).await?;
        }
        for item in &mut self.addresses.updated {
            db.party_addresses().update(item, session).await?;
        }
        for item in &self.addresses.created {
            db.party_addresses().create(item, session).await?;
        }
        for item in &mut self.bank_accounts.updated {
            db.party_bank_accounts().update(item, session).await?;
        }
        for item in &self.bank_accounts.created {
            db.party_bank_accounts().create(item, session).await?;
        }
        Ok(())
    }
}

/// 把实体集合转成按 ID 索引的当前集合。
fn by_id<T>(items: Vec<T>, id: impl Fn(&T) -> String) -> HashMap<String, T> {
    items.into_iter().map(|item| (id(&item), item)).collect()
}

/// 从当前集合取出客户端引用的既有事实。
fn take_existing<T>(current: &mut HashMap<String, T>, id: &str, label: &str) -> Result<T> {
    current
        .remove(id)
        .ok_or_else(|| Error::ConflictError(format!("{label}已变化，请刷新后重试")))
}

/// 判断联系人输入是否与当前事实内容一致。
fn contact_matches(contact: &PartyContact, input: &CustomerProfileContactInput, key: &[u8]) -> bool {
    let mobile_matches = input.mobile.as_deref().is_none_or(|mobile| {
        mobile.trim().is_empty() || PartyContact::mobile_fingerprint(mobile, key) == contact.mobile_query_hmac
    });
    mobile_matches
        && contact.contact_name == input.contact_name.trim()
        && normalized_optional(&contact.title) == normalized_optional(&input.title)
        && normalized_optional(&contact.telephone) == normalized_optional(&input.telephone)
        && normalized_optional(&contact.email) == normalized_optional(&input.email)
}

/// 判断地址输入是否与当前事实内容一致。
fn address_matches(address: &PartyAddress, input: &CustomerProfileAddressInput, key: &[u8]) -> bool {
    let content_matches = input.address.as_deref().is_none_or(|value| {
        value.trim().is_empty() || PartyAddress::address_fingerprint(value, key) == address.address_query_hmac
    });
    content_matches
        && address.address_type == input.address_type
        && normalized_optional(&address.contact_name) == normalized_optional(&input.contact_name)
}

/// 判断既有银行账户稳定内容是否未被修改。
fn bank_account_matches(
    account: &PartyBankAccount,
    input: &CustomerProfileBankAccountInput,
    key: &[u8],
) -> bool {
    let number_matches = input.account_number.as_deref().is_none_or(|value| {
        value.trim().is_empty()
            || PartyBankAccount::account_number_fingerprint(value, key) == account.account_number_query_hmac
    });
    number_matches
        && account.account_name == input.account_name.trim()
        && account.bank_name == input.bank_name.trim()
        && normalized_optional(&account.bank_branch_name) == normalized_optional(&input.bank_branch_name)
}

/// 仅在默认标记变化时更新联系人事实。
fn update_contact_default(
    contact: &mut PartyContact,
    is_default: bool,
    actor: &str,
    updated: &mut Vec<PartyContact>,
) -> Result<()> {
    if contact.is_default == is_default {
        return Ok(());
    }
    contact.update(
        PartyContactUpdate {
            is_default: Some(is_default),
            ..Default::default()
        },
        actor,
    )?;
    updated.push(contact.clone());
    Ok(())
}

/// 仅在默认标记变化时更新地址事实。
fn update_address_default(
    address: &mut PartyAddress,
    is_default: bool,
    actor: &str,
    updated: &mut Vec<PartyAddress>,
) -> Result<()> {
    if address.is_default == is_default {
        return Ok(());
    }
    address.update(
        PartyAddressUpdate {
            is_default: Some(is_default),
            ..Default::default()
        },
        actor,
    )?;
    updated.push(address.clone());
    Ok(())
}

/// 仅在默认标记变化时更新银行账户事实。
fn update_bank_default(
    account: &mut PartyBankAccount,
    is_default: bool,
    actor: &str,
    updated: &mut Vec<PartyBankAccount>,
) -> Result<()> {
    if account.is_default == is_default {
        return Ok(());
    }
    account.update(
        PartyBankAccountUpdate {
            is_default: Some(is_default),
            ..Default::default()
        },
        actor,
    )?;
    updated.push(account.clone());
    Ok(())
}

/// 结束联系人当前事实。
fn close_contact(contact: &mut PartyContact, effective_from: BusinessDate, actor: &str) -> Result<()> {
    Ok(contact.update(
        PartyContactUpdate {
            status: Some(EffectiveRecordStatus::Disabled),
            valid_to: close_date(contact.valid_from, effective_from),
            is_default: Some(false),
        },
        actor,
    )?)
}

/// 结束地址当前事实。
fn close_address(address: &mut PartyAddress, effective_from: BusinessDate, actor: &str) -> Result<()> {
    Ok(address.update(
        PartyAddressUpdate {
            status: Some(EffectiveRecordStatus::Disabled),
            valid_to: close_date(address.valid_from, effective_from),
            is_default: Some(false),
        },
        actor,
    )?)
}

/// 结束银行账户当前事实。
fn close_bank(account: &mut PartyBankAccount, effective_from: BusinessDate, actor: &str) -> Result<()> {
    Ok(account.update(
        PartyBankAccountUpdate {
            status: Some(EffectiveRecordStatus::Disabled),
            valid_to: close_date(account.valid_from, effective_from),
            is_default: Some(false),
        },
        actor,
    )?)
}

/// 结束未在目标集合中保留的联系人。
fn close_remaining_contacts(
    current: HashMap<String, PartyContact>,
    effective_from: BusinessDate,
    actor: &str,
    updated: &mut Vec<PartyContact>,
) -> Result<()> {
    for mut item in current.into_values() {
        close_contact(&mut item, effective_from, actor)?;
        updated.push(item);
    }
    Ok(())
}

/// 结束未在目标集合中保留的地址。
fn close_remaining_addresses(
    current: HashMap<String, PartyAddress>,
    effective_from: BusinessDate,
    actor: &str,
    updated: &mut Vec<PartyAddress>,
) -> Result<()> {
    for mut item in current.into_values() {
        close_address(&mut item, effective_from, actor)?;
        updated.push(item);
    }
    Ok(())
}

/// 结束未在目标集合中保留的银行账户。
fn close_remaining_banks(
    current: HashMap<String, PartyBankAccount>,
    effective_from: BusinessDate,
    actor: &str,
    updated: &mut Vec<PartyBankAccount>,
) -> Result<()> {
    for mut item in current.into_values() {
        close_bank(&mut item, effective_from, actor)?;
        updated.push(item);
    }
    Ok(())
}

/// 只有新生效日晚于旧事实开始日时才写结束日期；同日创建后修订仅停用。
fn close_date(valid_from: BusinessDate, effective_from: BusinessDate) -> FieldUpdate<BusinessDate> {
    if effective_from > valid_from {
        FieldUpdate::Set(effective_from)
    } else {
        FieldUpdate::Unchanged
    }
}

fn required_text(value: Option<&str>, label: &str) -> Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| Error::ValidationError(format!("{label}不能为空")))
}

/// 规范化可选文本供内容比较。
fn normalized_optional(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty())
}
