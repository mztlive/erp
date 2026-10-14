//! 销售单附加费用的财务凭证登记与冲减（§7.5.1）。
//!
//! 每个命令一个事务：在同一事务内重验销售单对象范围、销售单与归属明细资格、
//! 登记凭证文件、写入成本事实与分配并写审计。成本事实主键由操作人、动作和幂等键
//! 派生，重试命中原事实；冲减来源行按序号编排，并发冲减由成本唯一索引串行化。

pub mod sales_line;

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CostEntryId, FileAssetId, SalesOrderId, SupplierAccountId};
use erp_finance::dto::extra_cost::{
    ExtraCostCommandResult, ReduceExtraCostRequest, RegisterExtraCostRequest,
};
use erp_finance::entity::cost::{
    CostAllocation, CostEntry, ExtraCostReduction, ExtraCostRegistration, ReducedAmounts,
    SALES_EXTRA_COST_FACT, SALES_EXTRA_COST_REDUCTION_FACT, extra_cost_entry_id, is_registered_extra_cost,
};
use erp_finance::repository::CostExt;
use erp_finance::repository::cost::CostAllocationRepositoryExt;
use erp_finance::service::cost::{PreparedCostEntry, persist_cost_entry};
use erp_identity::SharedRbacService;
use erp_read_models::finance::extra_cost::order_costs;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use erp_supplier::SupplierExt;
use erp_support::{PendingAttachmentBatch, PendingFileAssetRequest};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use self::sales_line::{ensure_attributable_line, ensure_extra_cost_order};
use crate::attachments::PendingFileAssets;
use crate::audit::{persist_log, recover_command};
use crate::{Error, Result};

const REGISTER_ACTION: &str = "sales_extra_cost.register";
const REDUCE_ACTION: &str = "sales_extra_cost.reduce";

/// 附加费用登记与冲减流程。
pub struct SalesExtraCostProcess {
    db: Database,
    rbac: SharedRbacService,
}

