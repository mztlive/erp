//! 门户申请、开放资格与独立成功回执的集合访问器。
use application_core::{CommandFingerprint, CommandReceipt};
use entity_core::BaseModel;
use entity_macros::Entity;
use mongodb::Database;
use persistence_core::Repository;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{OfferingApplication, QuoteAccessGrant};
use crate::{Error, Result};

/// 所有门户命令独立保存成功结果，不依赖审计展示。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PortalCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub actor_id: String,
    pub supplier_id: String,
    pub action: String,
    pub resource_type: String,
    pub scope_id: Option<String>,
    pub fingerprint: String,
    pub fingerprint_algorithm: String,
    pub key_hash: String,
    pub result: Value,
}
impl PortalCommandReceipt {
    /// 保存规范命令身份和原结果。
    /// # 参数
    /// `command` 来自认证操作人和原载荷；`supplier_id` 来自有效绑定。
    /// # 返回
    /// 不可变成功回执。
    /// # 错误
    /// 身份、结果或指纹非法时拒绝。
    pub fn new(command: &CommandReceipt, supplier_id: &str, result: &Value) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(command.id().to_string()),
            schema_version: 1,
            actor_id: command.actor_id().to_string(),
            supplier_id: supplier_id.to_string(),
            action: command.action().to_string(),
            resource_type: command.resource_type().to_string(),
            scope_id: command.scope_id().map(str::to_string),
            fingerprint: command.fingerprint().as_str().to_string(),
            fingerprint_algorithm: "sha256-canonical-v1".to_string(),
            key_hash: command.idempotency_key_hash().digest_hex().to_string(),
            result: result.clone(),
        };
        value.validate()?;
        Ok(value)
    }
    /// 验证回执持久化合同，禁止未知或软删除回执作为未执行重跑。
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法回执成功。
    /// # 错误
    /// 损坏或未知schema失败关闭。
    pub fn validate(&self) -> Result<()> {
        CommandFingerprint::parse(self.fingerprint.clone())
            .map_err(|_| Error::Internal("门户命令回执指纹损坏".into()))?;
        if self.schema_version != 1
            || self.base.is_deleted()
            || self.base.version == 0
            || self.fingerprint_algorithm != "sha256-canonical-v1"
            || self.result.is_null()
            || [&self.base.id, &self.actor_id, &self.supplier_id, &self.action, &self.resource_type]
                .iter()
                .any(|v| v.trim().is_empty())
        {
            return Err(Error::Internal("门户命令回执身份损坏".into()));
        }
        if self.scope_id.as_deref().is_some_and(|scope| scope != self.supplier_id) {
            return Err(Error::Internal("门户命令回执供应商归属损坏".into()));
        }
        if self.key_hash.len() != 64 || !self.key_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Internal("门户命令回执指纹损坏".into()));
        }
        Ok(())
    }
    /// 按完整命令身份恢复结果。
    /// # 参数
    /// `command` 必须使用原操作号和载荷。
    /// # 返回
    /// 原成功结果。
    /// # 错误
    /// 异载荷冲突，身份损坏失败关闭。
    pub fn replay(&self, command: &CommandReceipt) -> Result<Value> {
        self.validate()?;
        if self.base.id != command.id()
            || self.actor_id != command.actor_id()
            || self.action != command.action()
            || self.resource_type != command.resource_type()
            || self.scope_id.as_deref() != command.scope_id()
            || self.key_hash != command.idempotency_key_hash().digest_hex()
        {
            return Err(Error::Internal("门户命令回执身份不匹配".into()));
        }
        if self.fingerprint != command.fingerprint().as_str() {
            return Err(Error::ConflictError("同一操作号已用于不同提交".into()));
        }
        Ok(self.result.clone())
    }
}
/// 门户拥有的集合访问器。
pub trait PortalSupplyExt {
    /// 商业申请集合。
    const PORTAL_APPLICATIONS: &'static str = "supplier_portal_offering_applications";
    /// SKU定向开放集合。
    const PORTAL_QUOTE_GRANTS: &'static str = "supplier_portal_quote_grants";
    /// 命令成功回执集合。
    const PORTAL_COMMAND_RECEIPTS: &'static str = "supplier_portal_command_receipts";
    /// 取得申请仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 调用方执行器驱动的CAS仓储。
    /// # 错误
    /// 无。
    fn portal_applications(&self) -> Repository<'_, OfferingApplication>;
    /// 取得定向开放仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 授权记录仓储。
    /// # 错误
    /// 无。
    fn portal_quote_grants(&self) -> Repository<'_, QuoteAccessGrant>;
    /// 取得独立回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 成功回执仓储。
    /// # 错误
    /// 无。
    fn portal_command_receipts(&self) -> Repository<'_, PortalCommandReceipt>;
}
impl PortalSupplyExt for Database {
    fn portal_applications(&self) -> Repository<'_, OfferingApplication> {
        Repository::new(self, Self::PORTAL_APPLICATIONS)
    }
    fn portal_quote_grants(&self) -> Repository<'_, QuoteAccessGrant> {
        Repository::new(self, Self::PORTAL_QUOTE_GRANTS)
    }
    fn portal_command_receipts(&self) -> Repository<'_, PortalCommandReceipt> {
        Repository::new(self, Self::PORTAL_COMMAND_RECEIPTS)
    }
}
