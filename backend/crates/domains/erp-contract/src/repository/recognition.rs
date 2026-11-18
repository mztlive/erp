//! 导入任务仓储，所有读写均限定创建人。
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Repository};

use super::templates::page;
use crate::entity::recognition::ContractImport;
use crate::{Error, PageView, Result};
pub const IMPORTS: &str = "contract_imports";

/// 合同领域拥有导入任务集合。
pub trait ContractImportExt {
    /// 取得合同识别任务集合的仓储，供写入任务状态和按创建人查询；执行器由调用方提供。
    fn contract_imports(&self) -> Repository<'_, ContractImport>;
}
impl ContractImportExt for Database {
    fn contract_imports(&self) -> Repository<'_, ContractImport> {
        Repository::new(self, IMPORTS)
    }
}

/// 在任务详情或命令中，用认证创建人和任务 ID 一起查询，不把单独的 ID 当作凭证。不存在与不可见统一返回 `NotFound`。
pub async fn owned(
    db: &Database,
    owner: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<ContractImport> {
    db.contract_imports()
        .find_one(doc! { "id": id, "owner_id": owner }, executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入任务不存在或无权查看".into()))
}

/// 同一创建人重试导入请求时，按其身份及原请求键查原任务。无记录返回 `None`，传播数据库读取错误。
pub async fn replay(
    db: &Database,
    owner: &str,
    key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ContractImport>> {
    Ok(db
        .contract_imports()
        .find_one(doc! { "owner_id": owner, "command.request_key": key }, executor)
        .await?)
}

/// 按创建人列出识别任务，可用追加归档的目标合同缩小结果。沿用当前执行器倒序分页，页码限制在 1 至 1,000,000，每页 20 条；传播读取错误。
pub async fn list(
    db: &Database,
    owner: &str,
    number: u64,
    revision_contract_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<PageView<ContractImport>> {
    page(
        db.contract_imports(),
        import_filter(owner, revision_contract_id),
        (number.clamp(1, 1_000_000), 20),
        executor,
    )
    .await
}

fn import_filter(owner: &str, revision_contract_id: Option<&str>) -> Document {
    let mut filter = doc! { "owner_id": owner };
    if let Some(id) = revision_contract_id {
        filter.insert("command.revision_target.contract_id", id);
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::recognition::{ImportCommand, ImportFailure, ImportSource, ImportStatus};
    #[test]
    fn revision_history_always_intersects_owner_and_target_contract() {
        assert_eq!(import_filter("owner-a", None), doc! { "owner_id": "owner-a" });
        assert_eq!(
            import_filter("owner-a", Some("contract-a")),
            doc! {
                "owner_id": "owner-a", "command.revision_target.contract_id": "contract-a",
            }
        );
    }

    #[test]
    fn import_task_round_trips_with_structured_evidence() {
        let task = ContractImport {
            base: entity_core::BaseModel::fake(),
            owner_id: "owner".into(),
            command: ImportCommand {
                request_key: "request-123".into(),
                expected_customer_id: None,
                revision_target: None,
            },
            source: ImportSource {
                file_asset_id: "file1".into(),
                file_name: "contract.pdf".into(),
                sha256: "digest".into(),
                page_count: 1,
            },
            status: ImportStatus::Failed,
            stage: None,
            started_at: Some(100),
            ocr: None,
            extraction: None,
            failure: Some(ImportFailure::new("OCR_NOT_CONFIGURED", "尚未配置")),
            result: None,
            confirmation: None,
            customer_id: None,
        };

        let encoded = mongodb::bson::serialize_to_document(&task).unwrap();
        let restored: ContractImport = mongodb::bson::deserialize_from_document(encoded).unwrap();
        assert_eq!(restored.extraction, task.extraction);
        assert_eq!(restored.source.sha256, "digest");
    }
}
