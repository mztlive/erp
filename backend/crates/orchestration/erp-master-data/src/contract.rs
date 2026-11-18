//! 登记合同文件并写入合同的命名流程。

use application_core::AuditActor;
use erp_audit::execution::persist_log;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_contract::{ContractService, UploadContractRequest, UploadContractView};
use erp_core::ids::FileAssetId;
use erp_support::{FileAsset, FileAssetExt, RegisterFileAssetRequest};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;
use validator::Validate;

use crate::Result;

/// PDF 已写入对象存储后，登记合同身份、首个修订和文件资产，并返回可定位三者的上传视图。
///
/// 对象存储已由 HTTP 适配器写完。本流程持有根事务：文件资产元数据、合同与修订、
/// 两笔成功审计共用同一个执行器。
///
/// - `db`：合同与文件资产所在数据库。
/// - `service`：应用装配且已注入合同数据范围 Port 的领域服务。
/// - `req`：合同业务字段。
/// - `asset_req`：已存储对象的登记元数据。
///
/// 返回合同、首个修订和文件资产的上传视图。
///
/// 字段校验失败、上传计划失败、创建范围重验失败、唯一冲突或事务失败时返回错误。
pub async fn upload_contract(
    db: Database,
    service: ContractService,
    req: UploadContractRequest,
    asset_req: RegisterFileAssetRequest,
    actor: AuditActor,
) -> Result<UploadContractView> {
    req.validate()?;
    asset_req.validate()?;

    let file_name = asset_req.file_name.clone();
    let asset = FileAsset::new(FileAssetId::new(next_id()), asset_req.into_data(actor.id())?)?;

    let file_asset_id = FileAssetId::new(asset.base.id.clone());
    let planned = service.plan_upload(req, file_asset_id, actor.id())?;
    let (asset_audit, contract_audit) =
        upload_audits(&actor, &asset.base.id, &planned.contract.base.id, &planned.contract.contract_no)?;

    let mut contract_for_tx = planned.contract.clone();
    let revision = planned.revision.clone();
    let asset_for_tx = asset.clone();
    let client = db.client().clone();
    let db_for_tx = db.clone();
    let actor_for_tx = actor.clone();
    let customer_id = planned.contract.customer_id.to_string();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                service.require_create(&actor_for_tx, &customer_id, executor).await?;

                db_for_tx.file_assets().create(&asset_for_tx, executor).await?;
                service.apply_create(&mut contract_for_tx, &revision, executor).await?;

                persist_log(&db_for_tx, &asset_audit, executor).await?;
                persist_log(&db_for_tx, &contract_audit, executor).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await?;

    Ok(UploadContractView {
        id: planned.contract.base.id,
        contract_no: planned.contract.contract_no,
        revision_id: planned.revision.base.id,
        revision_no: planned.revision.revision.revision_no,
        file_asset_id: asset.base.id,
        file_name,
        created_at: planned.contract.base.created_at,
    })
}

// 为同一次合同上传生成关联的两条成功审计；共享调用身份，按文件登记、合同创建的顺序编号。
fn upload_audits(
    actor: &AuditActor,
    asset_id: &str,
    contract_id: &str,
    contract_no: &str,
) -> Result<(AuditLog, AuditLog)> {
    let command_id = next_id();
    let asset = actor
        .clone()
        .resource_log("file_asset.register", "file_asset", asset_id.to_string())?
        .with_command_id(Some(command_id.clone()))?
        .with_event_sequence(1)?;
    let contract = actor
        .clone()
        .resource_log("contract.create", "contract", contract_id.to_string())?
        .with_command_id(Some(command_id))?
        .with_resource_number(Some(contract_no.to_string()))?
        .with_event_sequence(2)?;
    Ok((asset, contract))
}

#[cfg(test)]
mod audit_tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn upload_events_share_invocation_and_preserve_file_then_contract_order() {
        let actor = AuditActor::new("actor-1".into(), "sales".into(), AccountKind::Admin);
        let (asset, contract) = upload_audits(&actor, "asset-1", "contract-1", "CT-1").unwrap();
        assert_ne!(asset.base.id, contract.base.id);

        let asset = asset.structured_event.unwrap();
        let contract = contract.structured_event.unwrap();
        assert!(asset.command_id.is_some());
        assert_eq!(asset.command_id, contract.command_id);
        assert_eq!(asset.event_sequence.get(), 1);
        assert_eq!(contract.event_sequence.get(), 2);
        assert_eq!(asset.resource_id, "asset-1");
        assert_eq!(contract.resource_id, "contract-1");
        assert_eq!(contract.resource_number_snapshot.as_deref(), Some("CT-1"));
    }
}
