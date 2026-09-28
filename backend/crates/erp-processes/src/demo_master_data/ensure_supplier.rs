//! 生成或恢复演示供应商。没有公司主体时跳过，不改其他资料。

use application_core::AuditActor;
use erp_core::ids::PartyId;
use erp_supplier::repository::SupplierProfileCommandRepositoryExt;
use erp_supplier::{HandoverSupplierRequest, SaveSupplierProfileRequest, SupplierExt};
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::DemoStep;
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle, spec};
use crate::{Error, Result};

impl DemoMasterDataService {
    /// 解析公司及采购岗位后执行供应商种子。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`request` - 供应商创建输入；`company_party_id` - 公司 ID；`notices` - 跳过原因集合。
    ///
    /// # 返回
    /// 返回供应商创建、恢复或跳过结果。
    ///
    /// # 错误
    /// 命令回执失效、领域写入或清单登记失败时返回错误。
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
            .suppliers()
            .create(supplier_request(request, maintainer.id(), company_party_id), actor)
            .await
        {
            Ok(view) => view,
            Err(Error::ValidationError(message) | Error::Forbidden(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成供应商：{message}")));
            },
            Err(error) => return Err(error),
        };
        self.remember_created_supplier(actor, step, &view.supplier_id).await
    }

    /// 从供应商实际外键读取主体并登记两个 ID。
    async fn remember_created_supplier(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        supplier_id: &str,
    ) -> Result<EnsureOutcome> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id_including_deleted(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示供应商创建后未能读回".to_string()))?;
        let created = supplier.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        lifecycle::restore_party(&self.db, actor, supplier.party_id.as_ref()).await?;
        if supplier.base.deleted_at != entity_core::NOT_DELETED_TIMESTAMP {
            lifecycle::restore_supplier(&self.db, actor, supplier_id).await?;
        }
        record::save(&self.db, &supplier_record(step, supplier_id, supplier.party_id.as_ref())).await?;
        Ok(if created { EnsureOutcome::Created } else { EnsureOutcome::Restored })
    }

    /// 恢复已登记命令对应的供应商并对齐维护人。
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

    /// 通过移交用例对齐采购维护人。
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
        let Some(supplier) =
            self.db.supplier_accounts().find_by_id_including_deleted(supplier_id, &mut NoTransaction).await?
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
        if let Err(error) = self.suppliers().handover_supplier(supplier_id, request, actor).await {
            push_supplier_notice(notices, &format!("演示供应商未能交给采购账号：{error}"));
        }
        Ok(())
    }

    /// 按稳定幂等键读取此前创建的供应商。
    async fn supplier_command(&self, key: &str) -> Result<Option<String>> {
        let command =
            self.db.supplier_profile_commands().find_by_idempotency_key(key, &mut NoTransaction).await?;
        Ok(command.map(|command| command.supplier_id))
    }
}

/// 将公司和采购账号占位引用解析为当前数据库 ID。
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

/// 构造供应商与主体实际 ID 的登记记录。
fn supplier_record(step: &DemoStep, supplier_id: &str, party_id: &str) -> DemoMasterRecord {
    DemoMasterRecord {
        key: step.key.clone(),
        kind: step.kind.as_str().to_string(),
        entity_id: supplier_id.to_string(),
        related_ids: vec![party_id.to_string()],
        label: step.request.label().to_string(),
        removed: false,
    }
}

/// 合并不重复的供应商准备提示。
fn push_supplier_notice(notices: &mut Vec<String>, text: &str) {
    if !notices.iter().any(|item| item == text) {
        notices.push(text.to_string());
    }
}
