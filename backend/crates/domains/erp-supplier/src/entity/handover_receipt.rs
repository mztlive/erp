//! 供应商及能力交接的独立身份与强类型结果，两个目标类型不得混用。

use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::{Error, HandoverSupplierCapabilityView, HandoverSupplierView, Result};

/// 供应商维护人交接动作。
pub const SUPPLIER_HANDOVER_ACTION: &str = "supplier.handover";
/// 能力负责人交接动作。
pub const CAPABILITY_HANDOVER_ACTION: &str = "supplier_capability.handover";

/// 首次提交的交接结果快照，回放仍读取相应目标当前视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "view", rename_all = "snake_case")]
pub enum SupplierHandoverResult {
    Supplier(HandoverSupplierView),
    Capability(HandoverSupplierCapabilityView),
}

/// 供应商领域拥有的不可变交接命令回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SupplierHandoverReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 当前身份、目标类型、键摘要及载荷指纹摘要。
    pub command: StructuredCommandReceipt,
    pub result_schema_version: u16,
    pub result: SupplierHandoverResult,
    /// 展示关联，不作为业务恢复来源。
    pub audit_event_id: String,
}

impl SupplierHandoverReceipt {
    /// 构造与正式业务事实同事务提交的交接回执。
    ///
    /// # 参数
    /// * `command` - 当前请求的稳定身份及摘要。
    /// * `result` - 供应商或能力的已提交结果。
    /// * `audit_event_id` - 同事务成功事件编号。
    /// # 返回
    /// 返回已校验的领域回执。
    /// # 错误
    /// 身份、目标类型、结果或版本不合法时返回错误。
    pub fn new(
        command: &CommandReceipt,
        result: SupplierHandoverResult,
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

    /// 校验目标类型、原结果和完整身份，禁止损坏回执继续执行。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法时成功。
    /// # 错误
    /// schema、原结果、身份或展示关联缺失时返回内部错误。
    pub fn validate(&self) -> Result<()> {
        self.command.validate().map_err(|_| corrupted())?;
        let (action, resource, target, owner, supplier_id, version) = match &self.result {
            SupplierHandoverResult::Supplier(view) => (
                SUPPLIER_HANDOVER_ACTION,
                "supplier",
                view.supplier_id.as_str(),
                view.maintainer_user_id.as_str(),
                view.supplier_id.as_str(),
                view.version,
            ),
            SupplierHandoverResult::Capability(view) => (
                CAPABILITY_HANDOVER_ACTION,
                "supplier_capability",
                view.capability_id.as_str(),
                view.owner_user_id.as_str(),
                view.supplier_id.as_str(),
                view.version,
            ),
        };
        if self.base.id != self.command.command_id
            || self.base.is_deleted()
            || self.result_schema_version != 1
            || self.command.action != action
            || self.command.resource_type != resource
            || self.command.scope_id.as_deref() != Some(target)
            || supplier_id.trim().is_empty()
            || owner.trim().is_empty()
            || version == 0
            || self.audit_event_id.trim().is_empty()
        {
            return Err(corrupted());
        }
        Ok(())
    }

    /// 按结构化身份和指纹恢复原命令，不读取审计正文。
    ///
    /// # 参数
    /// * `command` - 本次请求身份和指纹。
    /// # 返回
    /// 同键同载荷时成功，调用方继续重验当前权限及目标归属。
    /// # 错误
    /// 异载荷返回原交接冲突文案；身份或结果损坏返回内部错误。
    pub fn ensure_matches(&self, command: &CommandReceipt) -> Result<()> {
        self.validate()?;
        match command.match_structured(&self.command) {
            StructuredReceiptMatch::SamePayload => Ok(()),
            StructuredReceiptMatch::DifferentPayload => {
                let message = match self.result {
                    SupplierHandoverResult::Supplier(_) => "同一幂等键已用于不同的供应商交接",
                    SupplierHandoverResult::Capability(_) => "同一幂等键已用于不同的能力交接",
                };
                Err(Error::ConflictError(message.into()))
            },
            StructuredReceiptMatch::Corrupted => Err(corrupted()),
        }
    }
}

/// 领域回执损坏的统一分类。
fn corrupted() -> Error {
    Error::Internal("供应商交接命令回执无效".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(capability: bool, payload: &str) -> CommandReceipt {
        let (action, resource) = if capability {
            (CAPABILITY_HANDOVER_ACTION, "supplier_capability")
        } else {
            (SUPPLIER_HANDOVER_ACTION, "supplier")
        };
        CommandReceipt::from_resource_parts(
            "handover-",
            "actor",
            action,
            resource,
            "target",
            "key",
            [payload.to_string()],
        )
        .unwrap()
    }

    fn result(capability: bool) -> SupplierHandoverResult {
        if capability {
            SupplierHandoverResult::Capability(HandoverSupplierCapabilityView {
                supplier_id: "supplier-1".into(),
                capability_id: "target".into(),
                owner_user_id: "user-2".into(),
                version: 2,
            })
        } else {
            SupplierHandoverResult::Supplier(HandoverSupplierView {
                supplier_id: "target".into(),
                maintainer_user_id: "user-2".into(),
                business_org_unit_id: "org-2".into(),
                version: 2,
            })
        }
    }

    #[test]
    fn matcher_preserves_both_results_and_distinct_target_types() {
        for capability in [false, true] {
            let receipt = SupplierHandoverReceipt::new(
                &command(capability, "fp"),
                result(capability),
                "event-1".into(),
            )
            .unwrap();
            assert!(receipt.ensure_matches(&command(capability, "fp")).is_ok());
            assert!(matches!(
                receipt.ensure_matches(&command(capability, "other")),
                Err(Error::ConflictError(_))
            ));
            assert!(matches!(receipt.ensure_matches(&command(!capability, "fp")), Err(Error::Internal(_))));
            let encoded = serde_json::to_string(&receipt).unwrap();
            let restored: SupplierHandoverReceipt = serde_json::from_str(&encoded).unwrap();
            assert!(restored.ensure_matches(&command(capability, "fp")).is_ok());
        }
        assert_ne!(command(true, "fp").id(), command(false, "fp").id());
    }

    #[test]
    fn matcher_rejects_missing_parent_target_schema_and_identity() {
        let original =
            SupplierHandoverReceipt::new(&command(true, "fp"), result(true), "event-1".into()).unwrap();
        let mut cases = Vec::new();
        let mut changed = original.clone();
        changed.command.actor_id = "other".into();
        cases.push(changed);
        let mut changed = original.clone();
        changed.result = result(false);
        cases.push(changed);
        let mut changed = original.clone();
        changed.result_schema_version = 2;
        cases.push(changed);
        let mut changed = original.clone();
        changed.audit_event_id.clear();
        cases.push(changed);
        let mut changed = original;
        if let SupplierHandoverResult::Capability(view) = &mut changed.result {
            view.supplier_id.clear();
        }
        cases.push(changed);
        for changed in cases {
            assert!(matches!(changed.ensure_matches(&command(true, "fp")), Err(Error::Internal(_))));
        }
    }
}
