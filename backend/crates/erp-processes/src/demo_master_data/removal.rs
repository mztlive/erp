//! 一批关联、主数据和登记共享一个硬删除事务。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_party::PartyExt;
use mongodb::Database;

use super::derived::{self, DeletionSet, MongoStore, Store};
use super::master_graph;
use super::plan::{self, DemoKind, DemoStep};
use super::record::{self, DemoMasterRecord};
use crate::audit::run_audited;
use crate::{Error, Result};

/// 本批硬删除完成情况。
pub(super) struct Outcome {
    pub removed: u32,
    pub related: u32,
    pub done: bool,
}

/// 登记读取、公司保护及登记清理沿用本批事务。
#[async_trait]
pub(super) trait RemovalStore: Store {
    /// 读取全部登记。
    ///
    /// # 参数
    /// 无；使用当前事务。
    /// # 返回
    /// 包含旧版本软删除状态的全部登记。
    /// # 错误
    /// 数据库读取失败时返回错误。
    async fn records(&mut self) -> Result<Vec<DemoMasterRecord>>;
    /// 检查待删除主体是否承担公司角色。
    ///
    /// # 参数
    /// ids - 待检查主体实际 ID。
    /// # 返回
    /// 均不承担公司角色时返回空结果。
    /// # 错误
    /// 存在公司角色或读取失败时返回错误。
    async fn guard_companies(&mut self, ids: &[String]) -> Result<()>;
    /// 清理已经完成硬删除的登记。
    ///
    /// # 参数
    /// records - 本批登记。
    /// # 返回
    /// 全部登记删除成功时返回空结果。
    /// # 错误
    /// 登记变化或删除失败时返回错误。
    async fn forget(&mut self, records: &[DemoMasterRecord]) -> Result<()>;
}

#[async_trait]
impl RemovalStore for MongoStore<'_> {
    /// 读取包括旧版已软删除记录在内的登记。
    async fn records(&mut self) -> Result<Vec<DemoMasterRecord>> {
        record::load(self.db, self.executor).await
    }

    /// 登记主体后来成为我方公司时必须拒绝删除。
    async fn guard_companies(&mut self, ids: &[String]) -> Result<()> {
        for id in ids {
            if self
                .db
                .parties()
                .find_by_id_including_deleted(id, self.executor)
                .await?
                .is_some_and(|party| party.company_profile.is_some())
            {
                return Err(Error::BusinessLogicError("演示主体已用作公司主体，不能删除".into()));
            }
        }
        Ok(())
    }

    /// 仅在主数据和子记录全部清理成功后移除实际 ID 登记。
    async fn forget(&mut self, records: &[DemoMasterRecord]) -> Result<()> {
        record::forget(self.db, records, self.executor).await
    }
}

/// 硬删除一个批次并记录审计。
///
/// # 参数
/// `db` - 数据库；`actor` - 操作人；`steps` - 当前 JSON 计划。
/// # 返回
/// 返回主数据和关联记录数量，以及是否全部完成。
/// # 错误
/// 混用、残留、登记变化或事务失败时整批回滚。
pub(super) async fn remove(db: &Database, actor: &AuditActor, steps: Vec<DemoStep>) -> Result<Outcome> {
    let audit =
        actor.clone().resource_log("demo_master_data.purge", "demo_master_data", "master-data".into())?;
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move { execute(&mut MongoStore { db, executor }, &steps).await })
    })
    .await
}

