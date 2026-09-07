//! 域 D20 `cost` 根流程编排（页面：W16 实际经营盈亏）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 跨集合写入（成本事实 + 分配行）→
//!   `persistence_core::Transactional::with_transaction`；
//! - 列表查询单集合 → `&mut NoTransaction`。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D13 `sales_order()`
//! 按 ID 集合批量校验成本归属销售单存在（D20 依赖域 D15/D16/D13，本期 P3
//! 只落地 D13 校验与查询编排，D15/D16 的采购/履约来源由对方域在 P3 经
//! `CostExt` 直接写入）。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::CostEntryId;
use erp_finance::dto::cost::{CostEntryView, CreateCostEntryRequest};
use erp_finance::service::cost::{
    dedupe_order_ids, missing_order_id, persist_cost_entry_in_transaction, prepare_cost_entry,
    validate_create_cost_entry,
};
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use services::{Error, Result};
/// 手工成本登记的根流程服务。
pub struct CostService {
    db: Database,
}
impl CostService {
    /// 创建成本登记流程，复用入口数据库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 手工登记成本事实与分配行（跨集合事务写入）。
    ///
    /// 同一事务内：校验归属销售单存在（D13 Repository）；分配合计必须等于
    /// 成本事实金额（含税与不含税双侧）；写入成本事实与分配行，保证「事实 +
    /// 分配行」原子可见（数据模型 §6.10）。业务幂等唯一
    /// `(source_fact_type, source_document_id, source_line_id, source_version,
    /// cost_stage, cost_type)` 由唯一索引保证，重复提交落入 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建成本事实的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 归属销售单不存在
    /// * `ConflictError` - 业务唯一键重复
    /// * `Logic` - 分配合计与事实金额不一致
    pub async fn create_cost_entry(
        &self,
        req: CreateCostEntryRequest,
        actor: &AuditActor,
    ) -> Result<CostEntryView> {
        validate_create_cost_entry(&req)?;
        // 归属销售单存在性：先去重、一次批量读取存在性事实，再解释缺失订单；
        // Repository 只返回已存在 ID 的最小事实，跨聚合报错决策保留 Service。
        let requested_order_ids = req
            .allocations
            .iter()
            .map(|line| line.sales_order_id.clone())
            .collect::<Vec<_>>();
        let unique_order_ids = dedupe_order_ids(&requested_order_ids);
        let existing_order_ids = self
            .db
            .sales_order()
            .find_existing_ids(&unique_order_ids, &mut NoTransaction)
            .await?;
        if missing_order_id(&unique_order_ids, &existing_order_ids).is_some() {
            return Err(Error::NotFound("成本归属销售单不存在".to_string()));
        }
        let prepared = prepare_cost_entry(req)?;
        let entry_id = CostEntryId::new(prepared.entry.base.id.clone());
        let audit = actor
            .clone()
            .resource_log("cost_entry.create", "cost_entry", entry_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    persist_cost_entry_in_transaction(&db, prepared, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), services::Error>(())
                })
            })
            .await?;

        Ok(erp_finance::service::cost::CostService::new(self.db.clone())
            .cost_entry_detail(&entry_id)
            .await?)
    }
}
