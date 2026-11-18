//! 目录池事实仓储；页面、成员和外层目录检查点使用同一个 Executor。
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, mongo_ops};

use super::{CatalogMembershipPage, CatalogMembershipScan, CatalogMembershipState, CatalogPoolMember};
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::ports::connector::common::SupplierSku;
use crate::repository::supplier_catalog::ITEMS;
use crate::{Error, Result};

pub(super) const SCANS: &str = "supplier_catalog_membership_scans";
pub(super) const MEMBERS: &str = "supplier_catalog_pool_members";

/// 本域目录池仓储访问器；不暴露其他领域集合。
pub trait CatalogMembershipExt {
    /// 取得当前数据库的完整范围扫描仓储。
    /// 扫描证据仓储。
    fn supplier_catalog_membership_scans(&self) -> Repository<'_, CatalogMembershipScan>;
    /// 取得当前数据库的目录池成员仓储。
    /// 账号连接专属成员仓储。
    fn supplier_catalog_pool_members(&self) -> Repository<'_, CatalogPoolMember>;
}

impl CatalogMembershipExt for Database {
    fn supplier_catalog_membership_scans(&self) -> Repository<'_, CatalogMembershipScan> {
        Repository::new(self, SCANS)
    }
    fn supplier_catalog_pool_members(&self) -> Repository<'_, CatalogPoolMember> {
        Repository::new(self, MEMBERS)
    }
}

/// 保存一页成员与完整范围进度，不自行开启或提交事务。
/// 本页保存后的扫描状态；Uncertain 只保留正向观察。
/// 并发扫描、账号变化、跳页、唯一冲突或数据库错误时拒绝。
/// 所有归属来自服务端任务；`page` 为来源证据，执行器须与目录检查点相同。
pub async fn record_page(
    db: &Database,
    connection: &str,
    supplier: &str,
    job: &str,
    page: &CatalogMembershipPage,
    observed_at: Instant,
    executor: &mut dyn Executor,
) -> Result<CatalogMembershipScan> {
    page.validate()?;
    let existing = db.supplier_catalog_membership_scans().find_by_id(job, executor).await?;
    let is_new = existing.is_none();
    let mut scan = match existing {
        Some(scan) => {
            scan.validate_binding(connection, supplier, &page.scope)?;
            scan
        },
        None => {
            ensure_new_scan(db, connection, supplier, &page.scope.account_scope, executor).await?;
            CatalogMembershipScan::new(job, connection, supplier, page.scope.clone())?
        },
    };

    scan.observe_page(page, observed_at)?;
    if scan.state == CatalogMembershipState::Complete {
        let previous = latest_baseline(db, &scan, executor).await?;
        scan.authorize_reconciliation(previous.as_ref().map(|scan| &scan.scope));
    }

    if is_new {
        db.supplier_catalog_membership_scans().create(&scan, executor).await?;
    } else {
        db.supplier_catalog_membership_scans().update(&mut scan, executor).await?;
    }

    if let Some(pool) = &page.pool_id {
        for sku in &page.members {
            save_member(db, &scan, pool, sku.clone(), observed_at, executor).await?;
        }
    }
    Ok(scan)
}

async fn ensure_new_scan(
    db: &Database,
    connection: &str,
    supplier: &str,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let collection = db.collection::<CatalogMembershipScan>(SCANS);
    let old_account = mongo_ops::find_many(
        &collection,
        doc! {"connection_id":connection,"$or":[{"scope.account_scope":{"$ne":account}},{"supplier_id":{"$ne":supplier}}],"deleted_at":0_i64},
        FindOptions::builder().limit(1).build(),
        executor,
    )
    .await?;
    if !old_account.is_empty() {
        return Err(Error::ConflictError("该连接已绑定其他供应商账号，请新建连接保留原来源命名空间".into()));
    }

    let active = mongo_ops::find_many(
        &collection,
        doc! {"connection_id":connection,"scope.account_scope":account,"active":true,"deleted_at":0_i64},
        FindOptions::builder().limit(1).build(),
        executor,
    )
    .await?;
    if !active.is_empty() {
        return Err(Error::ConflictError("该账号连接已有目录池扫描进行中，请恢复原任务".into()));
    }
    Ok(())
}

