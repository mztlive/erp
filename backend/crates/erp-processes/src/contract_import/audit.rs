//! 导入写入经过统一事务与审计边界，任务回放不追加成功事件。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{BusinessEventContent, BusinessEventContext, BusinessEventResult, registered_action};
use erp_contract::entity::recognition::{ConfirmImport, ContractImport, ImportCommand, ImportView};
use erp_support::FileAsset;
use persistence_core::Executor;

use super::ContractImportProcess;
use crate::audit::{AuditedCommand, AuditedWrite};
use crate::{Error, Result};

/// 按已登记动作构造业务事件上下文。
///
/// # 参数
/// * `actor` - 已认证操作人。
/// * `action` - 稳定动作代码。
/// * `resource` - 资源类型代码。
///
/// # 返回
/// 可供审计回执关联的事件上下文。
///
/// # 错误
/// 动作未登记、资源类型不匹配，或操作人身份、动作元数据无效时返回校验错误。
pub(super) fn context(actor: &AuditActor, action: &str, resource: &str) -> Result<BusinessEventContext> {
    Ok(BusinessEventContext::new(actor.clone(), registered_action(action, resource)?)?)
}

pub(super) struct CreateImport {
    pub process: ContractImportProcess,
    pub actor: AuditActor,
    pub command: ImportCommand,
    pub asset: FileAsset,
    pub digest: (String, u32),
}

#[async_trait]
impl AuditedCommand for CreateImport {
    type Output = (ImportView, bool);

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        let result = self
            .process
            .create_task(self.command.clone(), self.asset.clone(), self.digest.clone(), &self.actor, executor)
            .await?;
        if !result.1 {
            return Ok(AuditedWrite::Replayed(result));
        }
        Ok(AuditedWrite::Fresh { content: content(result.0.id.clone(), None), result })
    }
}

pub(super) struct ArchiveImport {
    pub process: ContractImportProcess,
    pub actor: AuditActor,
    pub task: ContractImport,
    pub command: ConfirmImport,
}

#[async_trait]
impl AuditedCommand for ArchiveImport {
    type Output = ImportView;

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        let (result, created) =
            self.process.persist_archive(self.task.clone(), &self.actor, &self.command, executor).await?;
        if !created {
            return Ok(AuditedWrite::Replayed(result));
        }
        let contract = result.result.as_ref().ok_or_else(|| Error::Internal("归档结果缺失".into()))?;
        Ok(AuditedWrite::Fresh {
            content: content(contract.id.clone(), Some(contract.contract_no.clone())),
            result,
        })
    }
}

fn content(target_id: String, target_number: Option<String>) -> BusinessEventContent {
    BusinessEventContent {
        target_id,
        target_number,
        result: BusinessEventResult::Succeeded,
        field_changes: Vec::new(),
        facts: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn import_actions_validate_real_catalog_and_only_project_safe_identity() {
        let actor = AuditActor::new("actor".into(), "account".into(), AccountKind::Admin);
        for (action, resource, number) in [
            ("contract.import.create", "contract_import", None),
            ("contract.import.archive", "contract", Some("CON-001".to_string())),
        ] {
            let log = context(&actor, action, resource)
                .unwrap()
                .log(content("target".into(), number.clone()))
                .unwrap();
            let event = log.structured_event.unwrap();
            assert_eq!(event.resource_number_snapshot, number);
            assert!(event.field_changes.is_empty());
            assert!(event.facts.is_empty());
        }
        assert!(context(&actor, "contract.import.create", "contract").is_err());
    }
}
