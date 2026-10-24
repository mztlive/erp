//! 手工成本登记 Process：校验销售单关联并协调财务事实与审计写入。
//!
//! 成本事实、分配金额规则及业务唯一键归 `erp-finance`；销售单存在性通过
//! `erp-sales` 仓储批量读取。入口先完成请求与关联校验，再在单一事务中使用
//! 同一 Executor 保存成本事实、分配行和审计。提交后由财务领域读取登记详情。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::ids::CostEntryId;
use erp_finance::dto::cost::{CostEntryView, CreateCostEntryRequest};
use erp_finance::service::cost::{
    CostService as FinanceCostService, dedupe_order_ids, missing_order_id, persist_cost_entry,
    prepare_cost_entry, validate_create_cost_entry,
};
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};

use crate::audit::persist_log;
use crate::{Error, Result};
/// 手工成本登记的跨域流程。
pub struct CostProcess {
    db: Database,
}
impl CostProcess {
    /// 创建成本登记流程，复用入口数据库。
    ///
    /// # 参数
    /// * `db` - 入口数据库。
    ///
    /// # 返回
    /// 返回尚未执行登记的流程实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 手工登记成本事实与分配行（跨集合事务写入）。
    ///
    /// 事务前校验归属销售单存在，分配合计必须等于成本事实金额（含税与
    /// 不含税双侧）；事务内写入成本事实、分配行与审计，保证「事实 +
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
        // Repository 只返回已存在 ID 的最小事实，跨聚合报错决策保留 Process。
        let requested_order_ids =
            req.allocations.iter().map(|line| line.sales_order_id.clone()).collect::<Vec<_>>();
        let unique_order_ids = dedupe_order_ids(&requested_order_ids);
        let existing_order_ids =
            self.db.sales_order().find_existing_ids(&unique_order_ids, &mut NoTransaction).await?;
        if missing_order_id(&unique_order_ids, &existing_order_ids).is_some() {
            return Err(Error::NotFound("成本归属销售单不存在".to_string()));
        }
        let prepared = prepare_cost_entry(req)?;
        let entry_id = CostEntryId::new(prepared.entry.base.id.clone());
        let audit = actor.clone().resource_log("cost_entry.create", "cost_entry", entry_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    persist_cost_entry(&db, prepared, executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;

        Ok(FinanceCostService::new(self.db.clone()).cost_entry_detail(&entry_id).await?)
    }
}
