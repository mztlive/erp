//! 授权配置命令的独立回执，不依赖审计日志判定重放。
use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::dto::authorization_bundle::PolicyApplyResult;
use crate::{Error, Result};

/// 与授权变更一起持久化的命令回执，用于幂等重放原提交结果。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PolicyReceipt {
    /// 以稳定命令 ID 为主键的持久化元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 供重放核对操作人、幂等身份和载荷的结构化命令记录。
    pub command: StructuredCommandReceipt,
    /// 首次提交得到的策略版本、变更数量和关联命令 ID。
    pub result: PolicyApplyResult,
}

impl PolicyReceipt {
    /// 用稳定命令身份和本次应用结果构造回执，供授权变更的同一事务保存。
    /// 命令身份无法转换为结构化回执时返回错误。
    pub(crate) fn new(command: &CommandReceipt, result: PolicyApplyResult) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(command.id().to_owned()),
            command: StructuredCommandReceipt::from_command(command)?,
            result,
        })
    }

    /// 用当前请求计算的命令身份核对操作人、幂等键、载荷以及回执和结果的命令 ID。
    /// 一致时返回标为重放的原结果；任一不符均返回冲突。
    pub(crate) fn replay(&self, command: &CommandReceipt) -> Result<PolicyApplyResult> {
        if command.match_structured(&self.command) != StructuredReceiptMatch::SamePayload
            || self.base.id != command.id()
            || self.result.command_id != command.id()
        {
            return Err(Error::ConflictError("操作号已被不同配置使用或回执损坏".into()));
        }

        let mut result = self.result.clone();
        result.replayed = true;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 固定应用动作和操作号，按参数生成不同操作人或载荷的测试命令。
    fn command(actor: &str, content: &str) -> CommandReceipt {
        CommandReceipt::from_payload(
            "policy-",
            actor,
            "authorization_policy.apply",
            "policy",
            "operation-1",
            &content,
        )
        .unwrap()
    }

    // 验证同一命令重放保留原版本和变更数，操作人、载荷或结果关联变化均导致冲突。
    #[test]
    fn same_actor_and_payload_replay_original_result_and_others_fail_closed() {
        let original = command("operator", "original");
        let receipt = PolicyReceipt::new(
            &original,
            PolicyApplyResult {
                command_id: original.id().into(),
                policy_version: 9,
                change_count: 3,
                replayed: false,
            },
        )
        .unwrap();
        let result = receipt.replay(&original).unwrap();
        assert!(result.replayed);
        assert_eq!(result.policy_version, 9);
        assert_eq!(result.change_count, 3);

        assert!(matches!(receipt.replay(&command("operator", "modified")), Err(Error::ConflictError(_))));
        assert!(matches!(receipt.replay(&command("another", "original")), Err(Error::ConflictError(_))));
        let mut corrupted = receipt;
        corrupted.result.command_id = "other".into();
        assert!(matches!(corrupted.replay(&original), Err(Error::ConflictError(_))));
    }
}
