//! 授权配置命令的独立回执，不依赖审计日志判定重放。
use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::dto::authorization_bundle::PolicyApplyResult;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PolicyReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub command: StructuredCommandReceipt,
    pub result: PolicyApplyResult,
}

impl PolicyReceipt {
    /// 构造与授权变更同事务持久化的结果。
    /// # 参数
    /// command 为稳定命令身份，result 为本次提交结果。
    /// # 返回
    /// 可由原执行器保存的独立回执。
    /// # 错误
    /// 命令身份无法构造结构化回执时拒绝。
    pub(crate) fn new(command: &CommandReceipt, result: PolicyApplyResult) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(command.id().to_owned()),
            command: StructuredCommandReceipt::from_command(command)?,
            result,
        })
    }

    /// 仅同操作人、同键和同载荷允许返回原结果。
    /// # 参数
    /// command 为当前请求重新计算的稳定命令身份。
    /// # 返回
    /// 标记为重放的原结果。
    /// # 错误
    /// 身份、载荷或结果关联不匹配时返回冲突。
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
