//! 采购责任规则管理和预览用例编排.

use application_core::AuditActor;
use erp_audit::execution::persist_log;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::ProcurementResponsibilityRuleId;
use erp_identity::AccessTarget;
use erp_permissions::admin;
use erp_procurement::dto::procurement_responsibility::{
    CreateProcurementResponsibilityRuleRequest, UpdateProcurementResponsibilityRuleRequest,
};
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleData,
};
use erp_procurement::service::procurement_responsibility::ProcurementResponsibilityService;
use erp_read_models::purchase_center::procurement_responsibility::dto::ProcurementResponsibilityRuleView;
use erp_read_models::purchase_center::procurement_responsibility::{
    apply_rule_list_facts, load_procurement_rule_list_facts,
};
use persistence_core::{Executor, Transactional};

use super::ProcurementResponsibilityProcess;
use super::adapter::ResponsibilityFactsAdapter;
use crate::{Error, Result};

impl ProcurementResponsibilityProcess {
    /// 创建采购责任规则并记录审计。
    ///
    /// `request` 为规则类型、选择器、具体负责人及状态。返回创建后的规则视图。选择器无效、负责人不合格、负责人授权版本变化、启用选择器冲突或事务失败时返回错误。
    pub async fn create_rule(
        &self,
        request: CreateProcurementResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let data = request.into_data();
        let id = ProcurementResponsibilityRuleId::new(id_generator::next_id());
        let rule = ProcurementResponsibilityRule::new(id, data.clone(), actor.id()).map_err(Error::Logic)?;

        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.create",
            "procurement_responsibility_rule",
            rule.base.id.clone(),
        )?;
        self.persist_created_rule(rule, data, audit, actor.clone()).await
    }

    /// 整项更新采购责任规则并记录审计。
    ///
    /// `id` 为规则主键；`request` 为期望版本与完整新规则数据。返回更新后的规则视图。
    ///
    /// 规则不存在、版本冲突、选择器或负责人无效、负责人授权版本变化或唯一索引冲突时返回错误。
    pub async fn update_rule(
        &self,
        id: &str,
        request: UpdateProcurementResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let (version, data) = request.into_parts();
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.update",
            "procurement_responsibility_rule",
            id.to_string(),
        )?;
        self.persist_updated_rule(id, version, data, actor.clone(), audit).await
    }

    // 在原事务内重验管理资格及负责人资格并创建规则与审计。
    //
    // `rule` 为已完成实体校验的新规则；`data` 为用于事务内重验的完整规则数据；`audit` 为成功审计日志。返回已原子提交的规则。
    //
    // 选择器或负责人事务内失效、策略变化、唯一冲突或写入失败时返回错误。
    async fn persist_created_rule(
        &self,
        rule: ProcurementResponsibilityRule,
        data: ProcurementResponsibilityRuleData,
        audit: AuditLog,
        actor: AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let db = self.db.clone();
        let validation = Self::new(db.clone(), self.rbac.clone());
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { validation.create_rule_stage(rule, data, audit, &actor, executor).await },
                )
            })
            .await
    }

    // 在原事务内重验管理资格及负责人资格并整项更新规则与审计。
    //
    // `id` 为待更新规则主键；`version` 为客户端期望乐观锁版本；`data` 为完整新规则数据；`actor` 为当前管理操作人；`audit` 为成功审计日志。
    //
    // 返回事务内更新后的规则。规则不存在、版本冲突、选择器或负责人失效、策略变化或写入失败时返回错误。
    async fn persist_updated_rule(
        &self,
        id: &str,
        version: u64,
        data: ProcurementResponsibilityRuleData,
        actor: AuditActor,
        audit: AuditLog,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let db = self.db.clone();
        let validation = Self::new(db.clone(), self.rbac.clone());
        let id = id.to_string();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    validation.update_rule_stage(&id, version, data, audit, &actor, executor).await
                })
            })
            .await
    }

    // 管理授权必须先于引用、负责人和现有规则读取。
    async fn create_rule_stage(
        &self,
        rule: ProcurementResponsibilityRule,
        data: ProcurementResponsibilityRuleData,
        audit: AuditLog,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        self.require_management(actor, executor).await?;
        self.validate_selector_reference(&data, executor).await?;
        self.validate_pool(&data, executor).await?;

        ProcurementResponsibilityService::new(self.db.clone()).create_rule(&rule, executor).await?;
        persist_log(&self.db, &audit, executor).await?;

        self.command_view(rule, actor, executor).await
    }

    // 规则更新和展示名称水合不能离开原授权事务。
    #[allow(clippy::too_many_arguments)]
    async fn update_rule_stage(
        &self,
        id: &str,
        version: u64,
        data: ProcurementResponsibilityRuleData,
        audit: AuditLog,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        self.require_management(actor, executor).await?;
        self.validate_selector_reference(&data, executor).await?;
        self.validate_pool(&data, executor).await?;

        let rule = ProcurementResponsibilityService::new(self.db.clone())
            .update_rule(id, version, data, actor.id(), executor)
            .await?;
        persist_log(&self.db, &audit, executor).await?;

        self.command_view(rule, actor, executor).await
    }

    // 写入后重新证明原管理资格，再在同一快照补齐命令回包。
    async fn command_view(
        &self,
        rule: ProcurementResponsibilityRule,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        self.require_management(actor, executor).await?;
        let facts = load_procurement_rule_list_facts(&self.db, std::slice::from_ref(&rule), executor).await?;
        let mut view = rule.into();
        apply_rule_list_facts(std::slice::from_mut(&mut view), &facts);
        Ok(view)
    }

    // 在原事务证明责任配置管理的当前功能资格。
    async fn require_management(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.rbac
            .require(actor, &admin::PROCUREMENT_RESPONSIBILITY_CREATE_RULE, AccessTarget::Operation, executor)
            .await?;
        Ok(())
    }

    // 在原授权事务内使用目录事实端口校验引用。
    async fn validate_selector_reference(
        &self,
        data: &ProcurementResponsibilityRuleData,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(ProcurementResponsibilityService::new(self.db.clone())
            .validate_selector_reference(data, &ResponsibilityFactsAdapter { db: self.db.clone() }, executor)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_procurement::entity::procurement_responsibility::{
        AssignmentStrategy, EnableStatus, ProcurementResponsibilityRuleType,
    };
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn management_failure_stops_create_update_and_result_hydration_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("rule_guard");
        let process = ProcurementResponsibilityProcess::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let data = ProcurementResponsibilityRuleData {
            rule_type: ProcurementResponsibilityRuleType::DefaultDispatcher,
            sku_id: None,
            category_id: None,
            product_kind: None,
            owner_user_ids: vec!["buyer".into()],
            strategy: AssignmentStrategy::RoundRobin,
            status: EnableStatus::Active,
        };
        let rule = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("rule"),
            data.clone(),
            actor.id(),
        )
        .unwrap();
        let audit = actor
            .clone()
            .resource_log(
                "procurement_responsibility_rule.create",
                "procurement_responsibility_rule",
                "rule".into(),
            )
            .unwrap();
        let mut executor = SnapshotExecutor::default();

        let create =
            process.create_rule_stage(rule.clone(), data.clone(), audit.clone(), &actor, &mut executor).await;
        let update = process.update_rule_stage("rule", 1, data, audit, &actor, &mut executor).await;
        let view = process.command_view(rule, &actor, &mut executor).await;

        for result in [create, update, view] {
            assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
        }
    }
}
