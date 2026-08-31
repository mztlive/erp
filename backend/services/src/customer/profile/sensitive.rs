//! 客户资料敏感字段令牌、归属校验与解密。

use database::{AccessControlExt, NoTransaction, PartyExt};
use entities::{
    common::time::Instant,
    ids::PartyId,
    party::{PartyAddress, PartyBankAccount, PartyContact, PartyOwned},
};
use validator::Validate;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    party::SensitiveFieldKind,
};

use super::super::{CustomerSensitiveFieldView, CustomerSensitiveRevealView, RevealCustomerSensitiveRequest};
use super::CustomerProfileService;

impl CustomerProfileService {
    /// 验证短时令牌和客户归属后解密单个敏感字段，并记录成功审计。
    ///
    /// HTTP 层必须先依据令牌字段类型执行对应 RBAC 校验。
    ///
    /// # Errors
    /// 令牌非法/过期、事实不属于令牌客户、密文不可用或审计失败时返回错误。
    pub async fn reveal_sensitive(
        &self,
        req: RevealCustomerSensitiveRequest,
        actor: &AuditActor,
    ) -> Result<CustomerSensitiveRevealView> {
        req.validate()?;
        let now = unix_now()?;
        let scope = self.sensitive_data.verify_reveal_token(&req.reveal_token, now)?;
        let account = self.load_customer(&scope.supplier_id).await?;
        let ciphertext = self
            .sensitive_ciphertext(scope.kind, &scope.record_id, &account.party_id)
            .await?;
        let value = self.sensitive_data.decrypt(&ciphertext)?;
        let audit =
            actor
                .clone()
                .resource_log("customer_sensitive.reveal", "customer_sensitive", scope.record_id)?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(CustomerSensitiveRevealView { value })
    }

    /// 为每条当前敏感事实签发一分钟有效的字段级令牌。
    pub(super) fn sensitive_fields(
        &self,
        customer_id: &str,
        contacts: &[PartyContact],
        addresses: &[PartyAddress],
        bank_accounts: &[PartyBankAccount],
    ) -> Result<Vec<CustomerSensitiveFieldView>> {
        let expires_at = unix_now()? + 60;
        let mut fields = Vec::with_capacity(contacts.len() + addresses.len() + bank_accounts.len());
        for contact in contacts {
            fields.push(self.sensitive_field(
                SensitiveFieldKind::ContactMobile,
                &contact.base.id,
                customer_id,
                masked_last4(&contact.mobile_last4),
                expires_at,
            )?);
        }
        for address in addresses {
            fields.push(self.sensitive_field(
                SensitiveFieldKind::Address,
                &address.base.id,
                customer_id,
                "********".to_string(),
                expires_at,
            )?);
        }
        for account in bank_accounts {
            fields.push(self.sensitive_field(
                SensitiveFieldKind::BankAccountNumber,
                &account.base.id,
                customer_id,
                masked_last4(&account.account_number_last4),
                expires_at,
            )?);
        }
        Ok(fields)
    }

    /// 签发一个受客户与事实行约束的敏感字段令牌。
    fn sensitive_field(
        &self,
        kind: SensitiveFieldKind,
        record_id: &str,
        customer_id: &str,
        masked_value: String,
        expires_at: u64,
    ) -> Result<CustomerSensitiveFieldView> {
        let reveal_token =
            self.sensitive_data
                .issue_reveal_token(kind, record_id, customer_id, expires_at)?;
        Ok(CustomerSensitiveFieldView {
            kind,
            record_id: record_id.to_string(),
            masked_value,
            reveal_token,
            expires_at,
        })
    }

    /// 读取令牌指定事实的密文并校验其 Party 归属。
    async fn sensitive_ciphertext(
        &self,
        kind: SensitiveFieldKind,
        record_id: &str,
        party_id: &PartyId,
    ) -> Result<String> {
        match kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party_contacts()
                    .find_contact(record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                record
                    .ensure_party(party_id)
                    .map_err(|error| Error::Forbidden(error.to_string()))?;
                Ok(record.mobile_ciphertext)
            }
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party_addresses()
                    .find_address(record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                record
                    .ensure_party(party_id)
                    .map_err(|error| Error::Forbidden(error.to_string()))?;
                Ok(record.address_ciphertext)
            }
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party_bank_accounts()
                    .find_bank_account(record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
                record
                    .ensure_party(party_id)
                    .map_err(|error| Error::Forbidden(error.to_string()))?;
                Ok(record.account_number_ciphertext)
            }
        }
    }
}

fn unix_now() -> Result<u64> {
    u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("系统时间非法".to_string()))
}

/// 生成不可逆末四位掩码。
fn masked_last4(last4: &str) -> String {
    if last4.is_empty() {
        "****".to_string()
    } else {
        format!("****{last4}")
    }
}