/// 完整校验后才开始删除；登记最后清理，支持失败后重新调用。
///
/// # 参数
/// store - 当前事务数据访问；steps - JSON 计划。
/// # 返回
/// 本批删除计数及完成状态。
/// # 错误
/// 混用、数据库操作或残留校验失败时返回错误。
pub(super) async fn execute(store: &mut impl RemovalStore, steps: &[DemoStep]) -> Result<Outcome> {
    let mut records = store.records().await?;
    refresh_skus(store, &mut records).await?;
    let all = master_graph::roots(&records)?;
    master_graph::guard(store, &all).await?;
    store.guard_companies(&master_graph::values(&all, "parties")).await?;
    let selected = select(&records, steps);
    let mut roots = master_graph::roots(&selected)?;
    // 共用主体等最后一个已登记角色被清理时再删除。
    for row in records.iter().filter(|row| !selected.iter().any(|item| item.key == row.key)) {
        if matches!(row.kind(), Some(DemoKind::Customer | DemoKind::Supplier))
            && let Some(parties) = roots.get_mut("parties")
        {
            for id in &row.related_ids {
                parties.remove(id);
            }
        }
    }
    let seed = derived::master_ids(&records);
    let mut doomed = derived::collect(store, &seed).await?;
    for (collection, ids) in master_graph::collect(store, &roots).await? {
        doomed.entry(collection).or_default().extend(ids);
    }
    let primary = selected.iter().map(|row| row.entity_id.clone()).collect::<Vec<_>>();
    let related = erase(store, &doomed, &primary).await?;
    if !derived::collect(store, &seed).await?.is_empty() {
        return Err(Error::BusinessLogicError("演示关联数据仍有残留，删除未完成".into()));
    }
    let remaining = master_graph::collect(store, &roots).await?;
    verify(store, &remaining).await?;
    store.forget(&selected).await?;
    let removed = u32::try_from(selected.len()).map_err(|_| Error::Internal("主数据数量超出范围".into()))?;
    Ok(Outcome {
        removed,
        related: u32::try_from(related).map_err(|_| Error::Internal("关联数据数量超出范围".into()))?,
        done: selected.len() == records.len(),
    })
}

/// 按种类依赖及 JSON 逆序分批，旧版本移出 JSON 的记录也参与排序。
fn select(records: &[DemoMasterRecord], steps: &[DemoStep]) -> Vec<DemoMasterRecord> {
    let keys = records.iter().map(|row| row.key.clone()).collect::<Vec<_>>();
    let order = plan::removal_batch(steps, &keys, keys.len());
    let mut selected = records.to_vec();
    selected.sort_by_key(|row| {
        let rank = match row.kind() {
            Some(DemoKind::Product) => 0,
            Some(DemoKind::Supplier) => 1,
            Some(DemoKind::Customer) => 2,
            Some(DemoKind::Warehouse) => 3,
            Some(DemoKind::Category) => 4,
            Some(DemoKind::Brand) => 5,
            Some(DemoKind::Unit) => 6,
            None => 7,
        };
        (rank, order.iter().position(|key| key == &row.key))
    });
    selected.truncate(plan::CHUNK_LEN);
    selected
}

/// 事务内补齐当前和历史 SKU；旧登记不因 removed 标志而跳过。
async fn refresh_skus(store: &mut impl Store, records: &mut [DemoMasterRecord]) -> Result<()> {
    master_graph::roots(records)?;
    for row in records.iter_mut().filter(|row| row.kind() == Some(DemoKind::Product)) {
        let hits = store.linked("skus", "product_id", std::slice::from_ref(&row.entity_id), None).await?;
        row.related_ids.extend(hits.into_iter().map(|hit| hit.id));
        row.related_ids.sort();
        row.related_ids.dedup();
    }
    Ok(())
}

/// 只按已经收集的实际 ID 删除，并检查每个 ID 的残留。
async fn erase(store: &mut impl Store, doomed: &DeletionSet, primary: &[String]) -> Result<u64> {
    let mut related_count = 0;
    for (collection, ids) in doomed {
        let (masters, related): (Vec<_>, Vec<_>) = ids.iter().cloned().partition(|id| primary.contains(id));
        if !masters.is_empty() {
            store.delete(collection, &masters).await?;
        }
        if !related.is_empty() {
            related_count += store.delete(collection, &related).await?;
        }
    }
    verify(store, doomed).await?;
    Ok(related_count)
}

/// 重新读取目标 ID；任何残留均使当前事务失败。
async fn verify(store: &mut impl Store, doomed: &DeletionSet) -> Result<()> {
    for (collection, ids) in doomed {
        if !store.linked(collection, "id", &ids.iter().cloned().collect::<Vec<_>>(), None).await?.is_empty() {
            return Err(Error::BusinessLogicError("演示数据仍有残留，删除未完成".into()));
        }
    }
    Ok(())
}
