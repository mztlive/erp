//! 导入批次与来源系统展示在同一功能资格快照内读取。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::ids::SourceSystemId;
use erp_import::{
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView, LegacyImportService,
    PageView,
};
use erp_support::SourceRegistryExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::Result;

/// 在原功能资格快照内读取导入批次页及来源名称。
///
/// # 参数
/// * `db` - 批次与来源系统所在数据库。
/// * `service` - 共享身份设施装配的批次查询服务。
/// * `params` - 批次查询参数。
/// * `actor` - 当前认证账号。
///
/// # 返回
/// 返回补齐来源名称的批次页；事务成功后允许交付。
///
/// # 错误
/// 功能资格失效、参数非法、仓储或事务失败时拒绝交付。
pub async fn batch_list(
    db: &Database,
    service: &LegacyImportService,
    params: &LegacyImportBatchListParams,
    actor: &AuditActor,
) -> Result<PageView<LegacyImportBatchListItem>> {
    let (db, service, params, actor) = (db.clone(), service.clone(), params.clone(), actor.clone());
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                let mut page = service.batch_list(&params, &actor, executor).await?;
                batch_names(&db, &mut page.items, executor).await?;
                Ok(page)
            })
        })
        .await
}

/// 在原功能资格快照内读取导入批次详情及来源名称。
///
/// # 参数
/// * `db` - 批次与来源系统所在数据库。
/// * `service` - 共享身份设施装配的批次查询服务。
/// * `id` - 批次 ID。
/// * `actor` - 当前认证账号。
///
/// # 返回
/// 返回补齐来源名称的详情；事务成功后允许交付。
///
/// # 错误
/// 功能资格失效、批次不存在、仓储或事务失败时拒绝交付。
pub async fn batch_detail(
    db: &Database,
    service: &LegacyImportService,
    id: &str,
    actor: &AuditActor,
) -> Result<LegacyImportBatchView> {
    let (db, service, id, actor) = (db.clone(), service.clone(), id.to_owned(), actor.clone());
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                let mut view = service.batch_detail(&id, &actor, executor).await?;
                batch_name(&db, &mut view, executor).await?;
                Ok(view)
            })
        })
        .await
}

/// 为当前批次页批量补齐来源系统名称。
///
/// # 参数
/// * `db` - 批次所在数据库。
/// * `items` - 已通过现有批次列表读取的当前页。
/// * `executor` - 批次根读取使用的原快照执行器。
///
/// # 返回
/// 原位补齐名称；关联缺失时保留空值。
///
/// # 错误
/// 来源系统仓储查询失败时返回错误。
async fn batch_names(
    db: &Database,
    items: &mut [LegacyImportBatchListItem],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = items.iter().map(|item| SourceSystemId::new(&item.source_system_id)).collect::<Vec<_>>();
    let names = source_names(db, &ids, executor).await?;

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
/// * `executor` - 批次根读取使用的原快照执行器。
///
/// # 返回
/// 原位补齐名称；关联缺失时保留空值。
///
/// # 错误
/// 来源系统仓储查询失败时返回错误。
async fn batch_name(
    db: &Database,
    item: &mut LegacyImportBatchView,
    executor: &mut dyn Executor,
) -> Result<()> {
    let names = source_names(db, &[SourceSystemId::new(&item.source_system_id)], executor).await?;
    item.source_system_name = names.get(&item.source_system_id).cloned();
    Ok(())
}

/// 仅沿批次根读取的执行器批量补齐来源名称。
async fn source_names(
    db: &Database,
    ids: &[SourceSystemId],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    Ok(db
        .source_systems()
        .find_systems_by_ids(ids, executor)
        .await?
        .into_iter()
        .filter(|item| !item.name.trim().is_empty())
        .map(|item| (item.base.id, item.name))
        .collect())
}
