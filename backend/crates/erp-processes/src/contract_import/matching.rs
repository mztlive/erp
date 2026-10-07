//! 归档事务内精确匹配当前主数据并冻结身份快照。
use erp_contract::entity::recognition::{
    ContractExtraction, ContractField, ContractImport, ImportFailure, MatchedIdentity, RecognitionProof,
    match_identity,
};
use erp_core::ids::PartyId;
use erp_customer::CustomerExt;
use erp_customer::repository::prelude::*;
use erp_party::repository::exact_identity::exact_parties;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub(super) async fn match_all(
    db: &Database,
    task: &ContractImport,
    extraction: &ContractExtraction,
    executor: &mut dyn Executor,
) -> Result<RecognitionProof> {
    let customer_party = identity(
        db,
        extraction,
        ContractField::CustomerName,
        ContractField::CustomerCreditCode,
        false,
        executor,
    )
    .await?;
    let company = identity(
        db,
        extraction,
        ContractField::CompanyName,
        ContractField::CompanyCreditCode,
        true,
        executor,
    )
    .await?;
    let settlement = identity(
        db,
        extraction,
        ContractField::SettlementName,
        ContractField::SettlementCreditCode,
        false,
        executor,
    )
    .await?;
    let customer = db
        .customer_accounts()
        .find_by_party(&PartyId::new(customer_party.id.clone()), executor)
        .await?
        .filter(|row| row.is_active())
        .ok_or_else(|| Error::BusinessLogicError("未找到对应的启用客户，请先维护客户后重试".into()))?;
    if task.command.expected_customer_id.as_ref().is_some_and(|id| id != &customer.base.id) {
        return Err(Error::BusinessLogicError(
            "识别出的客户与当前开单客户不一致，请上传对应客户的合同".into(),
        ));
    }
    Ok(RecognitionProof {
        import_id: task.base.id.clone(),
        source_sha256: task.source.sha256.clone(),
        customer_id: customer.base.id,
        customer_version: customer.base.version,
        customer_party,
        company,
        settlement,
        extraction: extraction.clone(),
    })
}

async fn identity(
    db: &Database,
    extraction: &ContractExtraction,
    name_field: ContractField,
    credit_field: ContractField,
    company: bool,
    executor: &mut dyn Executor,
) -> Result<MatchedIdentity> {
    let name = extraction.required(name_field).map_err(failure)?;
    let candidates = exact_parties(
        db,
        name,
        company,
        extraction.fields.get(&credit_field).map(|v| v.value.trim()),
        executor,
    )
    .await?
    .into_iter()
    .map(|party| MatchedIdentity {
        id: party.base.id,
        version: party.base.version,
        revision_id: party.stable.current_revision_id,
        legal_name: name.to_string(),
        credit_code: party.unified_credit_code,
    })
    .collect();
    match_identity(name, extraction.fields.get(&credit_field).map(|value| value.value.as_str()), candidates)
        .map_err(|error| {
            let label = match name_field {
                ContractField::CustomerName => "对方签约主体",
                ContractField::CompanyName => "我方签约主体",
                _ => "结算主体",
            };
            let page = extraction.fields.get(&name_field).map(|field| field.page).unwrap_or(0);
            Error::BusinessLogicError(format!("{label}（第 {page} 页）：{}", error.message))
        })
}

fn failure(error: ImportFailure) -> Error {
    Error::BusinessLogicError(error.message)
}
