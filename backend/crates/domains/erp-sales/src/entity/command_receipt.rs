//! 销售命令的独立回执；展示审计不参与结果恢复。

use application_core::CommandFingerprint;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 销售命令已经提交的结果引用，恢复时仍验证领域事实和当前访问资格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SalesCommandResult {
    /// 新建销售单，回放返回其当前详情。
    Created { sales_order_id: SalesOrderId },
    /// 提交形成不可变快照，回放返回原快照与行。
    Submitted { sales_order_id: SalesOrderId, submission_id: SalesOrderSubmissionId },
    /// 交接完成，回放返回当前责任及开放任务。
    HandedOver { sales_order_id: SalesOrderId },
}

impl SalesCommandResult {
    /// 返回结果种类允许的唯一命令动作。
    fn action(&self) -> &'static str {
        match self {
            Self::Created { .. } => "sales_order.create",
            Self::Submitted { .. } => "sales_order.submit",
            Self::HandedOver { .. } => "sales_order.handover",
        }
    }

    /// 返回该结果引用对应的资源类型。
    fn resource_type(&self) -> &'static str {
        match self {
            Self::Submitted { .. } => "sales_order_submission",
            _ => "sales_order",
        }
    }

    /// 返回正式结果对象 ID，提交命令引用原提交快照。
    fn result_id(&self) -> &str {
        match self {
            Self::Created { sales_order_id } | Self::HandedOver { sales_order_id } => sales_order_id.as_ref(),
            Self::Submitted { submission_id, .. } => submission_id.as_ref(),
        }
    }

    /// 返回命令作用的原销售单 ID。
    fn scope_id(&self) -> &str {
        match self {
            Self::Created { sales_order_id }
            | Self::Submitted { sales_order_id, .. }
            | Self::HandedOver { sales_order_id } => sales_order_id.as_ref(),
        }
    }
}

/// 成功销售命令的结构化身份、指纹及强类型结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SalesCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub fingerprint_algorithm: String,
    pub fingerprint: String,
    pub idempotency_key_hash: CommandFingerprint,
    pub result: SalesCommandResult,
    pub audit_event_id: String,
}

impl SalesCommandReceipt {
    /// 构造必须与销售事实、审计同事务写入的回执。
    ///
    /// # 参数
    /// * `id` - 稳定命令 ID
    /// * `actor_id` - 认证账号
    /// * `key` - 只用于计算幂等摘要，不明文落库
    /// * `fingerprint` - 领域规范化请求的十六进制指纹
    /// * `result` - 已提交结果
    /// * `audit_event_id` - 关联审计事件
    ///
    /// # 返回
    /// 返回已通过 `validate` 的独立回执。
    ///
    /// # 错误
    /// 操作号为空时返回 `Error::ValidationError`。命令身份、结果引用或审计事件为空，
    /// 或请求指纹不是 64 位十六进制时，`validate` 返回 `Error::Internal`。
    pub fn new(
        id: String,
        actor_id: &str,
        key: &str,
        fingerprint: String,
        result: SalesCommandResult,
        audit_event_id: String,
    ) -> Result<Self> {
        if key.trim().is_empty() {
            return Err(Error::ValidationError("操作号不能为空".to_string()));
        }
        let receipt = Self {
            base: BaseModel::new(id),
            schema_version: 1,
            actor_id: actor_id.to_string(),
            action: result.action().to_string(),
            resource_type: result.resource_type().to_string(),
            resource_id: Some(result.result_id().to_string()),
            fingerprint_algorithm: "sha256-json-v1".to_string(),
            fingerprint,
            idempotency_key_hash: CommandFingerprint::from_parts([key.trim().to_string()]),
            result,
            audit_event_id,
        };
        receipt.validate()?;
        Ok(receipt)
    }

