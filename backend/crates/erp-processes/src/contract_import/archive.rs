//! 合同、不可变修订、导入结果和审计共用同一事务。
use application_core::AuditActor;
use erp_contract::entity::recognition::{
    ContractImport, ImportStatus, ImportView, RecognitionProof, ValidatedFields,
};
use erp_contract::repository::prelude::*;
use erp_contract::repository::recognition::{self, ContractImportExt};
use erp_contract::{
    Contract, ContractExt, ContractRevision, ContractStatus, PlannedContractArchive, UploadContractRequest,
    UploadContractView, plan_upload_archive,
};
use erp_core::ids::{CustomerAccountId, FileAssetId, PartyId};
use persistence_core::Executor;

use super::ContractImportProcess;
use super::audit::{ArchiveImport, context};
use super::matching::match_all;
use crate::adapters::{contract_access, customer_access, scoped_contract_service};
use crate::audit::run_audited_event;
use crate::{Error, Result};

impl ContractImportProcess {
    pub(super) async fn archive(&self, task: &ContractImport, actor: &AuditActor) -> Result<ImportView> {
        run_audited_event(
            &self.db,
            context(actor, "contract.import.archive", "contract")?,
            ArchiveImport { process: self.clone(), actor: actor.clone(), task: task.clone() },
        )
        .await
    }

    pub(super) async fn persist_archive(
        &self,
        mut task: ContractImport,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ImportView> {
        let current = recognition::owned(&self.db, actor.id(), &task.base.id, executor).await?;
        if current.base.version != task.base.version || current.status != ImportStatus::Processing {
            return Err(Error::ConflictError("导入任务状态已变化，请刷新查看结果".into()));
        }
        self.require_source(&task, executor).await?;
        let extraction =
            task.extraction.as_ref().ok_or_else(|| Error::ValidationError("缺少提取结果".into()))?;
        let document = task.ocr.as_ref().ok_or_else(|| Error::ValidationError("缺少逐页识别结果".into()))?;
        document.validate(task.source.page_count).map_err(|e| Error::ValidationError(e.message))?;
        let fields = extraction.validate(document).map_err(|e| Error::ValidationError(e.message))?;
        let proof = match_all(&self.db, &task, extraction, executor).await?;
        let access = contract_access(self.db.clone(), self.rbac.clone());
        access.require_create(actor, &proof.customer_id, executor).await?;
        customer_access(self.db.clone(), self.rbac.clone())
            .require_with(actor.clone(), "detail", &proof.customer_id, executor)
            .await?;
        let request = archive_request(fields, &proof);
        let mut planned = plan_upload_archive(
            request,
            FileAssetId::new(task.source.file_asset_id.clone()),
            PartyId::new(proof.settlement.id.clone()),
            actor.id(),
        )?;
        planned.revision.recognition = Some(proof);
        self.persist_plan(&task, actor, &mut planned, executor).await?;
        task.customer_id = Some(planned.contract.customer_id.to_string());
        task.result = Some(UploadContractView {
            id: planned.contract.base.id.clone(),
            contract_no: planned.contract.contract_no,
            revision_id: planned.revision.base.id,
            revision_no: planned.revision.revision.revision_no,
            file_asset_id: task.source.file_asset_id.clone(),
            file_name: task.source.file_name.clone(),
            created_at: planned.revision.base.created_at,
        });
        task.status = ImportStatus::Succeeded;
        self.db.contract_imports().update(&mut task, executor).await?;
        Ok(task.into())
    }

    async fn persist_plan(
        &self,
        task: &ContractImport,
        actor: &AuditActor,
        planned: &mut PlannedContractArchive,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if let Some(target) = &task.command.revision_target {
            contract_access(self.db.clone(), self.rbac.clone())
                .require_with(actor.clone(), "update", &target.contract_id, executor)
                .await?;
            planned.contract = self.revision_parent(task, planned, executor).await?;
            let number = self
                .db
                .contract_revisions()
                .latest_revision_no(&target.contract_id.clone().into(), executor)
                .await?
                .unwrap_or(0);
            planned.revision.contract_id = target.contract_id.clone().into();
            planned.revision.revision.revision_no = ContractRevision::next_revision_no(number)?;
            self.db
                .contract()
                .archive_contract_revision(&mut planned.contract, &planned.revision, executor)
                .await?;
        } else {
            scoped_contract_service(self.db.clone(), self.rbac.clone())
                .apply_create(&mut planned.contract, &planned.revision, executor)
                .await?;
        }
        Ok(())
    }

    async fn revision_parent(
        &self,
        task: &ContractImport,
        recognized: &PlannedContractArchive,
        executor: &mut dyn Executor,
    ) -> Result<Contract> {
        let target = task
            .command
            .revision_target
            .as_ref()
            .ok_or_else(|| Error::ValidationError("缺少合同目标".into()))?;
        let original = self
            .db
            .contracts()
            .find_by_id(&target.contract_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".into()))?;
        if !original.matches_version(target.version)
            || original.stable.status != ContractStatus::Effective
            || original.contract_no != recognized.contract.contract_no
            || original.customer_id != recognized.contract.customer_id
            || original.settlement_party_id != recognized.contract.settlement_party_id
        {
            return Err(Error::ConflictError("合同版本、编号、客户或结算主体不一致，请核对原合同".into()));
        }
        if let Some(revision_id) = &original.stable.current_revision_id
            && let Some(revision) = self.db.contract_revisions().find_by_id(revision_id, executor).await?
            && let Some(proof) = revision.recognition
            && recognized.revision.recognition.as_ref().is_none_or(|new| new.company.id != proof.company.id)
        {
            return Err(Error::ConflictError("合同新版本不得更换我方签约主体".into()));
        }
        Ok(original)
    }
}

fn archive_request(fields: ValidatedFields, proof: &RecognitionProof) -> UploadContractRequest {
    UploadContractRequest {
        contract_no: fields.contract_no,
        customer_id: CustomerAccountId::new(proof.customer_id.clone()),
        settlement_party_id: Some(PartyId::new(proof.settlement.id.clone())),
        customer_name: proof.customer_party.legal_name.clone(),
        settlement_party_name: proof.settlement.legal_name.clone(),
        payment_term_code: fields.payment_code,
        payment_term_name: fields.payment_name,
        invoice_type: fields.invoice_type,
        tax_point: fields.tax_point,
        signed_at: fields.signed_at,
        valid_from: fields.valid_from,
        valid_to: fields.valid_to,
    }
}
