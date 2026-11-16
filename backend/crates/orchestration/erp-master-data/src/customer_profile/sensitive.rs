//! 客户资料敏感字段令牌、归属校验与解密。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::PartyId;
use erp_customer::{CustomerSensitiveRevealView, RevealCustomerSensitiveRequest};
use erp_identity::AccessTarget;
use erp_party::repository::prelude::*;
use erp_party::{PartyExt, PartyOwned, SensitiveFieldKind};
use erp_permissions::{Permission, customer, party, payable};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CustomerProfileProcess;
use crate::{Error, Result};

impl CustomerProfileProcess {
    /// 验证短时令牌和客户归属后解密单个敏感字段，并记录成功审计。
    ///
    /// 本入口在同一事务检查客户对象、字段详情和揭示资格。
    ///
    /// # 参数
    /// * `req` - 揭示请求，只携带短时令牌。
    /// * `actor` - 已认证操作人，用于成功审计。
    ///
    /// # 返回
    /// 返回解密后的单个敏感字段明文。
    ///
    /// # 错误
    /// 请求校验失败、令牌非法或过期、客户或事实不存在、事实不属于令牌客户、密文不可用或审计写入失败时返回错误。
    pub async fn reveal_sensitive(
        &self,
        req: RevealCustomerSensitiveRequest,
        actor: &AuditActor,
    ) -> Result<CustomerSensitiveRevealView> {
        req.validate()?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { this.reveal_snapshot(&req, &actor, executor).await })
            })
            .await
    }

    /// 字段资格、客户归属、密文读取及审计共用原事务。
    async fn reveal_snapshot(
        &self,
        req: &RevealCustomerSensitiveRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerSensitiveRevealView> {
        let scope = self.sensitive_data.verify_reveal_token(&req.reveal_token, unix_now()?)?;
        self.authorization
            .require(actor, &customer::SENSITIVE_REVEAL, AccessTarget::Operation, executor)
            .await?;
        for permission in field_permissions(scope.kind) {
            self.authorization.require(actor, &permission, AccessTarget::Operation, executor).await?;
        }
        self.access.require_with(actor.clone(), &customer::DETAIL, &scope.supplier_id, executor).await?;
        let account = self.load_customer(&scope.supplier_id, executor).await?;
        let ciphertext =
            self.sensitive_ciphertext(scope.kind, &scope.record_id, &account.party_id, executor).await?;
        let value = self.sensitive_data.decrypt(&ciphertext)?;
        let audit =
            actor.clone().resource_log("customer_sensitive.reveal", "customer_sensitive", scope.record_id)?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(CustomerSensitiveRevealView { value })
    }

    /// 读取令牌指定事实的密文并校验其 Party 归属。
    async fn sensitive_ciphertext(
        &self,
        kind: SensitiveFieldKind,
        record_id: &str,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        match kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party_contacts()
                    .find_contact(record_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                record.ensure_party(party_id).map_err(|error| Error::Forbidden(error.to_string()))?;
                Ok(record.mobile_ciphertext)
            },
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party_addresses()
                    .find_address(record_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                record.ensure_party(party_id).map_err(|error| Error::Forbidden(error.to_string()))?;
                Ok(record.address_ciphertext)
            },
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party_bank_accounts()
                    .find_bank_account(record_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
                record.ensure_party(party_id).map_err(|error| Error::Forbidden(error.to_string()))?;
                Ok(record.account_number_ciphertext)
            },
        }
    }
}

/// 读取当前时刻的 Unix 秒；负值或溢出不能签发令牌。
fn unix_now() -> Result<u64> {
    u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("系统时间非法".to_string()))
}

/// 每类字段保留独立的详情与揭示资格。
pub(super) fn field_permissions(kind: SensitiveFieldKind) -> [Permission; 2] {
    match kind {
        SensitiveFieldKind::ContactMobile => [party::CONTACT_DETAIL, party::CONTACT_REVEAL],
        SensitiveFieldKind::Address => [party::ADDRESS_DETAIL, party::ADDRESS_REVEAL],
        SensitiveFieldKind::BankAccountNumber => {
            [party::BANK_ACCOUNT_DETAIL, payable::PAYMENT_RECIPIENT_REVEAL]
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_customer::CustomerAccess;
    use erp_identity::testing::unavailable_authorization;
    use erp_party::SensitiveDataCodec;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn valid_reveal_token_still_requires_current_authorization_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("reveal_guard");
        let authorization = unavailable_authorization();
        let codec = Arc::new(SensitiveDataCodec::from_secret(b"customer-reveal-guard"));
        let token = codec
            .issue_reveal_token(
                SensitiveFieldKind::ContactMobile,
                "contact",
                "customer",
                unix_now().unwrap() + 60,
            )
            .unwrap();
        let access = CustomerAccess::new(db.clone(), authorization.clone());
        let process = CustomerProfileProcess::new(db, codec, access, authorization);
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = process
            .reveal_snapshot(
                &RevealCustomerSensitiveRequest { reveal_token: token },
                &actor,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn bank_reveal_rejects_non_snapshot_before_identity_or_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("bank_reveal_guard");
        let authorization = unavailable_authorization();
        let codec = Arc::new(SensitiveDataCodec::from_secret(b"customer-bank-reveal-guard"));
        let token = codec
            .issue_reveal_token(
                SensitiveFieldKind::BankAccountNumber,
                "bank-account",
                "customer",
                unix_now().unwrap() + 60,
            )
            .unwrap();
        let access = CustomerAccess::new(db.clone(), authorization.clone());
        let process = CustomerProfileProcess::new(db, codec, access, authorization);
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let result = process
            .reveal_snapshot(
                &RevealCustomerSensitiveRequest { reveal_token: token },
                &actor,
                &mut NoTransaction,
            )
            .await;
        assert!(
            matches!(result, Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务")
        );
    }
}
