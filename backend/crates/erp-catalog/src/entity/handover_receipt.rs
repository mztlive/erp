//! 商品交接的独立回执；结果快照保存首次提交事实，回放读取当前责任视图。

use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::{Error, HandoverProductView, Result};

/// 商品交接稳定动作。
pub const HANDOVER_ACTION: &str = "product.handover";
/// 商品交接目标类型。
pub const HANDOVER_RESOURCE: &str = "product";
/// 商品交接中文动作。
pub const HANDOVER_LABEL: &str = "商品交接";

/// 与正式交接事实同事务提交的不可变命令回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct ProductHandoverReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 当前身份、目标、幂等键摘要及原规范化载荷的显式摘要。
    pub command: StructuredCommandReceipt,
    /// 领域结果 schema。
    pub result_schema_version: u16,
    /// 原交接结果，用于关联核验；回放仍返回当前视图。
    pub result: HandoverProductView,
    /// 展示审计关联，不参与回放。
    pub audit_event_id: String,
}

impl ProductHandoverReceipt {
    /// 保存首次交接结果及独立命令身份。
    ///
    /// # 参数
    /// * `command` - 当前请求身份和摘要。
    /// * `result` - 已持久化的交接结果。
    /// * `audit_event_id` - 同事务展示事件编号。
    /// # 返回
    /// 返回已校验的独立回执。
    /// # 错误
    /// 身份、类型、结果或事件关联不合法时返回错误。
    pub fn new(
        command: &CommandReceipt,
        result: HandoverProductView,
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

    /// 校验身份及强类型结果，禁止损坏回执被当作未执行。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 有效时返回空结果。
    /// # 错误
    /// schema、目标、版本或审计关联无效时返回内部错误。
    pub fn validate(&self) -> Result<()> {
        self.command.validate().map_err(|_| corrupted())?;
        if self.base.id != self.command.command_id
            || self.base.is_deleted()
            || self.command.action != HANDOVER_ACTION
            || self.command.resource_type != HANDOVER_RESOURCE
            || self.result_schema_version != 1
            || self.result.version == 0
            || self.command.scope_id.as_deref() != Some(self.result.product_id.as_str())
            || self.result.maintainer_user_id.trim().is_empty()
            || self.audit_event_id.trim().is_empty()
        {
            return Err(corrupted());
        }
        Ok(())
    }

    /// 使用结构化字段匹配同一请求，不解析展示消息。
    ///
    /// # 参数
    /// * `command` - 本次请求身份及载荷。
    /// # 返回
    /// 同键同载荷时成功；调用方继续读取当前责任视图。
    /// # 错误
    /// 异载荷保留稳定冲突，损坏身份返回内部错误。
    pub fn ensure_matches(&self, command: &CommandReceipt) -> Result<()> {
        self.validate()?;
        match command.match_structured(&self.command) {
            StructuredReceiptMatch::SamePayload => Ok(()),
            StructuredReceiptMatch::DifferentPayload => {
                Err(Error::ConflictError("同一幂等键已用于不同的商品交接".into()))
            },
            StructuredReceiptMatch::Corrupted => Err(corrupted()),
        }
    }
}

/// 损坏回执统一失败分类。
fn corrupted() -> Error {
    Error::Internal("商品交接命令回执无效".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(payload: &str) -> CommandReceipt {
        CommandReceipt::from_resource_parts(
            "handover-",
            "actor",
            HANDOVER_ACTION,
            HANDOVER_RESOURCE,
            "target",
            "key",
            [payload.to_string()],
        )
        .unwrap()
    }

    fn receipt() -> ProductHandoverReceipt {
        ProductHandoverReceipt::new(
            &command("normalized-payload"),
            HandoverProductView {
                product_id: "target".into(),
                maintainer_user_id: "user-2".into(),
                business_org_unit_id: "org-2".into(),
                version: 2,
            },
            "event-1".into(),
        )
        .unwrap()
    }

    #[test]
    fn matcher_preserves_replay_and_payload_conflict() {
        let receipt = receipt();
        assert!(receipt.ensure_matches(&command("normalized-payload")).is_ok());
        assert!(matches!(receipt.ensure_matches(&command("other")), Err(Error::ConflictError(_))));
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(!encoded.contains("\"key\""));
        let restored: ProductHandoverReceipt = serde_json::from_str(&encoded).unwrap();
        assert!(restored.ensure_matches(&command("normalized-payload")).is_ok());
    }

    #[test]
    fn matcher_rejects_corrupted_identity_result_and_schema() {
        let original = receipt();
        let mut cases = Vec::new();
        let mut changed = original.clone();
        changed.command.actor_id = "other".into();
        cases.push(changed);
        let mut changed = original.clone();
        changed.result.product_id = "other".into();
        cases.push(changed);
        let mut changed = original.clone();
        changed.result_schema_version = 2;
        cases.push(changed);
        let mut changed = original.clone();
        changed.result.version = 0;
        cases.push(changed);
        let mut changed = original;
        changed.audit_event_id.clear();
        cases.push(changed);
        for changed in cases {
            assert!(matches!(
                changed.ensure_matches(&command("normalized-payload")),
                Err(Error::Internal(_))
            ));
        }
    }
}
