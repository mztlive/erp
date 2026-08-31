//! 域 D28 `card_instance` 服务编排（W28 卡券消费台账与上线切换）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 切换记录、余额快照的单一集合 CRUD 无跨步骤原子性 → `&mut NoTransaction`
//!   （审计日志按既有写法独立写入）；
//! - 卡实例基线 + 初始余额快照跨集合 → `database::Transactional::with_transaction`；
//! - 启用切换（§8.4 第 8 条）锁定切换记录并原子写唯一 `T` → 事务 + 审计。
//!
//! 跨域协作只调对方 Repository（P3-service-api.md §2）：本域经
//! `SalesOrderExt::sales_orders`（D13）校验原销售单存在；不依赖任何 Service。

use database::{
    AccessControlExt, CardBaselineRegistration, CardInstanceExt, NoTransaction, SalesOrderExt, Transactional,
};
use entities::card_instance::{
    CutoverEnableResult, MallBalanceSnapshot, MallBalanceSnapshotData, MallCardBaselineAggregate,
    MallCardInstance, MallCardInstanceData, MallConsumptionCutover, MallConsumptionCutoverData,
};
use entities::ids::{MallBalanceSnapshotId, MallCardInstanceId, MallConsumptionCutoverId};
use entities::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    BalanceSnapshotListParams, BalanceSnapshotView, CardInstanceDetailView, CardInstanceListParams,
    CardInstanceView, CorrectionListParams, CorrectionView, CreateBalanceSnapshotRequest,
    CreateCardInstanceRequest, CreateCutoverRequest, CutoverListParams, CutoverView, EnableCutoverRequest,
    PageView,
};

/// 切换记录列表筛选条件类型（经 `CardInstanceExt` 关联类型跨 crate 可达）。
type CutoverFilter = <mongodb::Database as CardInstanceExt>::MallConsumptionCutoverFilter;
/// 卡实例列表筛选条件类型。
type CardInstanceFilter = <mongodb::Database as CardInstanceExt>::MallCardInstanceFilter;

/// 卡实例域服务：切换管理、卡实例基线与余额快照、纠错查询。
pub struct CardInstanceService {
    db: Database,
}

