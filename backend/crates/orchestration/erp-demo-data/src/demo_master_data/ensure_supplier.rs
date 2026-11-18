//! 生成或恢复演示供应商。没有公司主体时跳过，不改其他资料。

use application_core::AuditActor;
use erp_core::ids::PartyId;
use erp_supplier::repository::SupplierProfileCommandRepositoryExt;
use erp_supplier::{HandoverSupplierRequest, SaveSupplierProfileRequest, SupplierExt};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::ReadExecutor;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::DemoStep;
use super::{DemoMasterDataProcess, lifecycle, record, spec};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 用实际公司主体和采购岗位执行供应商种子；重试优先恢复原创建回执对应的供应商。
    // 缺少前置岗位或公司时返回提示，回执失效、写入或登记失败时停止。
    pub(super) async fn ensure_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        request: &SaveSupplierProfileRequest,
        company_party_id: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        let Some(company_party_id) = company_party_id else {
            return Ok(EnsureOutcome::Notice("还没有公司主体，未生成供应商".to_string()));
        };
        if let Some(supplier_id) = self.supplier_command(&step.key).await? {
            return self.adopt_supplier(actor, step, &supplier_id, notices).await;
        }

        let Some(maintainer) = self.role_actor(&spec::foundation_spec().supplier_maintainer_account).await?
        else {
            return Ok(EnsureOutcome::Notice("没有可用的采购账号，未生成供应商".to_string()));
        };
        let view = match self
            .ports
            .suppliers
            .create(supplier_request(request, maintainer.id(), company_party_id), actor)
            .await
            .map_err(Error::from)
        {
            Ok(view) => view,
            Err(Error::ValidationError(message) | Error::Forbidden(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成供应商：{message}")));
            },
            Err(error) => return Err(error),
        };

        self.remember_created_supplier(actor, step, &view.supplier_id).await
    }

    // 从供应商记录的实际外键读回主体并登记两个主键，使后续恢复和关联使用正式关系而不是种子占位值。
    async fn remember_created_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
    ) -> Result<EnsureOutcome> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id_including_deleted(supplier_id, &mut ReadExecutor::default())
            .await?
            .ok_or_else(|| Error::NotFound("演示供应商创建后未能读回".to_string()))?;
        let created = supplier.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;

        lifecycle::restore_party(&self.db, self.authorization.clone(), actor, supplier.party_id.as_ref())
            .await?;
        if !created {
            lifecycle::restore_supplier(&self.db, self.authorization.clone(), actor, supplier_id).await?;
        }

        record::save(&self.db, step, supplier_id.to_string(), vec![supplier.party_id.to_string()]).await?;
        Ok(if created { EnsureOutcome::Created } else { EnsureOutcome::Restored })
    }

    // 以原命令绑定的真实主键恢复供应商及主体，随后对齐采购维护人，避免重试重复创建资料。
    async fn adopt_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        let outcome = self.remember_created_supplier(actor, step, supplier_id).await?;
        self.align_supplier_maintainer(actor, step, supplier_id, notices).await?;
        Ok(match outcome {
            EnsureOutcome::Created => EnsureOutcome::Skipped,
            other => other,
        })
    }

    // 通过正式移交用例把供应商维护责任对齐采购岗位，使演示采购可在实际业务范围内使用供应商。
    async fn align_supplier_maintainer(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<()> {
        let Some(maintainer) = self.role_actor(&spec::foundation_spec().supplier_maintainer_account).await?
        else {
            push_supplier_notice(notices, "没有可用的采购账号，演示供应商仍由原维护人负责");
            return Ok(());
        };
        let Some(supplier) = self
            .db
            .supplier_accounts()
            .find_by_id_including_deleted(supplier_id, &mut ReadExecutor::default())
            .await?
        else {
            return Ok(());
        };
        if supplier.base.deleted_at != entity_core::NOT_DELETED_TIMESTAMP
            || supplier.maintainer_user_id == maintainer.id()
        {
            return Ok(());
        }

        let request = HandoverSupplierRequest {
            expected_version: supplier.base.version,
            target_user_id: maintainer.id().to_string(),
            target_org_unit_id: None,
            reason: "演示供应商交给采购账号".to_string(),
            idempotency_key: format!("demo-handover-{}", step.key),
        };
        if let Err(error) = self.ports.suppliers.handover_supplier(supplier_id, request, actor).await {
            push_supplier_notice(notices, &format!("演示供应商未能交给采购账号：{error}"));
        }
        Ok(())
    }

    // 按稳定种子键读取供应商创建回执，重试沿用原供应商身份，不按相似名称认领记录。
    async fn supplier_command(&self, key: &str) -> Result<Option<String>> {
        let command = self
            .db
            .supplier_profile_commands()
            .find_by_idempotency_key(key, &mut ReadExecutor::default())
            .await?;
        Ok(command.map(|command| command.supplier_id))
    }
}

// 将模板中的公司与采购账号占位值换成当前环境的主键，使正式创建命令能校验真实归属和维护责任。
fn supplier_request(
    template: &SaveSupplierProfileRequest,
    maintainer_id: &str,
    company_party_id: &str,
) -> SaveSupplierProfileRequest {
    let mut request = template.clone();
    request.signing_entity_party_id = PartyId::new(company_party_id);
    request.payment_entity_party_id = PartyId::new(company_party_id);
    request.maintainer_user_id = Some(maintainer_id.to_string());
    for owner in &mut request.capability_owners {
        owner.owner_user_id = maintainer_id.to_string();
    }
    request
}

// 合并供应商准备时的业务提示，重复原因只保留一次，让批次报告可直接指出阻断项。
fn push_supplier_notice(notices: &mut Vec<String>, text: &str) {
    if !notices.iter().any(|item| item == text) {
        notices.push(text.to_string());
    }
}