impl SalesExtraCostProcess {
    /// 绑定数据库与当前授权服务。
    ///
    /// # 参数
    /// * `db` - 应用数据库
    /// * `rbac` - 当前 RBAC 服务，用于事务内重验销售单对象范围
    ///
    /// # 返回
    /// 返回流程实例，构造不执行 I/O。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 财务按凭证登记一笔附加费用，直接形成实际发生成本。
    ///
    /// # 参数
    /// * `sales_order_id` - 归属销售单
    /// * `req` - 登记命令；凭证须为本次上传文件
    /// * `assets` - 本次已写入对象存储的凭证文件
    /// * `actor` - 已通过 `cost_entry:create` 鉴权的操作人
    ///
    /// # 返回
    /// 返回成本事实主键；同一幂等键同一载荷重放时返回原事实。
    ///
    /// # 错误
    /// 输入非法、凭证缺失、销售单不可见或不可登记、归属明细或供应商无效、
    /// 幂等键已用于不同登记、写入失败时返回错误。
    pub async fn register(
        &self,
        sales_order_id: &str,
        mut req: RegisterExtraCostRequest,
        assets: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ExtraCostCommandResult> {
        req.validate()?;
        let pending = PendingFileAssets::prepare(assets, actor)?.shared();
        resolve_evidence(&mut req.evidence_attachment_id, pending.as_ref())?;
        let registration = ExtraCostRegistration::new(req.input(), BusinessDate::today())?;
        let entry_id = extra_cost_entry_id(actor.id(), REGISTER_ACTION, &req.idempotency_key);
        let order_id = SalesOrderId::new(sales_order_id.to_string());
        let expected = Expected::registration(&req, &order_id);
        if let Some(replayed) = replay(&self.db, &entry_id, &expected, &mut NoTransaction).await? {
            return Ok(replayed);
        }
        let command =
            Command::Register(RegisterCommand { order_id, entry_id: entry_id.clone(), registration });
        match self.transact(actor, pending, command).await {
            Ok(()) => Ok(ExtraCostCommandResult { cost_entry_id: entry_id.to_string(), replayed: false }),
            Err(error) => {
                recover_command(error, replay(&self.db, &entry_id, &expected, &mut NoTransaction).await)
            },
        }
    }

    /// 冲减一笔财务登记的附加费用；累计冲减不得超过原费用。
    ///
    /// # 参数
    /// * `sales_order_id` - 原费用归属销售单
    /// * `cost_entry_id` - 原附加费用成本事实
    /// * `req` - 冲减命令；凭证须为本次上传文件
    /// * `assets` - 本次已写入对象存储的凭证文件
    /// * `actor` - 已通过 `cost_entry:create` 鉴权的操作人
    ///
    /// # 返回
    /// 返回冲减成本事实主键；同一幂等键同一载荷重放时返回原事实。
    ///
    /// # 错误
    /// 原费用不存在或不属于该销售单、不是财务登记费用、冲减超额、并发冲减冲突、
    /// 幂等键已用于不同冲减或写入失败时返回错误。
    pub async fn reduce(
        &self,
        sales_order_id: &str,
        cost_entry_id: &str,
        mut req: ReduceExtraCostRequest,
        assets: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ExtraCostCommandResult> {
        req.validate()?;
        let pending = PendingFileAssets::prepare(assets, actor)?.shared();
        resolve_evidence(&mut req.evidence_attachment_id, pending.as_ref())?;
        let reduction = ExtraCostReduction::new(req.input(), BusinessDate::today())?;
        let entry_id = extra_cost_entry_id(actor.id(), REDUCE_ACTION, &req.idempotency_key);
        let order_id = SalesOrderId::new(sales_order_id.to_string());
        let original_id = CostEntryId::new(cost_entry_id.to_string());
        let expected = Expected::reduction(&req, &order_id, &original_id);
        if let Some(replayed) = replay(&self.db, &entry_id, &expected, &mut NoTransaction).await? {
            return Ok(replayed);
        }
        let command =
            Command::Reduce(ReduceCommand { order_id, original_id, entry_id: entry_id.clone(), reduction });
        match self.transact(actor, pending, command).await {
            Ok(()) => Ok(ExtraCostCommandResult { cost_entry_id: entry_id.to_string(), replayed: false }),
            Err(error) => {
                recover_command(error, replay(&self.db, &entry_id, &expected, &mut NoTransaction).await)
            },
        }
    }

    /// 在一个事务内执行命令；唯一来源冲突沿用统一冲突分类。
    async fn transact(
        &self,
        actor: &AuditActor,
        pending: Arc<dyn PendingAttachmentBatch>,
        command: Command,
    ) -> Result<()> {
        let db = self.db.clone();
        let access = SalesAccess::new(self.db.clone(), self.rbac.clone());
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { command.apply(&db, &access, &actor, pending.as_ref(), executor).await })
            })
            .await
    }
}

/// 事务内执行的附加费用命令。
enum Command {
    Register(RegisterCommand),
    Reduce(ReduceCommand),
}

impl Command {
    async fn apply(
        &self,
        db: &Database,
        access: &SalesAccess,
        actor: &AuditActor,
        pending: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match self {
            Self::Register(command) => command.apply(db, access, actor, pending, executor).await,
            Self::Reduce(command) => command.apply(db, access, actor, pending, executor).await,
        }
    }
}

/// 财务登记命令。
struct RegisterCommand {
    order_id: SalesOrderId,
    entry_id: CostEntryId,
    registration: ExtraCostRegistration,
}

impl RegisterCommand {
    async fn apply(
        &self,
        db: &Database,
        access: &SalesAccess,
        actor: &AuditActor,
        pending: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let order = load_order(access, actor, &self.order_id, executor).await?;
        ensure_attributable_line(db, &order, self.registration.sales_order_line_id(), executor).await?;
        ensure_supplier(db, self.registration.supplier_id(), executor).await?;
        let (entry, allocation) =
            self.registration.clone().into_facts(self.entry_id.clone(), self.order_id.clone())?;
        write(db, entry, allocation, REGISTER_ACTION, actor, pending, executor).await
    }
}

