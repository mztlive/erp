//! 手工成本登记 Process：校验销售单关联并协调财务事实与审计写入。
//!
//! 成本事实、分配金额规则及业务唯一键归 `erp-finance`；销售单存在性通过
//! `erp-sales` 仓储批量读取。入口先完成请求校验，再在单一事务中校验关联并使用
//! 同一 Executor 保存成本事实、分配行和审计，并装配登记结果后提交。

use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::{CostEntryId, SalesOrderId};
use erp_finance::dto::cost::{CostEntryView, CreateCostEntryRequest};
use erp_finance::service::cost::{
    CostService as FinanceCostService, dedupe_order_ids, missing_order_id, persist_cost_entry,
    prepare_cost_entry, validate_create_cost_entry,
};
use erp_identity::entity::authorization::{ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::cost;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, Result};
/// 手工成本登记的跨域流程。
pub struct CostProcess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}
impl CostProcess {
    /// 创建成本登记流程，复用入口数据库。
    ///
    /// # 参数
    /// * `db` - 入口数据库。
    /// * `authorization` - 应用共享的完整授权服务。
    ///
    /// # 返回
    /// 返回尚未执行登记的流程实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }
    /// 手工登记成本事实与分配行（跨集合事务写入）。
    ///
    /// 事务内校验归属销售单存在，分配合计必须等于成本事实金额（含税与
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

        let prepared = prepare_cost_entry(req)?;
        let entry_id = CostEntryId::new(prepared.entry.base.id.clone());
        let audit = actor.clone().resource_log("cost_entry.create", "cost_entry", entry_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorize_cost_sources(&db, &authorization, &actor, &unique_order_ids, executor).await?;

                    persist_cost_entry(&db, prepared, executor).await?;
                    persist_log(&db, &audit, executor).await?;

                    Ok(FinanceCostService::new(db)
                        .cost_entry_detail_with_executor(&entry_id, executor)
                        .await?)
                })
            })
            .await
    }
}

/// 先检查创建及真实来源资格，再解释业务来源缺失；全部读取共用原事务。
async fn authorize_cost_sources(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    order_ids: &[SalesOrderId],
    executor: &mut dyn Executor,
) -> Result<()> {
    let candidate = ObjectFacts {
        resource: "cost_entry".into(),
        relations: [(
            RelationKey::parse("sales_source")?,
            RelatedObjects {
                expected_ids: order_ids.iter().map(ToString::to_string).collect(),
                objects: vec![],
            },
        )]
        .into(),
        ..Default::default()
    };
    authorization.require(actor, &cost::ENTRY_CREATE, AccessTarget::Candidate(candidate), executor).await?;

    let existing = db.sales_order().find_existing_ids(order_ids, executor).await?;
    if missing_order_id(order_ids, &existing).is_some() {
        return Err(Error::NotFound("成本归属销售单不存在".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn denied_creation_does_not_probe_sales_order_existence() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("cost-source-authorization-unit");
        let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin);
        let error = authorize_cost_sources(
            &db,
            &unavailable_authorization(),
            &actor,
            &[SalesOrderId::new("unread-order")],
            &mut SnapshotExecutor::default(),
        )
        .await
        .expect_err("授权设施失败后不得读取销售单存在性");
        assert!(matches!(error, Error::Internal(message)
            if message == "测试授权设施不可用"));
    }
}
