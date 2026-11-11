//! 组合根只装配业务事实；权限检查及查询编译统一交给身份域。
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_supply::ports::FulfillmentExceptionHandlerPort;
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use erp_supply::{FulfillmentOrderAccess, FulfillmentOrderDataScopePort, FulfillmentOrderResolvedScope};
use erp_workflow::repository::prelude::*;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemFilter, WorkItemRow, WorkItemStatus, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support;
map_identity_error!(erp_supply);

#[derive(Clone)]
pub struct MongoFulfillmentOrderDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoFulfillmentOrderDataScope {
    /// 绑定身份设施。
    /// # 参数
    /// db、rbac 为应用共享设施。
    /// # 返回
    /// 不执行I/O的adapter。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 包装为领域授权端口。
    /// # 参数
    /// db、rbac 为共享身份设施。
    /// # 返回
    /// 已装配端口。
    /// # 错误
    /// 不执行I/O。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn FulfillmentOrderDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl FulfillmentOrderDataScopePort for MongoFulfillmentOrderDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<FulfillmentOrderResolvedScope> {
        erp_supply::authorization::resolve(
            &self.db,
            self.rbac.clone(),
            actor,
            "supplier_fulfillment_order",
            action,
            executor,
        )
        .await
        .map_err(map_identity_error)
    }
    async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<()> {
        erp_supply::authorization::require(
            &self.db,
            self.rbac.clone(),
            actor,
            "supplier_fulfillment_order",
            action,
            id,
            executor,
        )
        .await
        .map_err(map_identity_error)
    }
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<BTreeSet<String>> {
        scope_support::expand_org_units(
            &self.db,
            org_unit_ids,
            include_descendants,
            executor,
            Into::into,
            map_identity_error,
        )
        .await
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}
/// 当前开放 W26 异常处理人 adapter。
#[derive(Clone)]
pub struct MongoFulfillmentExceptionHandlers {
    db: Database,
}

impl MongoFulfillmentExceptionHandlers {
    /// 绑定工作项集合，构造时不查询。
    ///
    /// # 参数
    /// * `db` - 工作项集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为履约域可注入的异常处理人 Port。
    ///
    /// # 参数
    /// * `db` - 工作项集合所在数据库。
    ///
    /// # 返回
    /// 返回共享的异常处理人 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn FulfillmentExceptionHandlerPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FulfillmentExceptionHandlerPort for MongoFulfillmentExceptionHandlers {
    async fn open_handler_user_ids(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<HashMap<String, String>> {
        let mut handlers = HashMap::new();
        for order_id in order_ids {
            if let Some(owner) = open_w26_owner(&self.db, order_id, executor).await? {
                handlers.entry(order_id.clone()).or_insert(owner);
            }
        }
        Ok(handlers)
    }

    async fn order_ids_for_handlers(
        &self,
        handler_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Vec<String>> {
        if handler_user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut ids = scan_open_w26_order_ids(&self.db, handler_user_ids, executor).await?;
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

async fn open_w26_owner(
    db: &Database,
    order_id: &str,
    executor: &mut dyn Executor,
) -> erp_supply::Result<Option<String>> {
    let items = db.work_items().list_active_by_object(W26_BUSINESS_OBJECT_TYPE, order_id, executor).await?;
    Ok(items.into_iter().find_map(w26_owner))
}

fn w26_owner(item: WorkItem) -> Option<String> {
    if !matches!(
        item.work_item_type,
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
    ) {
        return None;
    }
    item.owner_user_id.filter(|id| !id.is_empty())
}

/// 按每批 100 条扫描开放 W26 工作项并收集订单编号。
///
/// # Panics
/// 批次大小常量 `100` 不能构成 `NonZeroU32` 时 panic。
async fn scan_open_w26_order_ids(
    db: &Database,
    handler_user_ids: &[String],
    executor: &mut dyn Executor,
) -> erp_supply::Result<Vec<String>> {
    let filter = w26_handler_filter(handler_user_ids);
    let batch = std::num::NonZeroU32::new(100).expect("W26 扫描批次");
    let mut offset = 0;
    let mut ids = Vec::new();
    loop {
        let rows = db.work_items().scan_work_item_batch(&filter, offset, batch, executor).await?;
        let count = rows.len() as u64;
        ids.extend(rows.into_iter().filter_map(w26_row_order_id));
        if count < u64::from(batch.get()) {
            break;
        }
        offset = offset.saturating_add(count);
    }
    Ok(ids)
}

fn w26_handler_filter(handler_user_ids: &[String]) -> WorkItemFilter {
    WorkItemFilter {
        work_item_types: vec![WorkItemType::IntegrationResultUnknown, WorkItemType::BusinessException],
        statuses: vec![WorkItemStatus::Open],
        managed_owner_ids: Some(handler_user_ids.to_vec()),
        object_access_shapes: Some(vec![
            (WorkItemType::IntegrationResultUnknown, W26_BUSINESS_OBJECT_TYPE.to_string()),
            (WorkItemType::BusinessException, W26_BUSINESS_OBJECT_TYPE.to_string()),
        ]),
        page: 1,
        page_size: 100,
        sort_by: Some("created_at".into()),
        sort_ascending: true,
        ..WorkItemFilter::default()
    }
}

fn w26_row_order_id(row: WorkItemRow) -> Option<String> {
    (row.business_object_type == W26_BUSINESS_OBJECT_TYPE)
        .then_some(row.business_object_id)
        .filter(|id| !id.is_empty())
}

/// 构造绑定身份数据库的履约订单访问器。
///
/// # 参数
/// * `db` - 履约与身份集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回已注入本 adapter 的履约订单访问器。
///
/// # 错误
/// 不返回错误。
pub fn fulfillment_order_access(db: Database, rbac: SharedRbacService) -> FulfillmentOrderAccess {
    FulfillmentOrderAccess::new(db.clone(), MongoFulfillmentOrderDataScope::shared(db, rbac))
}

/// 构造已接入身份域公共解析器与 W26 处理人 Port 的履约服务。
///
/// # 参数
/// * `db` - 履约与身份集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回已注入范围与异常处理人 adapter 的履约服务。
///
/// # 错误
/// 不返回错误。
pub fn scoped_fulfillment_service(db: Database, rbac: SharedRbacService) -> SupplierFulfillmentService {
    SupplierFulfillmentService::new(db.clone()).with_scope(
        MongoFulfillmentOrderDataScope::shared(db.clone(), rbac),
        MongoFulfillmentExceptionHandlers::shared(db),
    )
}