/// 冲减命令。
struct ReduceCommand {
    order_id: SalesOrderId,
    original_id: CostEntryId,
    entry_id: CostEntryId,
    reduction: ExtraCostReduction,
}

impl ReduceCommand {
    async fn apply(
        &self,
        db: &Database,
        access: &SalesAccess,
        actor: &AuditActor,
        pending: &dyn PendingAttachmentBatch,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        load_order(access, actor, &self.order_id, executor).await?;
        let (original, allocation) = load_original(db, &self.order_id, &self.original_id, executor).await?;
        let (entries, _) = order_costs(db, self.order_id.as_ref(), executor).await?;
        let reduced = ReducedAmounts::of(&original, entries.iter());
        let (entry, allocation) =
            self.reduction.clone().into_facts(self.entry_id.clone(), &original, &allocation, reduced)?;
        write(db, entry, allocation, REDUCE_ACTION, actor, pending, executor).await
    }
}

/// 读取当前操作人可见且可登记附加费用的销售单。
async fn load_order(
    access: &SalesAccess,
    actor: &AuditActor,
    order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<SalesOrder> {
    let order = access.require_object(actor, "detail", order_id.as_ref(), &[], executor).await?;
    ensure_extra_cost_order(&order)?;
    Ok(order)
}

/// 读取属于本销售单的财务登记附加费用及其唯一分配。
async fn load_original(
    db: &Database,
    order_id: &SalesOrderId,
    original_id: &CostEntryId,
    executor: &mut dyn Executor,
) -> Result<(CostEntry, CostAllocation)> {
    let original = db
        .cost_entries()
        .find_by_id(original_id.as_ref(), executor)
        .await?
        .filter(|entry| is_registered_extra_cost(entry) && entry.source_document_id == order_id.as_ref())
        .ok_or_else(|| Error::NotFound("附加费用不存在".into()))?;
    let allocation = db
        .cost_allocations()
        .find_allocations_by_entries(std::slice::from_ref(original_id), executor)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Internal("附加费用缺少销售单分配".into()))?;
    Ok((original, allocation))
}

/// 选填的收款供应商必须存在。
async fn ensure_supplier(
    db: &Database,
    supplier_id: Option<&SupplierAccountId>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(supplier_id) = supplier_id else {
        return Ok(());
    };
    db.supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ValidationError("收款供应商不存在".into()))?;
    Ok(())
}

