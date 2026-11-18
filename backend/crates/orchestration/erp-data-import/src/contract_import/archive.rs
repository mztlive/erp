//! 合同、不可变修订、导入结果和审计共用同一事务。
use application_core::AuditActor;
use erp_audit::execution::run_audited_event;
use erp_contract::entity::recognition::{
    ConfirmImport, ContractImport, ImportStatus, ImportView, RecognitionProof, ValidatedFields,
};
use erp_contract::repository::prelude::*;
use erp_contract::repository::recognition::ContractImportExt;
use erp_contract::{
    Contract, ContractExt, ContractRevision, ContractStatus, PlannedContractArchive, UploadContractRequest,
    UploadContractView, plan_upload_archive,
};
use erp_core::ids::{CustomerAccountId, FileAssetId, PartyId};
use erp_permissions::contract;
use persistence_core::Executor;

use super::ContractImportProcess;
use super::audit::{ArchiveImport, context};
use crate::{Error, Result};

impl ContractImportProcess {
    /// 人工编辑识别草稿后，由操作人确认自己的导入任务并进入审计归档；识别本身不创建合同。
    /// 相同确认重放返回原归档结果。并发、输入、主数据、权限或事务校验失败时拒绝。
    pub async fn confirm(&self, id: &str, command: ConfirmImport, actor: &AuditActor) -> Result<ImportView> {
        let task = self.task(id, actor).await?;
        task.check_confirmation(&command)?;
        run_audited_event(
            &self.db,
            context(actor, "contract.import.archive", "contract")?,
            ArchiveImport { process: self.clone(), actor: actor.clone(), task, command },
        )
        .await
    }

    // 正式归档时，在调用方事务重读本人任务与原确认，重验源文件、客户和合同权限；相同确认只回放已有结果。
    //
    // - `task`：调用方持有的导入任务；确认仍有效时改用事务内当前记录。
    // - `actor`：当前认证人，用于归属与权限。
    // - `command`：用户确认命令。
    // - `executor`：归档事务执行器。
    //
    // 归档视图，以及本次是否新写入。第二项为 `false` 时表示确认已处理。
    //
    // 任务不归属当前人、确认或源文件不可用、字段或主数据不合法、合同或客户权限不足、追加版本冲突，或仓储写入失败时返回对应错误。
    pub(super) async fn persist_archive(
        &self,
        mut task: ContractImport,
        actor: &AuditActor,
        command: &ConfirmImport,
        executor: &mut dyn Executor,
    ) -> Result<(ImportView, bool)> {
        let current = self.task_stage(&task.base.id, actor, executor).await?;
        if !current.check_confirmation(command)? {
            return Ok((current.into(), false));
        }
        task = current;
        self.require_source(&task, executor).await?;

        let (fields, values) = command.validate().map_err(|error| Error::ValidationError(error.message))?;
        let mut proof = self.match_all(&task, &values, command.create_customer, actor, executor).await?;
        proof.confirmed_fields = Some(command.fields.clone());

        let access = self.services.contract_access.clone();
        access.require_create(actor, &proof.customer_id, executor).await?;
        self.services.customer_access.require_current(actor, &proof.customer_id, executor).await?;

        let request = archive_request(fields, &proof);
        let mut planned = plan_upload_archive(
            request,
            FileAssetId::new(task.source.file_asset_id.clone()),
            PartyId::new(proof.settlement.id.clone()),
            actor.id(),
        )?;
        planned.revision.recognition = Some(proof);

        self.persist_plan(&task, actor, &mut planned, executor).await?;
        let view = self.complete_archive(task, planned, command, executor).await?;
        Ok((view, true))
    }

    async fn complete_archive(
        &self,
        mut task: ContractImport,
        planned: PlannedContractArchive,
        command: &ConfirmImport,
        executor: &mut dyn Executor,
    ) -> Result<ImportView> {
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
        task.confirmation = Some(command.clone());
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
            self.services
                .contract_access
                .clone()
                .require_with(actor.clone(), &contract::TERMINATE, &target.contract_id, executor)
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
            self.services
                .contracts
                .clone()
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
