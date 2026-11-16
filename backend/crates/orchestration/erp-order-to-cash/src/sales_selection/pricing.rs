//! 改价范围重验、领域写入与业务审计共用同一事务。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::execution::{AuditedCommand, AuditedWrite, run_audited_event};
use erp_audit::{BusinessEventContent, BusinessEventContext, BusinessEventResult, registered_action};
use erp_permissions::sales_selection;
use erp_read_models::sales_center::selection_pricing::enrich_selection_prices;
use erp_sales::dto::sales_selection::{SalesSelectionBookletView, UpdateSelectionPricesRequest};
use erp_sales::service::sales_selection::{SalesSelectionService, SelectionAccess};
use mongodb::Database;
use persistence_core::Executor;

use super::SalesSelectionProcess;
use crate::{Error, Result};

impl SalesSelectionProcess {
    /// 调整销售对客户的册内售价。
    ///
    /// # 参数
    /// `id` 为册，`req` 为改价命令，`actor` 为当前已认证操作人。
    ///
    /// # 返回
    /// 返回更新后的详情；同键重放不重复加价或记成功审计。
    ///
    /// # 错误
    /// 无维护权限、责任范围拒绝、业务冲突或审计失败时整次拒绝。
    pub async fn update_prices(
        &self,
        id: String,
        req: UpdateSelectionPricesRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        let context = BusinessEventContext::new(
            actor.clone(),
            registered_action("sales_selection_booklet.update_prices", "sales_selection_booklet")?,
        )?
        .with_target(Some(id.clone()), None)?;
        let command =
            PriceCommand { db: self.db.clone(), access: self.services.selections.clone(), id, req, actor };
        run_audited_event(&self.db, context, command).await
    }
}

struct PriceCommand {
    db: Database,
    access: SelectionAccess,
    id: String,
    req: UpdateSelectionPricesRequest,
    actor: AuditActor,
}

#[async_trait]
impl AuditedCommand for PriceCommand {
    type Error = Error;
    type Output = SalesSelectionBookletView;

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        self.access
            .require_booklet(&self.actor, &sales_selection::BOOKLET_DELETE_ITEM, &self.id, executor)
            .await?;
        let (mut result, fresh) = SalesSelectionService::new(self.db.clone(), self.access.authorization())
            .update_prices(&self.id, self.req.clone(), self.actor.id(), executor)
            .await?;
        enrich_selection_prices(&self.db, &mut result, executor).await?;
        if !fresh {
            return Ok(AuditedWrite::Replayed(result));
        }
        let content = BusinessEventContent {
            target_id: self.id.clone(),
            target_number: None,
            result: BusinessEventResult::Succeeded,
            field_changes: Vec::new(),
            facts: Vec::new(),
        };
        Ok(AuditedWrite::Fresh { result, content })
    }
}
