//! 内部门户供给影响视图：真实采购选源、当前负责人及可读开放履约任务。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::ids::SupplierOfferingId;
use erp_identity::repository::{AccessControlExt, AccountCoreRepositoryExt};
use erp_procurement::PurchaseAccess;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_order::PurchaseOfferingUsageFact;
use erp_supply::repository::{SupplierOfferingAvailabilityRepositoryExt, SupplierOfferingExt};
use erp_workflow::WorkflowAuthorizationPort;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::repository::impacts::actual_open_tasks;
use super::repository::query::PortalQuery;
use super::warnings::current_warning;
use super::{PortalListParams, PortalReadAuthorizationPort, SupplyInterruptionWarning};
use crate::workbench::WorkbenchReadService;
use crate::{Error, Result};

/// 内部可读的实际采购影响，禁止向门户账号返回。
#[derive(Debug, Serialize)]
pub struct PortalPurchaseImpactView {
    pub purchase_order_id: String,
    pub purchase_no: String,
    pub status: String,
    pub purchase_order_version: u64,
    pub owner_user_id: String,
    pub owner_name: Option<String>,
    pub lines: Vec<PortalImpactLine>,
    pub tasks: Vec<PortalImpactTask>,
}

/// 保留真实选源版本用于人工核验，不包含旧单金额或付款。
#[derive(Debug, Serialize)]
pub struct PortalImpactLine {
    pub line_id: String,
    pub offering_revision_id: String,
    pub selected_offering_version: u64,
    pub selected_revision_version: u64,
    pub selected_availability_version: u64,
}

/// 已独立证明工作项详情资格的当前开放任务。
#[derive(Debug, Serialize)]
pub struct PortalImpactTask {
    pub id: String,
    pub object_type: String,
    pub object_id: String,
    pub status: String,
    pub version: u64,
    pub owner_user_id: Option<String>,
}

/// 供给影响分页；缺失历史选源始终显式未知，不补猜测数量。
#[derive(Debug, Serialize)]
pub struct PortalOfferingImpactView {
    pub offering_id: String,
    pub warning: Option<SupplyInterruptionWarning>,
    pub items: Vec<PortalPurchaseImpactView>,
    pub total: i64,
    pub page: u64,
    pub page_size: u32,
    pub association_notice: String,
}

/// 根装配供给、采购与工作项的正式资格，读取全程共用执行器。
#[derive(Clone)]
pub struct PortalOfferingImpactReadService<A> {
    db: Database,
    purchase: PurchaseAccess,
    authorization: Arc<dyn PortalReadAuthorizationPort>,
    workflow: A,
}

