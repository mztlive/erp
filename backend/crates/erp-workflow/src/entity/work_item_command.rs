//! 工作项转交和受控关闭的独立命令结果，不依赖审计正文。

use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 工作项命令的强类型成功结果；任务当前视图由消费方重新授权读取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkItemCommandResult {
    Reassigned { work_item_id: String, task_version: u64, target_user_id: String, reason: String },
    Closed { work_item_id: String, task_version: u64, evidence_reference: String },
}

/// 与任务及领域证据同事务保存的不可变命令回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct WorkItemCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub command: StructuredCommandReceipt,
    pub result_schema_version: u16,
    pub result: WorkItemCommandResult,
    pub audit_event_id: String,
}

impl WorkItemCommandReceipt {
    /// 构造已成功执行的领域命令回执。
    /// # 参数
    /// * `command` - 保持原算法的命令身份。
    /// * `result` - 任务写入后形成的强类型事实。
    /// * `audit_event_id` - 同事务业务事件关联。
    /// # 返回
    /// 返回已校验的独立回执。
    /// # 错误
    /// 身份、动作、结果或关联无效时返回错误。
    pub fn new(
        command: &CommandReceipt,
        result: WorkItemCommandResult,
        audit_event_id: String,
    ) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(command.id().to_string()),
            command: StructuredCommandReceipt::from_command(command)?,
            result_schema_version: 1,
            result,
            audit_event_id,
        };
        value.validate()?;
        Ok(value)
    }

    /// 校验持久化结果与命令身份，禁止损坏记录被当作未执行。
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法时返回空结果。
    /// # 错误
    /// schema、结果、命令动作或关联不一致时返回内部错误。
    pub fn validate(&self) -> Result<()> {
        self.command.validate().map_err(|_| corrupted())?;
        let (id, version, action, detail) = match &self.result {
            WorkItemCommandResult::Reassigned { work_item_id, task_version, target_user_id, reason } => {
                if reason.trim().is_empty() {
                    return Err(corrupted());
                }
                (work_item_id, *task_version, "work_item.reassign", target_user_id)
            },
            WorkItemCommandResult::Closed { work_item_id, task_version, evidence_reference } => {
                (work_item_id, *task_version, "work_item.close", evidence_reference)
            },
        };
        if self.base.id != self.command.command_id
            || self.base.is_deleted()
            || self.result_schema_version != 1
            || self.command.resource_type != "work_item"
            || self.command.action != action
            || self.command.scope_id.as_deref() != Some(id)
            || id.trim().is_empty()
            || version == 0
            || detail.trim().is_empty()
            || self.audit_event_id.trim().is_empty()
        {
            return Err(corrupted());
        }
        Ok(())
    }

    /// 按当前请求匹配回执并取得任务及最低已提交版本。
    /// # 参数
    /// * `command` - 当前请求身份和原载荷。
    /// # 返回
    /// 返回已执行任务ID及结果版本。
    /// # 错误
    /// 异载荷保持稳定冲突；身份或结果损坏明确失败。
    pub fn committed_task(&self, command: &CommandReceipt) -> Result<(&str, u64)> {
        self.validate()?;
        match command.match_structured(&self.command) {
            StructuredReceiptMatch::DifferentPayload => {
                return Err(Error::ConflictError("同一操作号已用于不同提交，请重新发起操作".to_string()));
            },
            StructuredReceiptMatch::Corrupted => return Err(corrupted()),
            StructuredReceiptMatch::SamePayload => {},
        }
        Ok(match &self.result {
            WorkItemCommandResult::Reassigned { work_item_id, task_version, .. }
            | WorkItemCommandResult::Closed { work_item_id, task_version, .. } => {
                (work_item_id, *task_version)
            },
        })
    }
}
fn corrupted() -> Error {
    Error::Internal("业务命令收据格式无效".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(target: &str) -> CommandReceipt {
        CommandReceipt::from_resource_parts(
            "work-item-command-",
            "actor",
            "work_item.reassign",
            "work_item",
            "task",
            "key",
            ["1".to_string(), target.to_string(), "reason".to_string()],
        )
        .unwrap()
    }
    #[test]
    fn replay_matches_full_identity_and_typed_result_without_audit_message() {
        let receipt = WorkItemCommandReceipt::new(
            &command("owner"),
            WorkItemCommandResult::Reassigned {
                work_item_id: "task".to_string(),
                task_version: 2,
                target_user_id: "owner".to_string(),
                reason: "reason".to_string(),
            },
            "audit".to_string(),
        )
        .unwrap();
        assert_eq!(receipt.committed_task(&command("owner")).unwrap(), ("task", 2));
        assert!(matches!(receipt.committed_task(&command("other")), Err(Error::ConflictError(_))));
        let mut broken = receipt.clone();
        broken.result_schema_version = 2;
        assert!(matches!(broken.committed_task(&command("owner")), Err(Error::Internal(_))));
        let wire = serde_json::to_string(&receipt).unwrap();
        assert!(!wire.contains("command_fingerprint="));
        assert_eq!(serde_json::from_str::<WorkItemCommandReceipt>(&wire).unwrap(), receipt);
    }
}
