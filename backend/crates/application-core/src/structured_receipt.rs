//! 独立领域命令回执共享的结构化身份与载荷合同，不拥有集合或业务结果。

use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::{CommandFingerprint, CommandReceipt};

/// 由领域回执嵌入的不可变共享身份事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredCommandReceipt {
    /// 本共享身份 schema 版本。
    pub schema_version: u16,
    /// 稳定命令 ID，也是领域回执的唯一键。
    pub command_id: String,
    /// 操作人身份。
    pub actor_id: String,
    /// 稳定动作代码。
    pub action: String,
    /// 结果资源类型。
    pub resource_type: String,
    /// 资源定位命令的目标 ID；创建命令为空。
    pub scope_id: Option<String>,
    /// 规范化幂等键的版本化摘要。
    pub idempotency_key_hash: CommandFingerprint,
    /// 规范化请求的版本化 SHA-256 指纹。
    pub fingerprint: CommandFingerprint,
}

/// 结构化身份与载荷的匹配分类；领域继续验证强类型结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredReceiptMatch {
    /// 命令身份及原载荷一致。
    SamePayload,
    /// 同一命令身份已被不同载荷占用。
    DifferentPayload,
    /// schema、身份或指纹形态损坏。
    Corrupted,
}

impl StructuredCommandReceipt {
    /// 从当前请求构造结构化身份，不改变已有命令 ID 或请求指纹。
    ///
    /// # 参数
    /// * `command` - 已规范化命令身份。
    /// # 返回
    /// 返回可由领域实体持久化的身份事实。
    /// # 错误
    /// 身份字段为空或定位形态不合法时返回错误。
    pub fn from_command(command: &CommandReceipt) -> Result<Self> {
        let value = Self {
            schema_version: 1,
            command_id: command.id().to_string(),
            actor_id: command.actor_id().to_string(),
            action: command.action().to_string(),
            resource_type: command.resource_type().to_string(),
            scope_id: command.scope_id().map(str::to_string),
            idempotency_key_hash: command.idempotency_key_hash().clone(),
            fingerprint: command.fingerprint().clone(),
        };
        value.validate()?;
        Ok(value)
    }

    /// 校验共享身份形态；未知 schema 必须停止恢复。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法时返回空结果。
    /// # 错误
    /// 必填身份缺失或指纹非法时返回错误。
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || [&self.command_id, &self.actor_id, &self.action, &self.resource_type]
                .into_iter()
                .any(|value| value.trim().is_empty())
            || self.scope_id.as_ref().is_some_and(|value| value.trim().is_empty())
        {
            return Err(Error::from("结构化命令回执身份无效"));
        }
        CommandFingerprint::parse(self.fingerprint.as_str())?;
        CommandFingerprint::parse(self.idempotency_key_hash.as_str())?;
        Ok(())
    }
}

impl CommandReceipt {
    /// 比对结构化身份及原算法指纹，完全不读取展示消息。
    ///
    /// # 参数
    /// * `fact` - 拥有领域持久化的共享身份事实。
    /// # 返回
    /// 返回同载荷、异载荷或损坏分类。
    /// # 错误
    /// 无；非法持久化事实归为 Corrupted。
    pub fn match_structured(&self, fact: &StructuredCommandReceipt) -> StructuredReceiptMatch {
        if fact.validate().is_err() || !self.structured_identity_matches(fact) {
            return StructuredReceiptMatch::Corrupted;
        }
        let matched = &fact.fingerprint == self.fingerprint();
        if matched { StructuredReceiptMatch::SamePayload } else { StructuredReceiptMatch::DifferentPayload }
    }

    /// 校验独立持久化回执的完整身份，不查询历史审计候选。
    fn structured_identity_matches(&self, fact: &StructuredCommandReceipt) -> bool {
        fact.actor_id == self.actor_id()
            && fact.action == self.action()
            && fact.resource_type == self.resource_type()
            && fact.command_id == self.id()
            && fact.scope_id.as_deref() == self.scope_id()
            && &fact.idempotency_key_hash == self.idempotency_key_hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(amount: u32) -> CommandReceipt {
        CommandReceipt::from_payload("invoice-", "actor", "invoice.post", "invoice", "key", &amount).unwrap()
    }

    #[test]
    fn structured_matching_preserves_identity_and_payload_conflict() {
        let original = command(10);
        let fact = StructuredCommandReceipt::from_command(&original).unwrap();
        assert_eq!(original.match_structured(&fact), StructuredReceiptMatch::SamePayload);
        assert_eq!(command(20).match_structured(&fact), StructuredReceiptMatch::DifferentPayload);
        assert_eq!(fact.command_id, original.id());
        let encoded = serde_json::to_string(&fact).unwrap();
        let decoded: StructuredCommandReceipt = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, fact);
        assert!(!encoded.contains("\"key\""));
    }

    #[test]
    fn target_scope_and_key_digest_are_part_of_structured_identity() {
        let original = CommandReceipt::from_resource_parts(
            "invoice-",
            "actor",
            "invoice.post",
            "invoice",
            "invoice-1",
            "key",
            ["10".to_string()],
        )
        .unwrap();
        let mut fact = StructuredCommandReceipt::from_command(&original).unwrap();
        assert_eq!(original.match_structured(&fact), StructuredReceiptMatch::SamePayload);
        fact.scope_id = Some("invoice-2".to_string());
        assert_eq!(original.match_structured(&fact), StructuredReceiptMatch::Corrupted);
        fact.scope_id = original.scope_id().map(str::to_string);
        fact.idempotency_key_hash = CommandFingerprint::from_parts(["other".to_string()]);
        assert_eq!(original.match_structured(&fact), StructuredReceiptMatch::Corrupted);
    }

    #[test]
    fn malformed_identity_and_schema_fail_closed() {
        let original = command(10);
        let fact = StructuredCommandReceipt::from_command(&original).unwrap();
        let mut cases = Vec::new();
        let mut changed = fact.clone();
        changed.actor_id = "other".to_string();
        cases.push(changed);
        let mut changed = fact.clone();
        changed.command_id = "different-command-id".to_string();
        cases.push(changed);
        let mut changed = fact.clone();
        changed.schema_version = 2;
        cases.push(changed);
        let mut changed = fact.clone();
        changed.actor_id.clear();
        cases.push(changed);
        for changed in cases {
            assert_eq!(original.match_structured(&changed), StructuredReceiptMatch::Corrupted);
        }
    }
}
