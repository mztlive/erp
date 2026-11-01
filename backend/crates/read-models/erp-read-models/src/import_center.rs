//! 已读取导入批次的来源系统展示投影。

use std::collections::HashMap;

use erp_core::ids::SourceSystemId;
use erp_import::{LegacyImportBatchListItem, LegacyImportBatchView};
use erp_support::SourceRegistryExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

/// 为当前批次页批量补齐来源系统名称。
///
/// # 参数
/// * `db` - 批次所在数据库。
/// * `items` - 已通过现有批次列表读取的当前页。
///
/// # 返回
/// 原位补齐名称；关联缺失时保留空值。
///
/// # 错误
/// 来源系统仓储查询失败时返回错误。
pub async fn batch_names(db: &Database, items: &mut [LegacyImportBatchListItem]) -> Result<()> {
    let ids = items.iter().map(|item| SourceSystemId::new(&item.source_system_id)).collect::<Vec<_>>();
    let names = source_names(db, &ids).await?;
    for item in items {
        item.source_system_name = names.get(&item.source_system_id).cloned();
    }
    Ok(())
}

/// 为已经读取的批次详情补齐来源系统名称。
///
/// # 参数
/// * `db` - 批次所在数据库。
/// * `item` - 已通过现有批次详情读取的对象。
///
/// # 返回
/// 原位补齐名称；关联缺失时保留空值。
///
/// # 错误
/// 来源系统仓储查询失败时返回错误。
pub async fn batch_name(db: &Database, item: &mut LegacyImportBatchView) -> Result<()> {
    let names = source_names(db, &[SourceSystemId::new(&item.source_system_id)]).await?;
    item.source_system_name = names.get(&item.source_system_id).cloned();
    Ok(())
}

async fn source_names(db: &Database, ids: &[SourceSystemId]) -> Result<HashMap<String, String>> {
    Ok(db
        .source_systems()
        .find_systems_by_ids(ids, &mut NoTransaction)
        .await?
        .into_iter()
        .filter(|item| !item.name.trim().is_empty())
        .map(|item| (item.base.id, item.name))
        .collect())
}
