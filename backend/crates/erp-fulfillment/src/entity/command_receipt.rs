//! 履约拥有的独立成功命令回执，保留原对象当前视图的恢复语义。

use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::CustomerAcceptanceId;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 履约命令的强类型已提交结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum FulfillmentCommandResult {
    /// CustomerAcceptance 结果引用。
    CustomerAcceptance(CustomerAcceptanceId),
}
impl FulfillmentCommandResult {
    /// 返回正式结果对象 ID。
    fn id(&self) -> &str {
        match self {
            Self::CustomerAcceptance(id) => id.as_ref(),
        }
    }
}

/// 与正式事实及成功审计同事务保存的不可变命令回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct FulfillmentCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 结构化身份、作用域、幂等摘要及版本化载荷指纹。
    pub command: StructuredCommandReceipt,
    /// 本领域结果 schema 版本。
    pub result_schema_version: u16,
    /// 原正式结果引用。
    pub result: FulfillmentCommandResult,
    /// 同事务关联成功审计 ID，不反查审计恢复命令。
    pub audit_event_id: String,
}
impl FulfillmentCommandReceipt {
    /// 按显式动作目录构造独立成功回执。
    ///
    /// # 参数
    /// * `command` - 原请求身份及载荷。
    /// * `resource_id` - 正式结果对象 ID。
    /// * `audit_event_id` - 成功审计关联 ID。
    /// # 返回
    /// 返回已验证的领域回执。
    /// # 错误
    /// 目录、身份、schema 或结果无效时返回错误。
    pub fn resource(command: &CommandReceipt, resource_id: String, audit_event_id: String) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(command.id().to_string()),
            command: StructuredCommandReceipt::from_command(command)?,
            result_schema_version: 1,
            result: result_for_command(command.action(), command.resource_type(), resource_id)?,
            audit_event_id,
        };
        value.validate()?;
        Ok(value)
    }

    /// 校验持久化结果与原动作目录。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法时返回空结果。
    /// # 错误
    /// 身份、结果或关联损坏时返回内部错误。
    pub fn validate(&self) -> Result<()> {
        self.command.validate().map_err(|_| corrupted())?;
        if self.result_schema_version != 1
            || self.base.id != self.command.command_id
            || self.base.is_deleted()
            || self.audit_event_id.trim().is_empty()
            || (self.command.action == "customer_acceptance.reverse"
                && self.command.scope_id.as_deref().is_none_or(|id| id == self.result.id()))
            || result_for_command(
                &self.command.action,
                &self.command.resource_type,
                self.result.id().to_string(),
            )? != self.result
        {
            return Err(corrupted());
        }
        Ok(())
    }

    /// 比对原命令并返回原结果对象 ID，不写入或追加审计。
    ///
    /// # 参数
    /// * `command` - 当前请求的原身份及载荷。
    /// # 返回
    /// 返回结果对象 ID；调用方执行原授权和当前视图查询。
    /// # 错误
    /// 异载荷返回稳定冲突；身份或结果损坏返回内部错误。
    pub fn resource_id(&self, command: &CommandReceipt) -> Result<String> {
        self.validate()?;
        match command.match_structured(&self.command) {
            StructuredReceiptMatch::SamePayload => Ok(self.result.id().to_string()),
            StructuredReceiptMatch::DifferentPayload => {
                Err(Error::ConflictError("同一操作号已用于不同提交，请重新发起操作".to_string()))
            },
            StructuredReceiptMatch::Corrupted => Err(corrupted()),
        }
    }
}
/// 明确登记合法动作与强类型结果，不接受任意资源字符串。
fn result_for_command(action: &str, resource_type: &str, id: String) -> Result<FulfillmentCommandResult> {
    if id.trim().is_empty() {
        return Err(corrupted());
    }
    match (action, resource_type) {
        ("customer_acceptance.commit", "customer_acceptance") => {
            Ok(FulfillmentCommandResult::CustomerAcceptance(CustomerAcceptanceId::new(id)))
        },
        ("customer_acceptance.reverse", "customer_acceptance") => {
            Ok(FulfillmentCommandResult::CustomerAcceptance(CustomerAcceptanceId::new(id)))
        },
        _ => Err(corrupted()),
    }
}
/// 原回执损坏保持明确内部错误，不当作命令未执行。
fn corrupted() -> Error {
    Error::Internal("业务命令收据格式无效".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_results_bind_actions_payloads_and_fail_closed_on_damage() {
        for (action, resource_type) in [
            ("customer_acceptance.commit", "customer_acceptance"),
            ("customer_acceptance.reverse", "customer_acceptance"),
        ] {
            let command = test_command(action, resource_type, 10);
            let mut receipt =
                FulfillmentCommandReceipt::resource(&command, "result".to_string(), "audit".to_string())
                    .unwrap();
            assert_eq!(receipt.resource_id(&command).unwrap(), "result");
            let changed = test_command(action, resource_type, 20);
            assert!(matches!(receipt.resource_id(&changed), Err(Error::ConflictError(_))));
            let encoded = serde_json::to_string(&receipt).unwrap();
            assert_eq!(serde_json::from_str::<FulfillmentCommandReceipt>(&encoded).unwrap(), receipt);
            receipt.command.actor_id = "other".to_string();
            assert!(matches!(receipt.resource_id(&command), Err(Error::Internal(_))));
            receipt.command.actor_id = "actor".to_string();
            receipt.result_schema_version = 2;
            assert!(receipt.resource_id(&command).is_err());
        }
        let invalid =
            CommandReceipt::from_payload("receipt-", "actor", "unregistered", "other", "key", &10).unwrap();
        assert!(
            FulfillmentCommandReceipt::resource(&invalid, "result".to_string(), "audit".to_string()).is_err()
        );
    }

    fn test_command(action: &str, resource_type: &str, version: u64) -> CommandReceipt {
        if action == "customer_acceptance.reverse" {
            CommandReceipt::from_resource_parts(
                "receipt-",
                "actor",
                action,
                resource_type,
                "original",
                "key",
                [version.to_string()],
            )
            .unwrap()
        } else {
            CommandReceipt::from_payload("receipt-", "actor", action, resource_type, "key", &version).unwrap()
        }
    }

    #[test]
    fn reverse_receipt_requires_original_scope_and_a_distinct_result() {
        let command = test_command("customer_acceptance.reverse", "customer_acceptance", 1);
        assert!(
            FulfillmentCommandReceipt::resource(&command, "original".to_string(), "audit".to_string())
                .is_err()
        );
        let mut receipt =
            FulfillmentCommandReceipt::resource(&command, "reverse".to_string(), "audit".to_string())
                .unwrap();
        receipt.command.scope_id = None;
        assert!(matches!(receipt.resource_id(&command), Err(Error::Internal(_))));
    }
}
