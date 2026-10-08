use erp_core::common::time::BusinessDate;
use id_generator::next_id;
use persistence_core::Executor;

use super::{ACTOR, BuiltinCompany, PartyService, SensitiveDataCodec};
use crate::entity::party::{
    AddressType, EffectiveRecordStatus, Party, PartyAddress, PartyAddressData, PartyAddressId,
    PartyBankAccount, PartyBankAccountContentMatch, PartyBankAccountData, PartyBankAccountId, PartyContact,
    PartyContactData, PartyContactId, PartyId, PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileId,
    SensitiveFactReuse,
};
use crate::repository::PartyExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

// 仅同步默认选择，不改写被历史单据引用的从属事实内容。
macro_rules! save_default {
    ($service:expr, $repo:ident, $desired:expr, $existing:expr, $executor:expr) => {{
        let desired = $desired;
        match $existing {
            Some(mut existing) => {
                if existing.is_default {
                    false
                } else {
                    $service
                        .db
                        .$repo()
                        .clear_other_default_marks(&existing.party_id, Some(&existing.base.id), $executor)
                        .await?;
                    existing.is_default = true;
                    existing.updated_by = ACTOR.into();
                    $service.db.$repo().update(&mut existing, $executor).await?;
                    true
                }
            },
            None => {
                $service.db.$repo().clear_other_default_marks(&desired.party_id, None, $executor).await?;
                $service.db.$repo().create(&desired, $executor).await?;
                true
            },
        }
    }};
}

impl PartyService {
    pub(super) async fn sync_company_facts(
        &self,
        seed: &BuiltinCompany,
        party: &Party,
        codec: &SensitiveDataCodec,
        today: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let party_id = PartyId::new(&party.base.id);
        let (contacts, addresses, taxes, _) =
            self.db.party().load_current_facts(&party_id, today, executor).await?;
        let contact = seed.contact(&party_id, codec, today)?;
        let existing_contact = contacts.into_iter().find(|c| {
            c.mobile_query_hmac == contact.mobile_query_hmac && c.contact_name == contact.contact_name
        });
        let contact_changed = save_default!(self, party_contacts, contact, existing_contact, executor);
        let address = seed.address(&party_id, codec, today)?;
        let existing_address = addresses.into_iter().find(|a| {
            a.address_type == AddressType::Registered && a.address_query_hmac == address.address_query_hmac
        });
        let address_changed = save_default!(self, party_addresses, address, existing_address, executor);
        let tax = seed.tax(&party_id, today)?;
        let existing_tax = taxes.into_iter().find(|t| t.tax_no == tax.tax_no);
        let tax_changed = save_default!(self, party_tax_profiles, tax, existing_tax, executor);
        let bank_changed = self.sync_company_bank(seed, &party_id, codec, today, executor).await?;
        Ok(contact_changed || address_changed || tax_changed || bank_changed)
    }

    async fn sync_company_bank(
        &self,
        seed: &BuiltinCompany,
        party_id: &PartyId,
        codec: &SensitiveDataCodec,
        today: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(desired) = seed.bank(party_id, codec, today)? else {
            return Ok(false);
        };
        let accounts = self
            .db
            .party_bank_accounts()
            .find_many_by_field_including_deleted("party_id", party_id.to_string(), executor)
            .await?;
        let existing =
            accounts.into_iter().find(|a| a.account_number_query_hmac == desired.account_number_query_hmac);
        if let Some(account) = &existing {
            if account.base.deleted_at != 0
                || account.status != EffectiveRecordStatus::Active
                || account.valid_from > today
                || account.valid_to.is_some_and(|end| end <= today)
            {
                return Err(Error::ConflictError(format!(
                    "内建公司 {} 的银行账户已删除、停用或不在有效期",
                    seed.legal_name
                )));
            }
            account.ensure_unmodified(&PartyBankAccountContentMatch::new(
                &desired.account_name,
                &desired.bank_name,
                desired.bank_branch_name.clone(),
                SensitiveFactReuse::reuse_original(),
            ))?;
        }
        Ok(save_default!(self, party_bank_accounts, desired, existing, executor))
    }
}

impl BuiltinCompany {
    fn contact(
        &self,
        party_id: &PartyId,
        codec: &SensitiveDataCodec,
        today: BusinessDate,
    ) -> Result<PartyContact> {
        let mut contact = PartyContact::new(
            PartyContactId::new(next_id()),
            PartyContactData {
                party_id: party_id.clone(),
                contact_name: "公司电话".into(),
                title: None,
                mobile: self.telephone.clone(),
                telephone: None,
                email: None,
                valid_from: today,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            codec.fingerprint_key(),
            ACTOR,
        )?;
        contact.mobile_ciphertext = codec.encrypt(&self.telephone)?;
        Ok(contact)
    }

    fn address(
        &self,
        party_id: &PartyId,
        codec: &SensitiveDataCodec,
        today: BusinessDate,
    ) -> Result<PartyAddress> {
        let mut address = PartyAddress::new(
            PartyAddressId::new(next_id()),
            PartyAddressData {
                party_id: party_id.clone(),
                address_type: AddressType::Registered,
                contact_name: None,
                address: self.address.clone(),
                valid_from: today,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            codec.fingerprint_key(),
            ACTOR,
        )?;
        address.address_ciphertext = codec.encrypt(&self.address)?;
        Ok(address)
    }

    fn tax(&self, party_id: &PartyId, today: BusinessDate) -> Result<PartyTaxProfile> {
        Ok(PartyTaxProfile::new(
            PartyTaxProfileId::new(next_id()),
            PartyTaxProfileData {
                party_id: party_id.clone(),
                tax_no: self.credit_code.clone(),
                valid_from: today,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            ACTOR,
        )?)
    }

    fn bank(
        &self,
        party_id: &PartyId,
        codec: &SensitiveDataCodec,
        today: BusinessDate,
    ) -> Result<Option<PartyBankAccount>> {
        let Some(number) = &self.bank_account else {
            return Ok(None);
        };
        let mut bank = PartyBankAccount::new(
            PartyBankAccountId::new(next_id()),
            PartyBankAccountData {
                bank_account_no: format!("COMPANY-BANK-{}", next_id()),
                party_id: party_id.clone(),
                account_name: self.legal_name.clone(),
                bank_name: self.bank_name.clone(),
                bank_branch_name: None,
                account_number: number.clone(),
                valid_from: today,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            codec.fingerprint_key(),
            ACTOR,
        )?;
        bank.account_number_ciphertext = codec.encrypt(number)?;
        Ok(Some(bank))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seed_facts_validate_and_sensitive_values_round_trip() {
        let codec = SensitiveDataCodec::from_secret(b"unit-test-only");
        let id = PartyId::new("existing");
        let today = BusinessDate::today();
        for seed in BuiltinCompany::all().unwrap() {
            let address = seed.address(&id, &codec, today).unwrap();
            assert_eq!(codec.decrypt(&address.address_ciphertext).unwrap(), seed.address);
            let contact = seed.contact(&id, &codec, today).unwrap();
            assert_eq!(codec.decrypt(&contact.mobile_ciphertext).unwrap(), seed.telephone);
            assert_eq!(seed.tax(&id, today).unwrap().tax_no, seed.credit_code);
            let bank = seed.bank(&id, &codec, today).unwrap();
            match seed.bank_account {
                Some(number) => {
                    let bank = bank.unwrap();
                    assert_eq!(codec.decrypt(&bank.account_number_ciphertext).unwrap(), number);
                    assert!(!serde_json::to_string(&bank).unwrap().contains(&number));
                },
                None => assert!(bank.is_none()),
            }
        }
    }
}