    /// 校验结构化身份及结果之间的一致性。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 未知 `schema_version`、已删除、身份或结果引用缺失、动作与结果不一致，
    /// 或请求指纹不是 64 位十六进制时返回 `Error::Internal`。
    /// 幂等摘要无法按 v1 解析时返回 `Error::Logic`。
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.base.is_deleted()
            || self.base.id.trim().is_empty()
            || self.actor_id.trim().is_empty()
            || self.action != self.result.action()
            || self.resource_type != self.result.resource_type()
            || self.resource_id.as_deref() != Some(self.result.result_id())
            || self.result.result_id().trim().is_empty()
            || self.result.scope_id().trim().is_empty()
            || self.audit_event_id.trim().is_empty()
            || self.fingerprint_algorithm != "sha256-json-v1"
            || self.fingerprint.len() != 64
            || !self.fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::Internal("销售命令回执格式无效".to_string()));
        }
        CommandFingerprint::parse(self.idempotency_key_hash.as_str())?;
        Ok(())
    }

    /// 核对回执与本次命令身份、作用域、指纹和幂等摘要一致。
    ///
    /// 成功后调用方仍须用回执上的结果做当前授权和业务交叉验证；本方法不返回该结果。
    ///
    /// # 参数
    /// * `command_id` - 稳定命令 ID
    /// * `actor_id` - 认证账号
    /// * `action` - 命令动作
    /// * `scope_id` - 原销售单作用域；创建命令必须为 `None`，提交和交接必须携带原销售单
    /// * `fingerprint` - 本次规范化请求指纹
    /// * `expected_key_hash` - 本次规范化幂等键摘要
    ///
    /// # 返回
    /// 一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 回执损坏时 `validate` 返回 `Error::Internal` 或 `Error::Logic`。
    /// 命令身份、动作、作用域或幂等摘要不一致时返回 `Error::Internal`。
    /// 同一操作号的请求指纹不同时返回 `Error::ConflictError`。
    pub fn matches(
        &self,
        command_id: &str,
        actor_id: &str,
        action: &str,
        scope_id: Option<&str>,
        fingerprint: &str,
        expected_key_hash: &CommandFingerprint,
    ) -> Result<()> {
        self.validate()?;
        if self.base.id != command_id
            || self.actor_id != actor_id
            || self.action != action
            || (action == "sales_order.create" && scope_id.is_some())
            || (action != "sales_order.create" && scope_id.is_none())
            || scope_id.is_some_and(|scope_id| scope_id != self.result.scope_id())
            || &self.idempotency_key_hash != expected_key_hash
        {
            return Err(Error::Internal("销售命令回执与业务对象不一致".to_string()));
        }
        if self.fingerprint != fingerprint {
            return Err(Error::ConflictError("同一操作号已用于不同的销售命令".to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(result: SalesCommandResult) -> SalesCommandReceipt {
        SalesCommandReceipt::new("command".into(), "actor", "raw-key", "a".repeat(64), result, "event".into())
            .unwrap()
    }

    #[test]
    fn typed_results_preserve_creation_submission_and_handover_scopes() {
        for result in [
            SalesCommandResult::Created { sales_order_id: SalesOrderId::new("order") },
            SalesCommandResult::Submitted {
                sales_order_id: SalesOrderId::new("order"),
                submission_id: SalesOrderSubmissionId::new("submission"),
            },
            SalesCommandResult::HandedOver { sales_order_id: SalesOrderId::new("order") },
        ] {
            let receipt = record(result);
            let scope = (receipt.action != "sales_order.create").then_some("order");
            receipt
                .matches(
                    "command",
                    "actor",
                    &receipt.action,
                    scope,
                    &"a".repeat(64),
                    &CommandFingerprint::from_parts(["raw-key".into()]),
                )
                .unwrap();
            assert!(matches!(
                receipt.matches(
                    "command",
                    "actor",
                    &receipt.action,
                    scope,
                    &"b".repeat(64),
                    &CommandFingerprint::from_parts(["raw-key".into()])
                ),
                Err(Error::ConflictError(_))
            ));
            assert!(
                receipt
                    .matches(
                        "command",
                        "other",
                        &receipt.action,
                        scope,
                        &"a".repeat(64),
                        &CommandFingerprint::from_parts(["raw-key".into()])
                    )
                    .is_err()
            );
            assert!(
                receipt
                    .matches(
                        "command",
                        "actor",
                        &receipt.action,
                        Some("other"),
                        &"a".repeat(64),
                        &CommandFingerprint::from_parts(["raw-key".into()])
                    )
                    .is_err()
            );
            assert!(
                receipt
                    .matches(
                        "other-command",
                        "actor",
                        &receipt.action,
                        scope,
                        &"a".repeat(64),
                        &CommandFingerprint::from_parts(["raw-key".into()])
                    )
                    .is_err()
            );
            if scope.is_some() {
                assert!(
                    receipt
                        .matches(
                            "command",
                            "actor",
                            &receipt.action,
                            None,
                            &"a".repeat(64),
                            &CommandFingerprint::from_parts(["raw-key".into()])
                        )
                        .is_err()
                );
            }
            let serialized = serde_json::to_string(&receipt).unwrap();
            assert!(!serialized.contains("raw-key"));
            assert_eq!(serde_json::from_str::<SalesCommandReceipt>(&serialized).unwrap(), receipt);
        }
    }

    #[test]
    fn corrupt_schema_result_and_fingerprint_fail_closed() {
        let receipt = record(SalesCommandResult::Created { sales_order_id: SalesOrderId::new("order") });
        let mut invalid = receipt.clone();
        invalid.schema_version = 2;
        assert!(invalid.validate().is_err());
        let mut invalid = receipt.clone();
        invalid.resource_id = Some("other".into());
        assert!(invalid.validate().is_err());
        let mut invalid = receipt.clone();
        invalid.fingerprint = "bad".into();
        assert!(invalid.validate().is_err());
        let mut invalid = receipt.clone();
        invalid.result = SalesCommandResult::HandedOver { sales_order_id: SalesOrderId::new("order") };
        assert!(invalid.validate().is_err());
        let mut invalid = receipt.clone();
        invalid.resource_type = "sales_order_submission".into();
        assert!(invalid.validate().is_err());
        let mut invalid = receipt;
        invalid.base.deleted_at = 1;
        assert!(invalid.validate().is_err());
    }

    /// 合法但被替换的幂等摘要必须视为身份损坏，并先于异载荷冲突失败。
    #[test]
    fn altered_well_formed_key_digest_cannot_replay_the_command() {
        let expected_key_hash = CommandFingerprint::from_parts(["raw-key".into()]);
        for result in [
            SalesCommandResult::Created { sales_order_id: SalesOrderId::new("order") },
            SalesCommandResult::Submitted {
                sales_order_id: SalesOrderId::new("order"),
                submission_id: SalesOrderSubmissionId::new("snapshot"),
            },
            SalesCommandResult::HandedOver { sales_order_id: SalesOrderId::new("order") },
        ] {
            let mut receipt = record(result);
            let scope = (receipt.action != "sales_order.create").then_some("order");
            receipt
                .matches("command", "actor", &receipt.action, scope, &"a".repeat(64), &expected_key_hash)
                .unwrap();
            receipt.idempotency_key_hash = CommandFingerprint::from_parts(["different-key".into()]);
            receipt.validate().unwrap();
            for fingerprint in ["a".repeat(64), "b".repeat(64)] {
                assert!(matches!(
                    receipt.matches(
                        "command",
                        "actor",
                        &receipt.action,
                        scope,
                        &fingerprint,
                        &expected_key_hash
                    ),
                    Err(Error::Internal(_))
                ));
            }
        }
    }
}
