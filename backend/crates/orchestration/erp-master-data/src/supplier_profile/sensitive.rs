//! 供应商资料敏感字段令牌校验与解密。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::{PartyId, SupplierAccountId};
use erp_identity::AccessTarget;
use erp_party::{PartyExt, SensitiveFieldKind, SensitiveRevealScope};
use erp_permissions::supplier;
use erp_supplier::{
    RevealSupplierSensitiveRequest, SupplierAccount, SupplierExt, SupplierSensitiveRevealView,
};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierProfileProcess;
use super::validation::ensure_sensitive_party;
use crate::{Error, Result};

impl SupplierProfileProcess {
    /// 验证短时令牌、字段资格与供应商详情范围后解密单个敏感字段并记录审计。
    ///
    /// `req`：含短时揭示令牌的请求；`actor`：当前操作人，写入揭示审计。
    ///
    /// 解密后的单个敏感字段值。
    ///
    /// 请求非法、字段或供应商详情无权、令牌非法或过期、归属不符、记录缺失、解密或审计失败时返回错误。
    pub async fn reveal_sensitive(
        &self,
        req: RevealSupplierSensitiveRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSensitiveRevealView> {
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

    // 在原事务内核验揭示资格、真实主体归属并记录审计。
    async fn reveal_snapshot(
        &self,
        req: &RevealSupplierSensitiveRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSensitiveRevealView> {
        self.authorization
            .rbac
            .require(actor, &supplier::SENSITIVE_REVEAL, AccessTarget::Operation, executor)
            .await?;

        let now = u64::try_from(Instant::now().unix_secs())
            .map_err(|_| Error::Internal("系统时间非法".to_string()))?;
        let scope = self.sensitive_data.verify_reveal_token(&req.reveal_token, now)?;

        let supplier = self.reveal_supplier(&scope.supplier_id, actor, executor).await?;
        let ciphertext = self.reveal_ciphertext(&scope, &supplier.party_id, executor).await?;
        let value = self.sensitive_data.decrypt(&ciphertext)?;

        let audit =
            actor.clone().resource_log("supplier_sensitive.reveal", "supplier_sensitive", scope.record_id)?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(SupplierSensitiveRevealView { value })
    }

    // 在读取供应商与敏感事实前核对令牌供应商的当前详情范围。
    async fn reveal_supplier(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierAccount> {
        self.authorization.access.require_with(actor, &supplier::DETAIL, supplier_id, executor).await?;
        self.db
            .supplier()
            .account(&SupplierAccountId::new(supplier_id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))
    }

    // 读取与已授权供应商主体一致的单个敏感事实密文。
    async fn reveal_ciphertext(
        &self,
        scope: &SensitiveRevealScope,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let ciphertext = match scope.kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party()
                    .contact(&scope.record_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, party_id)?;
                record.mobile_ciphertext
            },
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party()
                    .address(&scope.record_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, party_id)?;
                record.address_ciphertext
            },
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party()
                    .bank_account(&scope.record_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, party_id)?;
                record.account_number_ciphertext
            },
        };
        Ok(ciphertext)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_party::SensitiveDataCodec;
    use erp_supplier::SupplierAccess;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn reveal_rejects_missing_current_permission_before_supplier_and_ciphertext_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("reveal_guard");
        let authorization = unavailable_authorization();
        let access = SupplierAccess::new(db.clone(), authorization.clone());
        let process = SupplierProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"supplier-reveal-guard")),
            authorization,
            access,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error = process
            .reveal_snapshot(
                &RevealSupplierSensitiveRequest { reveal_token: "unused".into() },
                &actor,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn reveal_rejects_failed_supplier_detail_authorization_before_supplier_and_ciphertext_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("reveal_detail_guard");
        let authorization = unavailable_authorization();
        let access = SupplierAccess::new(db.clone(), authorization.clone());
        let process = SupplierProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"supplier-reveal-detail-guard")),
            authorization,
            access,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error = process
            .reveal_supplier("token-supplier", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
