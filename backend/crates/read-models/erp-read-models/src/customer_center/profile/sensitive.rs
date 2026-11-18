//! 当前客户敏感字段的只读令牌投影。
use erp_core::common::time::Instant;
use erp_customer::CustomerSensitiveFieldView;
use erp_party::{PartyAddress, PartyBankAccount, PartyContact, SensitiveFieldKind};
use erp_permissions::{Permission, party, payable};

use super::CustomerProfileReadService;
use crate::{Error, Result};
impl CustomerProfileReadService {
    // 处理客户敏感字段时，为每条当前敏感事实签发一分钟有效的字段级令牌。
    //
    // 调用参数：
    // * `customer_id` - 令牌绑定的客户主键。
    // * `contacts` - 当前联系人事实。
    // * `addresses` - 当前地址事实。
    // * `bank_accounts` - 当前银行账户事实。
    //
    // 按联系人、地址、银行账户的顺序返回令牌视图；手机号和账号使用末四位掩码，地址固定掩码。
    //
    // 系统时间无法转换为秒，或令牌签发失败时返回错误。
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

    // 处理客户敏感字段时，签发一个受客户与事实行约束的敏感字段令牌。
    fn sensitive_field(
        &self,
        kind: SensitiveFieldKind,
        record_id: &str,
        customer_id: &str,
        masked_value: String,
        expires_at: u64,
    ) -> Result<CustomerSensitiveFieldView> {
        let reveal_token =
            self.sensitive_data.issue_reveal_token(kind, record_id, customer_id, expires_at)?;
        Ok(CustomerSensitiveFieldView {
            kind: super::views::customer_sensitive_kind(kind),
            record_id: record_id.to_string(),
            masked_value,
            reveal_token,
            expires_at,
        })
    }
}
// 处理客户敏感字段时，读取当前时刻的 Unix 秒；负值或溢出不能签发令牌。
fn unix_now() -> Result<u64> {
    u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("系统时间非法".to_string()))
}

// 处理客户敏感字段时，生成不可逆末四位掩码。
fn masked_last4(last4: &str) -> String {
    if last4.is_empty() { "****".to_string() } else { format!("****{last4}") }
}

// 处理客户敏感字段时，每类字段保留独立的详情与揭示资格。
pub(super) fn field_permissions(kind: SensitiveFieldKind) -> [Permission; 2] {
    match kind {
        SensitiveFieldKind::ContactMobile => [party::CONTACT_DETAIL, party::CONTACT_REVEAL],
        SensitiveFieldKind::Address => [party::ADDRESS_DETAIL, party::ADDRESS_REVEAL],
        SensitiveFieldKind::BankAccountNumber => {
            [party::BANK_ACCOUNT_DETAIL, payable::PAYMENT_RECIPIENT_REVEAL]
        },
    }
}