impl<A: WorkflowAuthorizationPort> PortalOfferingImpactReadService<A> {
    /// 装配领域对象范围与当前工作项授权，不缓存岗位或责任人。
    /// # 参数
    /// `db` 为数据库；`purchase` 为真实采购范围；`authorization` 为供给资格；`workflow` 为真实任务授权。
    /// # 返回
    /// 内部供给影响读取器。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        purchase: PurchaseAccess,
        authorization: Arc<dyn PortalReadAuthorizationPort>,
        workflow: A,
    ) -> Self {
        Self { db, purchase, authorization, workflow }
    }

    /// 查询精确供给关联、范围内未完成采购及真实当前履约任务。
    /// # 参数
    /// `actor` 为内部账号；`offering_id` 为服务器解析对象；`params` 为有界分页与采购单号搜索。
    /// # 返回
    /// 当前缺货/停供提示及可读采购页，不修改任何正式事实。
    /// # 错误
    /// 专项与对象资格未满足、来源损坏、读取超限或基础设施失败时拒绝。
    pub async fn offering_impacts(
        &self,
        actor: &AuditActor,
        offering_id: &str,
        params: &PortalListParams,
    ) -> Result<PortalOfferingImpactView> {
        if actor.kind() != AccountKind::Admin {
            return Err(Error::Forbidden("仅内部人员可以查看采购影响".into()));
        }
        if params.status.as_deref().is_some_and(|status| !status.trim().is_empty()) {
            return Err(Error::ValidationError("采购影响不接受申请状态筛选".into()));
        }
        let query = PortalQuery::new(params)?;
        let this = self.clone();
        let actor = actor.clone();
        let id = offering_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.read_with(&actor, &id, &query, executor).await })
            })
            .await
    }

    /// 同事务快照证明供给、采购范围、真实关联和当前任务。
    async fn read_with(
        &self,
        actor: &AuditActor,
        id: &str,
        query: &PortalQuery,
        executor: &mut dyn Executor,
    ) -> Result<PortalOfferingImpactView> {
        if !self.authorization.offering_readable(actor, id, executor).await? {
            return Err(Error::NotFound("供给不存在或无权查看".into()));
        }
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供给不存在或无权查看".into()))?;
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&SupplierOfferingId::new(id), executor)
            .await?;
        let mut visible = self.visible_usage(actor, id, &offering.supplier_id, query, executor).await?;
        visible.sort_by(|a, b| {
            (&a.order.purchase_no, &a.order.base.id).cmp(&(&b.order.purchase_no, &b.order.base.id))
        });
        let total = i64::try_from(visible.len()).map_err(|_| Error::Internal("采购影响数量越界".into()))?;
        let skip = usize::try_from(query.skip).map_err(|_| Error::ValidationError("分页范围过大".into()))?;
        let page = visible
            .into_iter()
            .skip(skip)
            .take(usize::try_from(query.page_size).unwrap_or(100))
            .collect::<Vec<_>>();
        let items = self.project_page(actor, page, executor).await?;
        Ok(PortalOfferingImpactView {
            offering_id: id.into(),
            warning: current_warning(id, Some(&offering), availability.as_ref()),
            items,
            total,
            page: query.page,
            page_size: query.page_size,
            association_notice:
                "仅列出当前采购行记录的正式供给选源；历史未记录选源的关联未知，不按供应商或SKU推断。".into(),
        })
    }

    /// 使用正式采购范围过滤精确供给关系，供应商归属损坏时失败关闭。
    async fn visible_usage(
        &self,
        actor: &AuditActor,
        id: &str,
        supplier_id: &erp_core::ids::SupplierAccountId,
        query: &PortalQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOfferingUsageFact>> {
        let (access, scope) = self
            .purchase
            .resolve_permissions(actor, "detail", &["purchase_order:list".into()], executor)
            .await?;
        let facts = self
            .db
            .purchase_order()
            .find_current_offering_usage(&SupplierOfferingId::new(id), &scope, 10_000, executor)
            .await?;
        let mut visible = Vec::new();
        for fact in facts {
            if fact.order.supplier_id != *supplier_id {
                return Err(Error::Internal("采购正式选源的供应商与供给归属不符".into()));
            }
            if self.purchase.allows(&access, &scope, &fact.order)?
                && query
                    .q
                    .as_ref()
                    .is_none_or(|q| fact.order.purchase_no.to_lowercase().contains(&q.to_lowercase()))
            {
                visible.push(fact);
            }
        }
        Ok(visible)
    }

    /// 当前页批量补真实采购负责人名称，并独立过滤任务读取资格。
    async fn project_page(
        &self,
        actor: &AuditActor,
        facts: Vec<PurchaseOfferingUsageFact>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PortalPurchaseImpactView>> {
        let owner_ids = facts
            .iter()
            .map(|fact| fact.order.current_owner_user_id().map(str::to_string))
            .collect::<erp_core::Result<Vec<_>>>()?;
        let names = self.db.accounts().names_by_ids(&owner_ids, executor).await?;
        let workbench = WorkbenchReadService::new(self.db.clone(), self.workflow.clone());
        let mut result = Vec::new();
        for fact in facts {
            let mut tasks = Vec::new();
            for task in actual_open_tasks(&self.db, &fact.order, executor).await? {
                if workbench.work_item_readable_with_executor(actor, &task.base.id, executor).await? {
                    tasks.push(PortalImpactTask {
                        id: task.base.id,
                        object_type: task.business_object_type,
                        object_id: task.business_object_id,
                        status: task.status.as_str().into(),
                        version: task.base.version,
                        owner_user_id: task.owner_user_id,
                    });
                }
            }
            result.push(purchase_view(fact, &names, tasks)?);
        }
        Ok(result)
    }
}

/// 投影允许的关联与责任字段，正式采购实体不进入响应。
fn purchase_view(
    fact: PurchaseOfferingUsageFact,
    names: &HashMap<String, String>,
    tasks: Vec<PortalImpactTask>,
) -> Result<PortalPurchaseImpactView> {
    let owner = fact.order.current_owner_user_id()?.to_string();
    let lines = fact
        .lines
        .into_iter()
        .map(|line| PortalImpactLine {
            line_id: line.line_id,
            offering_revision_id: line.source.supplier_offering_revision_id.to_string(),
            selected_offering_version: line.source.offering_version,
            selected_revision_version: line.source.revision_version,
            selected_availability_version: line.source.availability_version,
        })
        .collect();
    Ok(PortalPurchaseImpactView {
        purchase_order_id: fact.order.base.id,
        purchase_no: fact.order.purchase_no,
        status: fact.order.stable.status.as_str().into(),
        purchase_order_version: fact.order.base.version,
        owner_name: names.get(&owner).cloned(),
        owner_user_id: owner,
        lines,
        tasks,
    })
}
