//! 演示主数据清单。删除和再次生成只认这里记下的身份。

use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use mongodb::options::{IndexOptions, UpdateOptions};
use mongodb::{Database, IndexModel};
use serde::{Deserialize, Serialize};

use super::plan::DemoKind;
use crate::{Error, Result};

pub(super) const COLLECTION: &str = "demo_master_records";

/// 一条已生成的演示主数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DemoMasterRecord {
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
    pub(super) fn kind(&self) -> Option<DemoKind> {
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
pub(super) async fn load_all(db: &Database) -> Result<Vec<DemoMasterRecord>> {
    let records = db
        .collection::<DemoMasterRecord>(COLLECTION)
        .find(doc! {})
        .await
        .map_err(persistence_core::Error::from)?
        .try_collect::<Vec<_>>()
        .await
        .map_err(persistence_core::Error::from)?;
    Ok(records)
}

/// 按稳定身份写入或覆盖一条清单。
///
/// # 参数
/// * `db` - 目标数据库
/// * `record` - 要保存的清单行
///
/// # 错误
/// 序列化或写入失败时返回错误。
pub(super) async fn save(db: &Database, record: &DemoMasterRecord) -> Result<()> {
    let document =
        mongodb::bson::serialize_to_document(record).map_err(|error| Error::Internal(error.to_string()))?;
    db.collection::<Document>(COLLECTION)
        .update_one(doc! { "key": &record.key }, doc! { "$set": document })
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
