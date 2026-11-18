//! 销售变更提交的独立命令身份；不从当前指针推测历史提交版本。
use application_core::CommandFingerprint;
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::SalesChangeSubmission;
use crate::{Error, Result};

/// 本次请求的稳定去重身份及完整载荷摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeSubmissionCommand {
    pub id: String,
    pub change_id: String,
    pub actor_id: String,
    pub source_version: u64,
    pub key_hash: CommandFingerprint,
    pub payload_hash: CommandFingerprint,
}

impl SalesChangeSubmissionCommand {
    /// 绑定已认证人、原单、操作号以及请求版本和重提说明。
    /// # 参数
    /// * `change_id` / `actor_id` - 已认证操作人与变更单。
    /// * `source_version` - 原请求的变更单锁版本。
    /// * `key` / `reason` - 原请求操作号和可选重提说明。
    /// # 返回
    /// 返回与当前审批版本无关、可精确回放的命令身份。
    /// # 错误
    /// 身份空白、版本为零、操作号或说明超长时拒绝。
    pub fn new(
        change_id: &str,
        actor_id: &str,
        source_version: u64,
        key: &str,
        reason: Option<&str>,
    ) -> Result<Self> {
        let key = key.trim();
        let reason = reason.map(str::trim).unwrap_or_default();
        if change_id.trim().is_empty()
            || actor_id.trim().is_empty()
            || key.is_empty()
            || key.chars().count() > 128
            || reason.chars().count() > 1000
            || source_version == 0
        {
            return Err(Error::ValidationError("销售变更提交命令身份或载荷无效".into()));
        }

        let key_hash = CommandFingerprint::from_parts([key.to_string()]);
        let id = format!(
            "sales-change-submit:{}",
            CommandFingerprint::from_parts([actor_id.to_string(), change_id.to_string(), key.to_string(),])
                .as_str()
        );
        Ok(Self {
            id,
            change_id: change_id.into(),
            actor_id: actor_id.into(),
            source_version,
            key_hash,
            payload_hash: CommandFingerprint::from_parts([source_version.to_string(), reason.to_string()]),
        })
    }
}

/// 持久化冻结提交的不可变身份；不包含当前审批状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesChangeSubmissionFact {
    pub submission_id: String,
    pub change_id: String,
    pub submission_no: u32,
    pub source_version: Option<u64>,
    pub submitted_by: String,
}
impl From<&SalesChangeSubmission> for SalesChangeSubmissionFact {
    fn from(value: &SalesChangeSubmission) -> Self {
        Self {
            submission_id: value.base.id.clone(),
            change_id: value.sales_change_order_id.to_string(),
            submission_no: value.submission_no,
            source_version: value.source_version,
            submitted_by: value.submitted_by.clone(),
        }
    }
}

/// 与销售变更提交和审批实例同事务形成的首次结果证明。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct SalesChangeSubmissionReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub action: String,
    pub command: SalesChangeSubmissionCommand,
    pub submission: SalesChangeSubmissionFact,
    pub approval_process_instance_id: String,
    pub audit_event_id: String,
}
impl SalesChangeSubmissionReceipt {
    /// 冻结原请求对应的精确提交及审批实例，供任何后续版本中回放。
    /// # 参数
    /// * `command` - 完整原请求身份。
    /// * `submission` - 本次事务实际创建的不可变提交。
    /// * `instance_id` / `audit_event_id` - 同事务创建的实例和成功审计。
    /// # 返回
    /// 返回经过原请求版本及归属验证的独立回执。
    /// # 错误
    /// 请求与提交的版本、归属、提交人不一致或结果身份缺失时拒绝。
    pub fn new(
        command: SalesChangeSubmissionCommand,
        submission: SalesChangeSubmissionFact,
        instance_id: String,
        audit_event_id: String,
    ) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(command.id.clone()),
            schema_version: 1,
            action: "sales_change_order.submit".into(),
            command,
            submission,
            approval_process_instance_id: instance_id,
            audit_event_id,
        };
        value.validate(&value.command, &value.submission)?;
        Ok(value)
    }

    /// 校验完整原请求和按回执主键读取的冻结提交，拒绝以当前提交代替。
    /// # 参数
    /// * `expected` - 本次请求形成的命令身份。
    /// * `persisted` - 按回执 submission_id 读取的真实冻结提交。
    /// # 返回
    /// 全部身份和请求载荷一致时成功。
    /// # 错误
    /// 损坏回执、异载荷、旧记录缺原请求版本或冻结事实不一致均拒绝。
    pub fn validate(
        &self,
        expected: &SalesChangeSubmissionCommand,
        persisted: &SalesChangeSubmissionFact,
    ) -> Result<()> {
        if self.schema_version != 1
            || self.action != "sales_change_order.submit"
            || self.base.is_deleted()
            || self.base.id != self.command.id
            || self.approval_process_instance_id.trim().is_empty()
            || self.audit_event_id.trim().is_empty()
            || self.submission.submission_id.trim().is_empty()
            || self.submission.submission_no == 0
        {
            return Err(Error::ConflictError("销售变更提交回执无效".into()));
        }

        if &self.command != expected {
            return Err(Error::ConflictError("同一操作号已用于不同的销售变更提交请求".into()));
        }

        if &self.submission != persisted
            || persisted.change_id != expected.change_id
            || persisted.source_version != Some(expected.source_version)
            || persisted.submitted_by != expected.actor_id
        {
            return Err(Error::ConflictError("销售变更提交回执与原请求版本或冻结提交不一致".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_version_is_required_and_serialized_with_result_identity() {
        let command = SalesChangeSubmissionCommand::new("change", "actor", 4, "key", Some("说明")).unwrap();
        let mut fact = SalesChangeSubmissionFact {
            submission_id: "s1".into(),
            change_id: "change".into(),
            submission_no: 1,
            source_version: None,
            submitted_by: "actor".into(),
        };
        assert!(
            SalesChangeSubmissionReceipt::new(
                command.clone(),
                fact.clone(),
                "instance".into(),
                "audit".into()
            )
            .is_err()
        );

        fact.source_version = Some(4);
        let receipt = SalesChangeSubmissionReceipt::new(
            command.clone(),
            fact.clone(),
            "instance".into(),
            "audit".into(),
        )
        .unwrap();
        let restored: SalesChangeSubmissionReceipt =
            serde_json::from_value(serde_json::to_value(&receipt).unwrap()).unwrap();
        restored.validate(&command, &fact).unwrap();
        assert_eq!(restored, receipt);

        for (actor, version, reason) in [("other", 4, "说明"), ("actor", 5, "说明"), ("actor", 4, "改说明")]
        {
            let other =
                SalesChangeSubmissionCommand::new("change", actor, version, "key", Some(reason)).unwrap();
            assert!(restored.validate(&other, &fact).is_err());
        }
    }
}