/// 登记凭证、写入成本事实与分配并写审计。
async fn write(
    db: &Database,
    entry: CostEntry,
    allocation: CostAllocation,
    action: &str,
    actor: &AuditActor,
    pending: &dyn PendingAttachmentBatch,
    executor: &mut dyn Executor,
) -> Result<()> {
    let audit = actor.clone().resource_log(action, "cost_entry", entry.base.id.clone())?;
    pending.persist(db, executor).await?;
    persist_cost_entry(db, PreparedCostEntry { entry, allocations: vec![allocation] }, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 把凭证临时引用替换为正式资产；附加费用凭证必须随命令上传。
fn resolve_evidence(id: &mut FileAssetId, pending: &dyn PendingAttachmentBatch) -> Result<()> {
    let mut used = HashSet::new();
    if !pending.resolve_id(id, &mut used)? {
        return Err(Error::ValidationError("请上传费用凭证".into()));
    }
    pending.ensure_all_used(&used)?;
    Ok(())
}

/// 重放比对所需的原命令关键事实。
struct Expected {
    fact_type: &'static str,
    order_id: String,
    adjusts: Option<String>,
    gross: erp_core::money::Amount,
    description: String,
}

impl Expected {
    fn registration(req: &RegisterExtraCostRequest, order_id: &SalesOrderId) -> Self {
        Self {
            fact_type: SALES_EXTRA_COST_FACT,
            order_id: order_id.to_string(),
            adjusts: None,
            gross: req.gross_amount,
            description: req.description.trim().to_string(),
        }
    }

    fn reduction(req: &ReduceExtraCostRequest, order_id: &SalesOrderId, original: &CostEntryId) -> Self {
        Self {
            fact_type: SALES_EXTRA_COST_REDUCTION_FACT,
            order_id: order_id.to_string(),
            adjusts: Some(original.to_string()),
            gross: req.gross_amount,
            description: req.description.trim().to_string(),
        }
    }

    fn matches(&self, entry: &CostEntry) -> bool {
        entry.source_fact_type == self.fact_type
            && entry.source_document_id == self.order_id
            && entry.adjusts_cost_entry_id.as_ref().map(ToString::to_string) == self.adjusts
            && entry.gross_amount == self.gross
            && entry.description.as_deref() == Some(self.description.as_str())
    }
}

/// 同一幂等键已写入时按原载荷重放；载荷不同则拒绝。
async fn replay(
    db: &Database,
    entry_id: &CostEntryId,
    expected: &Expected,
    executor: &mut dyn Executor,
) -> Result<Option<ExtraCostCommandResult>> {
    let Some(entry) = db.cost_entries().find_by_id(entry_id.as_ref(), executor).await? else {
        return Ok(None);
    };
    if !expected.matches(&entry) {
        return Err(Error::ConflictError("幂等键已用于不同的附加费用命令".into()));
    }
    Ok(Some(ExtraCostCommandResult { cost_entry_id: entry.base.id, replayed: true }))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::Amount;
    use erp_finance::entity::cost::{CostEntryData, CostScope, CostStage, CostType};

    use super::*;

    fn entry(fact: &str, adjusts: Option<&str>, description: &str) -> CostEntry {
        CostEntry::new(
            CostEntryId::new("xc-1"),
            CostEntryData {
                cost_type: CostType::Logistics,
                cost_stage: if adjusts.is_some() { CostStage::Reduction } else { CostStage::Actual },
                cost_scope: CostScope::NonVoucherFulfillment,
                cost_basis: None,
                supplier_id: None,
                gross_amount: Amount::from_str("10.00").unwrap(),
                net_amount: Amount::from_str("10.00").unwrap(),
                tax_amount: Amount::zero(),
                tax_inclusion: true,
                input_tax_rate: erp_core::money::Rate::from_str("0").unwrap(),
                occurred_at: erp_core::common::time::Instant::from_unix_secs(1),
                source_fact_type: fact.into(),
                source_document_id: "so-1".into(),
                source_line_id: "xc-1".into(),
                source_version: "1".into(),
                adjusts_cost_entry_id: adjusts.map(CostEntryId::new),
                evidence_attachment_id: None,
                description: Some(description.into()),
            },
        )
        .unwrap()
    }

    fn expected(fact: &'static str, adjusts: Option<&str>) -> Expected {
        Expected {
            fact_type: fact,
            order_id: "so-1".into(),
            adjusts: adjusts.map(Into::into),
            gross: Amount::from_str("10.00").unwrap(),
            description: "运费".into(),
        }
    }

    #[test]
    fn replay_matches_only_same_command_payload() {
        let registered = entry(SALES_EXTRA_COST_FACT, None, "运费");
        assert!(expected(SALES_EXTRA_COST_FACT, None).matches(&registered));
        assert!(!expected(SALES_EXTRA_COST_REDUCTION_FACT, Some("xc-0")).matches(&registered));
        let other_description = entry(SALES_EXTRA_COST_FACT, None, "包装");
        assert!(!expected(SALES_EXTRA_COST_FACT, None).matches(&other_description));
        let reduction = entry(SALES_EXTRA_COST_REDUCTION_FACT, Some("xc-0"), "运费");
        assert!(expected(SALES_EXTRA_COST_REDUCTION_FACT, Some("xc-0")).matches(&reduction));
        assert!(!expected(SALES_EXTRA_COST_REDUCTION_FACT, Some("xc-9")).matches(&reduction));
    }
}
