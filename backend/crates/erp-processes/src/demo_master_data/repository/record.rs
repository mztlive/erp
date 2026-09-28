//! 演示主数据清单。删除和再次生成只认这里记下的身份。

use mongodb::bson::{Document, doc};
use mongodb::options::{IndexOptions, UpdateOptions};
use mongodb::{Database, IndexModel};
use persistence_core::{Executor, NoTransaction, mongo_ops};
use serde::{Deserialize, Serialize};

use super::super::plan::DemoKind;
use crate::{Error, Result};

pub(in crate::demo_master_data) const COLLECTION: &str = "demo_master_records";

/// 一条已生成的演示主数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::demo_master_data) struct DemoMasterRecord {
    /// 稳定身份。
    pub key: String,
    /// [`DemoKind::as_str`] 的值。
    pub kind: String,
    /// 业务记录 ID。
    pub entity_id: String,
    /// 一起恢复或删除的关联 ID，例如主体和 SKU。
    pub related_ids: Vec<String>,
    /// 列表里能认出的名称。
    pub label: String,
    /// 为 true 时已经从列表删除。
    pub removed: bool,
}

impl DemoMasterRecord {
    /// 按种类代码读取种类。
    pub(in crate::demo_master_data) fn kind(&self) -> Option<DemoKind> {
        DemoKind::parse(&self.kind)
    }
}

/// 读取全部演示清单。
///
/// # 参数
/// * `db` - 目标数据库
///
/// # 返回
/// 返回清单中的全部记录。
///
/// # 错误
/// 查询失败时返回错误。
pub(in crate::demo_master_data) async fn load_all(db: &Database) -> Result<Vec<DemoMasterRecord>> {
    load(db, &mut NoTransaction).await
}

/// 在调用方事务内读取登记，包含旧版软删除记录。
///
/// # 参数
/// `db` - 数据库；`executor` - 当前事务。
/// # 返回
/// 返回全部登记。
/// # 错误
/// 数据库读取失败时返回错误。
pub(in crate::demo_master_data) async fn load(
    db: &Database,
    executor: &mut dyn Executor,
) -> Result<Vec<DemoMasterRecord>> {
    Ok(mongo_ops::find_many(&db.collection(COLLECTION), doc! {}, Default::default(), executor).await?)
}

/// 主数据清理成功后在同一事务内删除对应登记。
///
/// # 参数
/// `db` - 数据库；`records` - 本批登记；`executor` - 当前事务。
/// # 返回
/// 全部登记删除成功时返回空结果。
/// # 错误
/// 登记发生变更或数据库删除失败时返回错误。
pub(in crate::demo_master_data) async fn forget(
    db: &Database,
    records: &[DemoMasterRecord],
    executor: &mut dyn Executor,
) -> Result<()> {
    for row in records {
        let result = mongo_ops::delete_many(
            &db.collection::<Document>(COLLECTION),
            doc! {"key": &row.key, "entity_id": &row.entity_id},
            executor,
        )
        .await?;
        if result.deleted_count != 1 {
            return Err(Error::BusinessLogicError("演示登记发生变化，请刷新后重试".into()));
        }
    }
    Ok(())
}

/// 按稳定身份写入或覆盖一条清单。
///
/// # 参数
/// * `db` - 目标数据库
/// * `record` - 要保存的清单行
///
/// # 错误
/// 序列化或写入失败时返回错误。
pub(in crate::demo_master_data) async fn save(db: &Database, record: &DemoMasterRecord) -> Result<()> {
    let document =
        mongodb::bson::serialize_to_document(record).map_err(|error| Error::Internal(error.to_string()))?;
    db.collection::<Document>(COLLECTION)
        .update_one(doc! { "key": &record.key, "entity_id": &record.entity_id }, doc! { "$set": document })
        .with_options(UpdateOptions::builder().upsert(true).build())
        .await
        .map_err(persistence_core::Error::from)?;
    Ok(())
}

/// 创建演示清单的唯一索引。
///
/// # 参数
/// * `db` - 目标数据库
///
/// # 错误
/// 索引创建失败时返回错误。
pub async fn ensure_indexes(db: &Database) -> persistence_core::Result<()> {
    let index = IndexModel::builder()
        .keys(doc! { "key": 1 })
        .options(IndexOptions::builder().name("uk_demo_master_records_key".to_string()).unique(true).build())
        .build();
    db.collection::<Document>(COLLECTION).create_indexes(vec![index]).await?;
    Ok(())
}

/// 拒绝把同编号的非演示记录自动收编进删除清单。
///
/// # 参数
/// `db` - 数据库；`key` - 稳定种子键；`id` - 拟复用的实际主键。
///
/// # 返回
/// 归属一致时返回空结果。
///
/// # 错误
/// 记录未登记、ID 不符或查询失败时返回错误。
pub(in crate::demo_master_data) async fn ensure_owned(db: &Database, key: &str, id: &str) -> Result<()> {
    let record = db
        .collection::<DemoMasterRecord>(COLLECTION)
        .find_one(doc! {"key": key})
        .await
        .map_err(persistence_core::Error::from)?;
    check_owner(record.as_ref(), id)
}

/// 只有清单登记的同一 ID 可以恢复或继续使用。
fn check_owner(record: Option<&DemoMasterRecord>, id: &str) -> Result<()> {
    if record.is_none_or(|row| row.entity_id != id) {
        return Err(Error::BusinessLogicError("演示编号已被未登记的资料占用，请先核对资料归属".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DemoMasterRecord, check_owner};

    #[test]
    fn only_registered_identity_can_be_adopted() {
        let row = DemoMasterRecord {
            key: "key".into(),
            kind: "product".into(),
            entity_id: "owned".into(),
            related_ids: vec![],
            label: "name".into(),
            removed: true,
        };
        assert!(check_owner(Some(&row), "owned").is_ok());
        assert!(check_owner(Some(&row), "other").is_err());
        assert!(check_owner(None, "owned").is_err());
    }
}
