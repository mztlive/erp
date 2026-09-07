//! 供应商资料敏感字段令牌校验与解密。

use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierAccountId;
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use persistence_core::NoTransaction;
use validator::Validate;

use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_party::SensitiveFieldKind;

use super::{validation::ensure_sensitive_party, SupplierProfileService};
use erp_supplier::{RevealSupplierSensitiveRequest, SupplierSensitiveRevealView};

impl SupplierProfileService {
    /// 验证短时令牌、归属与权限入口后解密单个敏感字段并记录审计。
    ///
    /// # Errors
    /// 令牌非法/过期、记录不属于令牌供应商、旧数据无密文或审计写入失败时返回错误。
    pub async fn reveal_sensitive(
        &self,
        req: RevealSupplierSensitiveRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSensitiveRevealView> {
        req.validate()?;
        let now = u64::try_from(Instant::now().unix_secs())
            .map_err(|_| Error::Internal("系统时间非法".to_string()))?;
        let scope = self.sensitive_data.verify_reveal_token(&req.reveal_token, now)?;
        let supplier = self
            .db
            .supplier()
            .account(&SupplierAccountId::new(&scope.supplier_id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let ciphertext = match scope.kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party()
                    .contact(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.mobile_ciphertext
            }
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party()
                    .address(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.address_ciphertext
            }
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party()
                    .bank_account(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.account_number_ciphertext
            }
        };
        let value = self.sensitive_data.decrypt(&ciphertext)?;
        let audit =
            actor
                .clone()
                .resource_log("supplier_sensitive.reveal", "supplier_sensitive", scope.record_id)?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(SupplierSensitiveRevealView { value })
    }
}