impl CardInstanceService {
    /// 创建服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询切换记录列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn cutover_list(&self, params: &CutoverListParams) -> Result<PageView<CutoverView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CutoverFilter {
            mall_id: query.mall_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_consumption_cutovers()
            .search_cutovers(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CutoverView {
                id: row.id,
                mall_id: row.mall_id,
                status: row.status,
                enabled_at: row.enabled_at.map(|instant| instant.unix_secs() as u64),
                enabled_by: row.enabled_by,
                checklist_reference: None,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建切换记录（准备态，未启用）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建切换记录视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_cutover(&self, req: CreateCutoverRequest, actor: &AuditActor) -> Result<CutoverView> {
        req.validate()?;
        let id = MallConsumptionCutoverId::new(next_id());
        let mut cutover = MallConsumptionCutover::new(
            id,
            MallConsumptionCutoverData {
                mall_id: req.mall_id,
                checklist_reference: req.checklist_reference,
            },
        )?;
        let audit = actor.clone().resource_log(
            "mall_consumption_cutover.create",
            "mall_consumption_cutover",
            cutover.base.id.clone(),
        )?;

        let cutover_for_tx = cutover.clone();
        crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.mall_consumption_cutovers()
                    .create(&cutover_for_tx, session)
                    .await?;
                Ok(())
            })
        })
        .await?;

        Ok(cutover_view(&mut cutover))
    }

    /// 启用切换（登记唯一 `T`，对应 §8.4 第 8 条）。
    ///
    /// 以乐观锁版本 CAS 更新；同一商城已存在启用 `T` 时拒绝；已启用且
    /// 启用时间一致时按幂等返回既有结果，不重复登记。
    ///
    /// # 参数
    /// * `id` - 切换记录 ID
    /// * `req` - 启用请求（期望版本 + `T`）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回启用后的切换记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 切换记录不存在
    /// * `ConflictError` - 期望版本过期或该商城已有启用 `T`
    /// * `ValidationError` - 请求体校验失败
    pub async fn enable_cutover(
        &self,
        id: &str,
        req: EnableCutoverRequest,
        actor: &AuditActor,
    ) -> Result<CutoverView> {
        req.validate()?;
        let enabled_at = entities::common::time::Instant::from_unix_secs(req.enabled_at as i64);
        let expected_version = req.version;
        let actor_id = actor.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        let audit = actor.clone().resource_log(
            "mall_consumption_cutover.enable",
            "mall_consumption_cutover",
            id.to_string(),
        )?;
        let id_owned = id.to_string();
        let mut cutover = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut cutover = db
                        .mall_consumption_cutovers()
                        .find_by_id(&id_owned, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("切换记录不存在".to_string()))?;
                    let outcome =
                        cutover.enable_idempotently(expected_version, enabled_at, actor_id.clone())?;
                    if outcome == CutoverEnableResult::AlreadySame {
                        db.audit_logs().create(&audit, session).await?;
                        return Ok(cutover);
                    }
                    if outcome == CutoverEnableResult::Conflict {
                        return Err(Error::ConflictError(
                            "数据已被其他请求修改或切换时间冲突，请刷新后重试".to_string(),
                        ));
                    }
                    if db
                        .mall_consumption_cutovers()
                        .find_enabled_cutover_by_mall_id(&cutover.mall_id, session)
                        .await?
                        .is_some()
                    {
                        return Err(Error::ConflictError("该商城已存在启用切换记录".to_string()));
                    }
                    db.mall_consumption_cutovers()
                        .update(&mut cutover, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(cutover)
                })
            })
            .await?;

        Ok(cutover_view(&mut cutover))
    }

    /// 查询切换记录详情。
    ///
    /// # 参数
    /// * `id` - 切换记录 ID
    ///
    /// # 返回
    /// 返回切换记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 切换记录不存在
    pub async fn cutover_detail(&self, id: &str) -> Result<CutoverView> {
        let mut cutover = self
            .db
            .mall_consumption_cutovers()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("切换记录不存在".to_string()))?;
        Ok(cutover_view(&mut cutover))
    }

    /// 分页查询卡实例列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_id`/`opaque_instance_ref`/`source_type` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn card_instance_list(
        &self,
        params: &CardInstanceListParams,
    ) -> Result<PageView<CardInstanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CardInstanceFilter {
            mall_id: query.mall_id,
            opaque_instance_ref: query.opaque_instance_ref,
            source_type: query.source_type,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_card_instances()
            .search_card_instances(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CardInstanceView {
                id: row.id,
                mall_id: row.mall_id,
                opaque_instance_ref: row.opaque_instance_ref,
                origin_sales_order_id: row.origin_sales_order_id.to_string(),
                origin_sales_order_revision_id: None,
                source_baseline_version: None,
                initial_balance: row.initial_balance.to_string(),
                baseline_at: row.baseline_at.unix_secs() as u64,
                source_type: row.source_type,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 建立卡实例基线（跨集合事务：基线 + 初始余额快照原子可见）。
    ///
    /// 同一 `(mall_id, opaque_instance_ref)` 重复基线完全一致时按幂等返回既有
    /// 结果，不一致时返回冲突；并发竞争由唯一索引透出 `DuplicateKey`。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或既有）卡实例视图。
    ///
    /// # 错误
    /// * `NotFound` - 原销售单不存在（经 D13 仓储校验）
    /// * `ConflictError` - 同一身份重复且内容不一致，或并发唯一冲突
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_card_instance(
        &self,
        req: CreateCardInstanceRequest,
        actor: &AuditActor,
    ) -> Result<CardInstanceView> {
        req.validate()?;
        // 跨域：经 D13 的 Repository 校验原销售单存在（P3-service-api.md §2）。
        let sales_order_id = req.origin_sales_order_id.clone();
        self.db
            .sales_orders()
            .find_by_id(&sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原销售单不存在".to_string()))?;

        let mall_id = req.mall_id.trim().to_string();
        let opaque_instance_ref = req.opaque_instance_ref.trim().to_string();
        let baseline_at = entities::common::time::Instant::from_unix_secs(req.baseline_at as i64);
        let aggregate = MallCardBaselineAggregate::new(
            MallCardInstanceId::new(next_id()),
            MallBalanceSnapshotId::new(next_id()),
            MallCardInstanceData {
                mall_id: mall_id.clone(),
                opaque_instance_ref: opaque_instance_ref.clone(),
                origin_sales_order_source_identity_id: req.origin_sales_order_source_identity_id,
                origin_sales_order_id: req.origin_sales_order_id,
                origin_sales_order_revision_id: req.origin_sales_order_revision_id,
                source_baseline_version: req.source_baseline_version,
                initial_balance: Amount::from_str(&req.initial_balance)?,
                baseline_at,
                source_type: req.source_type,
            },
        )?;
        let (instance, snapshot) = aggregate.into_parts();
        let audit = actor.clone().resource_log(
            "mall_card_instance.create",
            "mall_card_instance",
            instance.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let instance_for_tx = instance.clone();
        let registration = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let registration = db
                        .card_instance()
                        .create_card_instance_with_initial_snapshot(&instance_for_tx, &snapshot, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CardBaselineRegistration, database::Error>(registration)
                })
            })
            .await;

        match registration {
            Ok(CardBaselineRegistration::Created) => Ok(card_instance_view(&instance)),
            Ok(CardBaselineRegistration::ExistingSame(existing)) => Ok(card_instance_view(&existing)),
            Ok(CardBaselineRegistration::ExistingConflict(_)) => Err(Error::ConflictError(
                "同一卡实例存在冲突基线，请按纠错流程处理".to_string(),
            )),
            Err(database::Error::DuplicateKey(_)) => {
                // 唯一竞争已使上一个事务退出；必须使用新的非事务 executor 复核，
                // 禁止复用已失败 session。
                match self
                    .db
                    .card_instance()
                    .registration_by_identity(&instance, &mut NoTransaction)
                    .await?
                {
                    Some(CardBaselineRegistration::ExistingSame(existing)) => {
                        Ok(card_instance_view(&existing))
                    }
                    Some(CardBaselineRegistration::ExistingConflict(_)) => Err(Error::ConflictError(
                        "同一卡实例存在冲突基线，请按纠错流程处理".to_string(),
                    )),
                    Some(CardBaselineRegistration::Created) | None => Err(Error::ConflictError(
                        "卡实例唯一竞争结果已变化，请刷新后重试".to_string(),
                    )),
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    /// 查询卡实例详情（基线 + 最新余额 + 快照/纠错摘要）。
    ///
    /// # 参数
    /// * `id` - 卡实例 ID
    ///
    /// # 返回
    /// 返回卡实例详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 卡实例不存在
    pub async fn card_instance_detail(&self, id: &str) -> Result<CardInstanceDetailView> {
        let instance = self
            .db
            .mall_card_instances()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡实例不存在".to_string()))?;
        let card_id = MallCardInstanceId::new(instance.base.id.clone());
        let (latest_balance, balance_snapshot_count) = self
            .db
            .balance_snapshots()
            .latest_balance_and_count(&card_id, &mut NoTransaction)
            .await?;
        let correction_count = self
            .db
            .card_instance_corrections()
            .count_by_card(&card_id, &mut NoTransaction)
            .await?;

        Ok(CardInstanceDetailView {
            instance: card_instance_view(&instance),
            latest_balance: latest_balance.map(|balance| balance.to_string()),
            balance_snapshot_count,
            correction_count,
        })
    }

    /// 分页查询余额快照列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_card_instance_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn balance_snapshot_list(
        &self,
        mall_card_instance_id: &MallCardInstanceId,
        params: &BalanceSnapshotListParams,
    ) -> Result<PageView<BalanceSnapshotView>> {
        params.validate()?;
        let query = params.normalized()?;
        if !self
            .db
            .mall_card_instances()
            .exists_by_id(mall_card_instance_id, &mut NoTransaction)
            .await?
        {
            return Err(Error::NotFound("卡实例不存在".to_string()));
        }
        let page = self
            .db
            .balance_snapshots()
            .page_by_card(
                mall_card_instance_id,
                query.paging.page,
                query.paging.page_size,
                query.paging.sort_by,
                matches!(query.paging.sort_dir, SortDir::Asc),
                &mut NoTransaction,
            )
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|snapshot| BalanceSnapshotView {
                id: snapshot.base.id.clone(),
                mall_card_instance_id: snapshot.mall_card_instance_id.to_string(),
                snapshot_at: snapshot.snapshot_at.unix_secs() as u64,
                balance: snapshot.balance.to_string(),
                source_snapshot_version: snapshot.source_snapshot_version.clone(),
                source_event_id: snapshot.source_event_id.clone(),
                created_at: snapshot.base.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    /// 追加余额快照（商城余额快照回流，单集合写入）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建快照视图。
    ///
    /// # 错误
    /// * `NotFound` - 卡实例不存在
    /// * `ConflictError` - 同卡实例同快照时间重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_balance_snapshot(
        &self,
        req: CreateBalanceSnapshotRequest,
        actor: &AuditActor,
    ) -> Result<BalanceSnapshotView> {
        req.validate()?;
        let card_id = req.mall_card_instance_id.clone();
        self.db
            .mall_card_instances()
            .find_by_id(&card_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡实例不存在".to_string()))?;
        let snapshot = MallBalanceSnapshot::new(
            MallBalanceSnapshotId::new(next_id()),
            MallBalanceSnapshotData {
                mall_card_instance_id: req.mall_card_instance_id,
                snapshot_at: entities::common::time::Instant::from_unix_secs(req.snapshot_at as i64),
                balance: Amount::from_str(&req.balance)?,
                source_snapshot_version: req.source_snapshot_version,
                source_event_id: req.source_event_id,
            },
        )?;
        let audit = actor.clone().resource_log(
            "mall_balance_snapshot.create",
            "mall_balance_snapshot",
            snapshot.base.id.clone(),
        )?;

        let snapshot_for_tx = snapshot.clone();
        crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.balance_snapshots().create(&snapshot_for_tx, session).await?;
                Ok(())
            })
        })
        .await?;

        Ok(BalanceSnapshotView {
            id: snapshot.base.id,
            mall_card_instance_id: snapshot.mall_card_instance_id.to_string(),
            snapshot_at: snapshot.snapshot_at.unix_secs() as u64,
            balance: snapshot.balance.to_string(),
            source_snapshot_version: snapshot.source_snapshot_version,
            source_event_id: snapshot.source_event_id,
            created_at: snapshot.base.created_at,
        })
    }

    /// 分页查询卡实例纠错列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_card_instance_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn correction_list(
        &self,
        mall_card_instance_id: &MallCardInstanceId,
        params: &CorrectionListParams,
    ) -> Result<PageView<CorrectionView>> {
        params.validate()?;
        let query = params.normalized()?;
        if !self
            .db
            .mall_card_instances()
            .exists_by_id(mall_card_instance_id, &mut NoTransaction)
            .await?
        {
            return Err(Error::NotFound("卡实例不存在".to_string()));
        }
        let page = self
            .db
            .card_instance_corrections()
            .page_by_card(
                mall_card_instance_id,
                query.paging.page,
                query.paging.page_size,
                query.paging.sort_by,
                matches!(query.paging.sort_dir, SortDir::Asc),
                &mut NoTransaction,
            )
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|correction| CorrectionView {
                id: correction.base.id.clone(),
                mall_card_instance_id: correction.mall_card_instance_id.to_string(),
                correction_no: correction.correction_no,
                correction_type: correction.correction_type,
                before_value: correction.before_value.clone(),
                after_value: correction.after_value.clone(),
                reason: correction.reason.clone(),
                approved_by: correction.approved_by.clone(),
                approved_at: correction.approved_at.unix_secs() as u64,
                supersedes_correction_id: correction.supersedes_correction_id.map(|id| id.to_string()),
                created_at: correction.base.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }
}

/// 从实体构造切换记录视图。
///
/// # 参数
/// * `cutover` - 切换记录实体
///
/// # 返回
/// 返回响应视图。
fn cutover_view(cutover: &mut MallConsumptionCutover) -> CutoverView {
    CutoverView {
        id: cutover.base.id.clone(),
        mall_id: cutover.mall_id.clone(),
        status: cutover.status,
        enabled_at: cutover.enabled_at.map(|instant| instant.unix_secs() as u64),
        enabled_by: cutover.enabled_by.clone(),
        checklist_reference: cutover.checklist_reference.clone(),
        created_at: cutover.base.created_at,
        version: cutover.base.version,
    }
}

/// 从实体构造卡实例视图。
///
/// # 参数
/// * `instance` - 卡实例实体
///
/// # 返回
/// 返回响应视图。
fn card_instance_view(instance: &MallCardInstance) -> CardInstanceView {
    CardInstanceView {
        id: instance.base.id.clone(),
        mall_id: instance.mall_id.clone(),
        opaque_instance_ref: instance.opaque_instance_ref.clone(),
        origin_sales_order_id: instance.origin_sales_order_id.to_string(),
        origin_sales_order_revision_id: Some(instance.origin_sales_order_revision_id.to_string()),
        source_baseline_version: instance.source_baseline_version.clone(),
        initial_balance: instance.initial_balance.to_string(),
        baseline_at: instance.baseline_at.unix_secs() as u64,
        source_type: instance.source_type,
        created_at: instance.base.created_at,
        version: instance.base.version,
    }
}
