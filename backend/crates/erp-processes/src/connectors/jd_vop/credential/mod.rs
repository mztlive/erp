//! 固定 Mongo 凭据身份的加密导入及版本化 OAuth 刷新。HTTP 始终在审计事务外执行。
pub mod authorization;
mod crypto;
mod dto;
mod provider;
mod store;

use std::sync::Arc;

use application_core::AuditActor;
use config::JdVopConfig;
use crypto::CredentialCrypto;
pub use dto::{JdVopCredentialImport, JdVopCredentialStatus};
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialPayload, SupplierCredentialReauthorization,
};
use erp_supply::ports::connector::common::ConnectorResult;
use id_generator::next_id;
use mongodb::Database;
pub use provider::MongoJdVopTokenProvider;
use store::{CredentialStore, MongoCredentialStore};

use super::token::token_error;

trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// 本地受控运维入口；预览和状态查询不触发刷新，不实现敏感 Debug。
pub struct JdVopCredentialProcess {
    crypto: CredentialCrypto,
    store: Arc<dyn CredentialStore>,
    clock: Arc<dyn Clock>,
}

impl JdVopCredentialProcess {
    /// 装配加密导入用例，不读取数据库或发送 HTTP。
    /// # 参数
    /// `db` 为应用数据库；`settings` 提供固定身份和独立凭据加密密钥。
    /// # 返回
    /// 受控预览、导入及脱敏状态入口。
    /// # 错误
    /// 凭据配置无效时返回不含密钥或输入正文的错误。
    pub fn new(db: Database, settings: &JdVopConfig) -> ConnectorResult<Self> {
        let crypto = CredentialCrypto::new(settings)?;
        let store = MongoCredentialStore::new(db, crypto.id.clone(), crypto.binding.clone());
        Ok(Self { crypto, store: Arc::new(store), clock: Arc::new(SystemClock) })
    }

    /// 只读预检导入版本及身份，执行与正式写入相同的领域规则。
    /// # 参数
    /// `input` 为受控 JSON；`expected_version` 更新时必需；`reauthorization_evidence` 为显式新授权依据。
    /// # 返回
    /// 当前版本和阶段及待导入有效期；首次创建版本显示0。
    /// # 错误
    /// 身份、令牌、版本、授权恢复期限或存储读取无效时返回脱敏错误。
    pub async fn preview(
        &self,
        input: &JdVopCredentialImport,
        expected_version: Option<u64>,
        reauthorization_evidence: Option<&str>,
    ) -> ConnectorResult<JdVopCredentialStatus> {
        let now = self.clock.now();
        let payload = self.crypto.seal(copy_input(input), now)?;
        let current = self.store.load().await?;
        let recovery = reauthorization_evidence
            .map(|evidence| reauthorization(expected_version, evidence.into(), "preview"))
            .transpose()?;
        let mut draft = current.clone();
        validate_import(&self.crypto, &mut draft, payload, expected_version, recovery.as_ref(), now)?;
        let draft = draft.ok_or_else(token_error)?;
        let mut status = JdVopCredentialStatus::from_record(current.as_ref().unwrap_or(&draft), now);
        status.version = current.as_ref().map_or(0, |record| record.base.version);
        status.expires_at = input.expires_at;
        Ok(status)
    }

    /// 将首次授权或明确版本的更新以密文及同事务审计原子保存。
    /// # 参数
    /// `input/expected_version` 为预检同一导入；恢复未知刷新必须给新授权 `reauthorization_evidence`；`actor` 为受信操作人。
    /// # 返回
    /// 保存后的脱敏版本及阶段。
    /// # 错误
    /// 身份、授权、版本或审计保存失败时拒绝；结果未知不得自动重放。
    pub async fn import(
        &self,
        input: JdVopCredentialImport,
        expected_version: Option<u64>,
        reauthorization_evidence: Option<String>,
        actor: &AuditActor,
    ) -> ConnectorResult<JdVopCredentialStatus> {
        let now = self.clock.now();
        let payload = self.crypto.seal(input, now)?;
        let recovery = reauthorization_evidence
            .map(|evidence| reauthorization(expected_version, evidence, actor.id()))
            .transpose()?;
        let record = self.store.import(payload, expected_version, recovery, actor.clone(), now).await?;
        Ok(JdVopCredentialStatus::from_record(&record, now))
    }

    /// 读取固定凭据身份的脱敏状态，不解密或刷新令牌。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前版本、有效期、刷新阶段和恢复期限。
    /// # 错误
    /// 记录不存在、绑定变化或读取失败时返回脱敏错误。
    pub async fn status(&self) -> ConnectorResult<JdVopCredentialStatus> {
        let record = self.store.load().await?.ok_or_else(token_error)?;
        Ok(JdVopCredentialStatus::from_record(&record, self.clock.now()))
    }
}

fn copy_input(input: &JdVopCredentialImport) -> JdVopCredentialImport {
    JdVopCredentialImport {
        credential_id: input.credential_id.clone(),
        account_scope: input.account_scope.clone(),
        app_key: input.app_key.clone(),
        access_token: input.access_token.clone(),
        refresh_token: input.refresh_token.clone(),
        expires_at: input.expires_at,
    }
}

fn reauthorization(
    expected: Option<u64>,
    evidence: String,
    actor: &str,
) -> ConnectorResult<SupplierCredentialReauthorization> {
    Ok(SupplierCredentialReauthorization {
        expected_version: expected.ok_or_else(token_error)?,
        new_authorization: true,
        operation_id: next_id(),
        actor_id: actor.into(),
        evidence_reference: evidence,
    })
}

fn validate_import(
    crypto: &CredentialCrypto,
    current: &mut Option<SupplierCredential>,
    payload: SupplierCredentialPayload,
    expected: Option<u64>,
    recovery: Option<&SupplierCredentialReauthorization>,
    now: Instant,
) -> ConnectorResult<()> {
    let result = if let Some(record) = current {
        record.require_binding(&crypto.binding).map_err(|_| token_error())?;
        match recovery {
            Some(recovery) => record.reauthorize(payload, recovery, now).map(|_| ()),
            None => record.import(payload, expected.ok_or_else(token_error)?, now),
        }
    } else {
        if expected.is_some() || recovery.is_some() {
            return Err(token_error());
        }
        SupplierCredential::new(crypto.id.clone(), crypto.binding.clone(), payload, now)
            .map(|record| *current = Some(record))
    };
    result.map_err(|_| token_error())
}
