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
use persistence_core::{Executor, NoTransaction, Transactional};

use super::ProcurementResponsibilityProcess;
use super::adapter::ResponsibilityFactsAdapter;
use crate::{Error, Result};

impl ProcurementResponsibilityProcess {
    /// 创建采购责任规则并记录审计。
    ///
    /// # 参数
    /// * `request` - 规则类型、选择器、具体负责人及状态
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回创建后的规则视图。
    ///
    /// # 错误
    /// 选择器无效、负责人不合格、负责人授权版本变化、启用选择器冲突或事务失败时返回错误。
    pub async fn create_rule(
        &self,
        request: CreateProcurementResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let data = request.into_data();
        self.validate_selector_reference(&data, &mut NoTransaction).await?;
        self.validate_pool(&data, &mut NoTransaction).await?;
        let id = ProcurementResponsibilityRuleId::new(id_generator::next_id());
        let rule = ProcurementResponsibilityRule::new(id, data.clone(), actor.id()).map_err(Error::Logic)?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.create",
            "procurement_responsibility_rule",
            rule.base.id.clone(),
        )?;
        let rule = self.persist_created_rule(rule, data, audit, actor.clone()).await?;
        let facts =
            load_procurement_rule_list_facts(&self.db, std::slice::from_ref(&rule), &mut NoTransaction)
                .await?;
        let mut view: ProcurementResponsibilityRuleView = rule.into();
        apply_rule_list_facts(std::slice::from_mut(&mut view), &facts);
        Ok(view)
    }

    /// 整项更新采购责任规则并记录审计。
    ///
    /// # 参数
    /// * `id` - 规则主键
    /// * `request` - 期望版本与完整新规则数据
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回更新后的规则视图。
    ///
    /// # 错误
    /// 规则不存在、版本冲突、选择器或负责人无效、负责人授权版本变化或唯一索引冲突时返回错误。
    pub async fn update_rule(
        &self,
        id: &str,
        request: UpdateProcurementResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let (version, data) = request.into_parts();
        self.validate_selector_reference(&data, &mut NoTransaction).await?;
        self.validate_pool(&data, &mut NoTransaction).await?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.update",
            "procurement_responsibility_rule",
            id.to_string(),
        )?;
        let rule = self.persist_updated_rule(id, version, data, actor.clone(), audit).await?;
        let facts =
            load_procurement_rule_list_facts(&self.db, std::slice::from_ref(&rule), &mut NoTransaction)
                .await?;
        let mut view: ProcurementResponsibilityRuleView = rule.into();
        apply_rule_list_facts(std::slice::from_mut(&mut view), &facts);
        Ok(view)
    }

    /// 在原事务内重验管理资格及负责人资格并创建规则与审计。
    ///
    /// # 参数
    /// * `rule` - 已完成实体校验的新规则
    /// * `data` - 用于事务内重验的完整规则数据
    /// * `audit` - 成功审计日志
    ///
    /// # 返回
    /// 返回已原子提交的规则。
    ///
    /// # 错误
    /// 选择器或负责人事务内失效、策略变化、唯一冲突或写入失败时返回错误。
    async fn persist_created_rule(
        &self,
        rule: ProcurementResponsibilityRule,
        data: ProcurementResponsibilityRuleData,
        audit: AuditLog,
        actor: AuditActor,
    ) -> Result<ProcurementResponsibilityRule> {
        let db = self.db.clone();
        let validation = Self::new(db.clone(), self.rbac.clone());
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    validation.require_management(&actor, executor).await?;
                    validation.validate_selector_reference(&data, executor).await?;
                    validation.validate_pool(&data, executor).await?;
                    ProcurementResponsibilityService::new(db.clone()).create_rule(&rule, executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(rule)
                })
            })
            .await
    }

    /// 在原事务内重验管理资格及负责人资格并整项更新规则与审计。
    ///
    /// # 参数
    /// * `id` - 待更新规则主键
    /// * `version` - 客户端期望乐观锁版本
    /// * `data` - 完整新规则数据
    /// * `actor` - 当前管理操作人
    /// * `audit` - 成功审计日志
    ///
    /// # 返回
    /// 返回事务内更新后的规则。
    ///
    /// # 错误
    /// 规则不存在、版本冲突、选择器或负责人失效、策略变化或写入失败时返回错误。
    async fn persist_updated_rule(
        &self,
        id: &str,
        version: u64,
        data: ProcurementResponsibilityRuleData,
        actor: AuditActor,
        audit: AuditLog,
    ) -> Result<ProcurementResponsibilityRule> {
        let db = self.db.clone();
        let validation = Self::new(db.clone(), self.rbac.clone());
        let id = id.to_string();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    validation.require_management(&actor, executor).await?;
                    validation.validate_selector_reference(&data, executor).await?;
                    validation.validate_pool(&data, executor).await?;
                    let rule = ProcurementResponsibilityService::new(db.clone())
                        .update_rule(&id, version, data, actor.id(), executor)
                        .await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(rule)
                })
            })
            .await
    }

    /// 在原事务证明责任配置管理的当前功能资格。
    async fn require_management(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.rbac
            .require(actor, &admin::PROCUREMENT_RESPONSIBILITY_CREATE_RULE, AccessTarget::Operation, executor)
            .await?;
        Ok(())
    }

    /// 在事务外预检和原授权提交事务内使用相同目录事实端口校验引用。
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