async fn latest_baseline(
    db: &Database,
    scan: &CatalogMembershipScan,
    executor: &mut dyn Executor,
) -> Result<Option<CatalogMembershipScan>> {
    let rows = mongo_ops::find_many(&db.collection::<CatalogMembershipScan>(SCANS),
        doc! {"connection_id":&scan.connection_id,"supplier_id":&scan.supplier_id,"scope.account_scope":&scan.scope.account_scope,"state":"RECONCILED","deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"completed_at":-1,"id":-1}).limit(1).build(), executor).await?;
    Ok(rows.into_iter().next())
}

async fn save_member(
    db: &Database,
    scan: &CatalogMembershipScan,
    pool: &str,
    sku: SupplierSku,
    at: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut member = CatalogPoolMember::observed(scan, pool, sku, at)?;
    if let Some(previous) = db.supplier_catalog_pool_members().find_by_id(&member.base.id, executor).await? {
        if previous.supplier_id != member.supplier_id {
            return Err(Error::ConflictError("商品池成员供应商归属冲突".into()));
        }
        member.base = previous.base;
        db.supplier_catalog_pool_members().update(&mut member, executor).await?;
    } else {
        db.supplier_catalog_pool_members().create(&member, executor).await?;
    }
    Ok(())
}

/// 有界读取完整范围内本轮未见的旧成员。
/// 按 ID 排序的旧成员；其他账号、连接及池不进入核对。
/// 扫描没有完整资格、已经中止或仓储失败时拒绝。
/// `after` 为稳定成员 ID 游标，`limit` 必须为 1 至 100；调用方保留事务边界。
pub async fn reconciliation_candidates(
    db: &Database,
    scan: &CatalogMembershipScan,
    after: Option<&str>,
    limit: u32,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogPoolMember>> {
    current_scan(db, scan, executor).await?;
    if !(1..=100).contains(&limit) {
        return Err(Error::ValidationError("目录池核对批次须为1至100条".into()));
    }
    let mut filter = stale_filter(scan);
    if let Some(after) = after {
        filter.insert("id", doc! {"$gt":after});
    }
    Ok(mongo_ops::find_many(
        &db.collection::<CatalogPoolMember>(MEMBERS),
        filter,
        FindOptions::builder().sort(doc! {"id":1}).limit(i64::from(limit)).build(),
        executor,
    )
    .await?)
}

/// 将一条完整范围内的旧成员标记为本轮未见，不删除历史来源身份。
/// 成员失活或已被本轮正向观察保护时完成。
/// 扫描失效、归属冲突或数据库失败时拒绝。
/// `scan` 为完整范围，`member` 来自同一有界核对查询，执行器属外层结果事务。
pub async fn retire_member(
    db: &Database,
    scan: &CatalogMembershipScan,
    member: &CatalogPoolMember,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut fence = current_scan(db, scan, executor).await?;
    db.supplier_catalog_membership_scans().update(&mut fence, executor).await?;

    let Some(mut current) = db.supplier_catalog_pool_members().find_by_id(&member.base.id, executor).await?
    else {
        return Ok(());
    };
    if current.connection_id != scan.connection_id
        || current.account_scope != scan.scope.account_scope
        || current.supplier_id != scan.supplier_id
    {
        return Err(Error::ConflictError("旧商品池成员不属于当前账号或供应商".into()));
    }

    if current.retirement_candidate(&fence) {
        current.active = false;
        db.supplier_catalog_pool_members().update(&mut current, executor).await?;
    }
    Ok(())
}

/// 判断同订货规格本轮是否仍在任一明确授权池观察到。
/// 至少一个授权池有本轮正向成员时为真，禁止撤回该 SKU。
/// 范围不完整或扫描失效时返回错误，不返回可被误解为缺项的假值。
pub async fn has_current_membership(
    db: &Database,
    scan: &CatalogMembershipScan,
    spec_id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    current_scan(db, scan, executor).await?;
    let rows = mongo_ops::find_many(&db.collection::<CatalogPoolMember>(MEMBERS),
        doc! {"connection_id":&scan.connection_id,"supplier_id":&scan.supplier_id,"account_scope":&scan.scope.account_scope,
            "pool_id":{"$in":&scan.scope.pool_ids},"sku.spec_id":spec_id,"active":true,"last_seen_run":&scan.base.id,"deleted_at":0_i64},
        FindOptions::builder().limit(1).build(), executor).await?;
    Ok(rows.iter().any(|member| member.observed_in(scan, spec_id)))
}

/// 完成所有旧成员核对并释放本账号扫描围栏。
/// 当前完整范围成为新的 Reconciled 基线。
/// 仍有待核对成员、范围已失效或仓储失败时拒绝。
/// `scan` 来自当前任务；调用方须与最后一批目录结果共用事务。
pub async fn finish_reconciliation(
    db: &Database,
    scan: &CatalogMembershipScan,
    executor: &mut dyn Executor,
) -> Result<()> {
    let existing = db
        .supplier_catalog_membership_scans()
        .find_by_id(&scan.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原目录池扫描证据不存在".into()))?;
    existing.validate_binding(&scan.connection_id, &scan.supplier_id, &scan.scope)?;
    if existing.state == CatalogMembershipState::Reconciled {
        return Ok(());
    }

    let mut current = current_scan(db, scan, executor).await?;
    let remaining = mongo_ops::find_many(
        &db.collection::<CatalogPoolMember>(MEMBERS),
        stale_filter(&current),
        FindOptions::builder().limit(1).build(),
        executor,
    )
    .await?;
    if !remaining.is_empty() {
        return Err(Error::ConflictError("目录池仍有旧成员未完成核对".into()));
    }

    current.finish_reconciliation()?;
    db.supplier_catalog_membership_scans().update(&mut current, executor).await?;
    Ok(())
}

/// 中止确定失败的本地扫描，保留正向观察并释放围栏。
/// 已存在扫描转为待核实；没有扫描时不写入。
/// 数据库读取或并发更新失败时返回错误。
/// `job` 为当前已结算失败任务；正常发布后恢复的原任务不得调用。
pub async fn abort_scan(db: &Database, job: &str, executor: &mut dyn Executor) -> Result<()> {
    let Some(mut scan) = db.supplier_catalog_membership_scans().find_by_id(job, executor).await? else {
        return Ok(());
    };
    if scan.state == CatalogMembershipState::Reconciled
        || (scan.state == CatalogMembershipState::Uncertain && !scan.active)
    {
        return Ok(());
    }
    scan.abort();
    db.supplier_catalog_membership_scans().update(&mut scan, executor).await?;
    Ok(())
}

async fn current_scan(
    db: &Database,
    expected: &CatalogMembershipScan,
    executor: &mut dyn Executor,
) -> Result<CatalogMembershipScan> {
    let scan = db
        .supplier_catalog_membership_scans()
        .find_by_id(&expected.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原目录池扫描证据不存在".into()))?;
    scan.validate_binding(&expected.connection_id, &expected.supplier_id, &expected.scope)?;
    if !scan.can_reconcile() {
        return Err(Error::ConflictError("原目录池完整范围资格已失效".into()));
    }
    Ok(scan)
}

fn stale_filter(scan: &CatalogMembershipScan) -> Document {
    doc! {"connection_id":&scan.connection_id,"supplier_id":&scan.supplier_id,"account_scope":&scan.scope.account_scope,
    "pool_id":{"$in":&scan.scope.pool_ids},"active":true,"last_seen_run":{"$ne":&scan.base.id},"deleted_at":0_i64}
}

/// 由订货规格反查当前连接的来源目录，避免依赖未知 SPU 分组。
/// 最多两条候选；超过一条时调用方必须阻断身份不明的负向应用。
/// 身份为空或数据库查询失败时拒绝。
pub async fn catalog_items_for_spec(
    db: &Database,
    connection: &str,
    spec_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCatalogItem>> {
    if connection.trim().is_empty() || spec_id.trim().is_empty() {
        return Err(Error::ValidationError("目录成员的连接或订货规格缺失".into()));
    }
    Ok(mongo_ops::find_many(
        &db.collection::<SupplierCatalogItem>(ITEMS),
        doc! {"connection_id":connection,"spec_id":spec_id,"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(2).build(),
        executor,
    )
    .await?)
}
