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
    /// 获取导入任务仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 合同任务仓储。
    /// # 错误
    /// 无。
    fn contract_imports(&self) -> Repository<'_, ContractImport>;
}
impl ContractImportExt for Database {
    fn contract_imports(&self) -> Repository<'_, ContractImport> {
        Repository::new(self, IMPORTS)
    }
}

/// 只读取本人任务，不以任务 ID 授权。
/// # 参数
/// * `db` / `owner` / `id` / `executor` - 数据库、身份、任务及执行器。
/// # 返回
/// 已授权任务。
/// # 错误
/// 不存在或不可见时返回相同错误。
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

/// 按本人和请求键查找原任务。
/// # 参数
/// * `db` / `owner` / `key` / `executor` - 数据库、身份、请求键及执行器。
/// # 返回
/// 原任务或空值。
/// # 错误
/// 数据库读取失败。
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

/// 本人任务分页，固定每页 20 条。
/// # 参数
/// * `db` / `owner` / `number` / `revision_contract_id` / `executor` - 数据库、身份、页码、追加目标及执行器。
/// # 返回
/// 倒序任务页。
/// # 错误
/// 数据库读取失败。
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
            started_at: Some(100),
            ocr: None,
            extraction: None,
            failure: Some(ImportFailure::new("OCR_NOT_CONFIGURED", "尚未配置")),
            result: None,
            customer_id: None,
        };
        let encoded = mongodb::bson::serialize_to_document(&task).unwrap();
        let restored: ContractImport = mongodb::bson::deserialize_from_document(encoded).unwrap();
        assert_eq!(restored.extraction, task.extraction);
        assert_eq!(restored.source.sha256, "digest");
    }
}
