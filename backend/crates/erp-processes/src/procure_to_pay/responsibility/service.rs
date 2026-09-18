//! 采购责任规则管理和预览用例编排.

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog};
use erp_core::ids::ProcurementResponsibilityRuleId;
use erp_procurement::dto::procurement_responsibility::{
    CreateProcurementResponsibilityRuleRequest, ProcurementResponsibilityResolveLineView,
    ProcurementResponsibilityResolveRequest, ProcurementResponsibilityResolveView,
    UpdateProcurementResponsibilityRuleRequest,
};
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityResolutionBatch, ProcurementResponsibilityRule,
    ProcurementResponsibilityRuleData,
};
use erp_procurement::service::procurement_responsibility::ProcurementResponsibilityService;
use erp_read_models::purchase_center::procurement_responsibility::dto::ProcurementResponsibilityRuleView;
use erp_read_models::purchase_center::procurement_responsibility::{
    apply_rule_list_facts, load_procurement_rule_list_facts,
};
use persistence_core::{Executor, NoTransaction};

use super::ProcurementResponsibilityProcess;
use super::adapter::ResponsibilityFactsAdapter;
use super::resolver::{ResolutionInput, load_owner_account};
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
        let policy_revision = self.authorize_owner_eligibility(data.owner_user_id.as_str()).await?;
        let id = ProcurementResponsibilityRuleId::new(id_generator::next_id());
        let rule = ProcurementResponsibilityRule::new(id, data.clone(), actor.id()).map_err(Error::Logic)?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.create",
            "procurement_responsibility_rule",
            rule.base.id.clone(),
        )?;
        let rule = self.persist_created_rule(rule, data, audit, policy_revision).await?;
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
        let policy_revision = self.authorize_owner_eligibility(data.owner_user_id.as_str()).await?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.update",
            "procurement_responsibility_rule",
            id.to_string(),
        )?;
        let rule = self.persist_updated_rule(id, version, data, actor.id(), audit, policy_revision).await?;
        let facts =
            load_procurement_rule_list_facts(&self.db, std::slice::from_ref(&rule), &mut NoTransaction)
                .await?;
        let mut view: ProcurementResponsibilityRuleView = rule.into();
        apply_rule_list_facts(std::slice::from_mut(&mut view), &facts);
        Ok(view)
    }

    /// 以负责人授权策略版本为提交栅栏创建规则与审计。
    ///
    /// # 参数
    /// * `rule` - 已完成实体校验的新规则
    /// * `data` - 用于事务内重验的完整规则数据
    /// * `audit` - 成功审计日志
    /// * `policy_revision` - 负责人资格校验使用的策略版本
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
        policy_revision: u64,
    ) -> Result<ProcurementResponsibilityRule> {
        let db = self.db.clone();
        let validation = Self::new(db.clone(), self.rbac.clone());
        let rbac = self.rbac.clone();
        rbac.run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                validation.validate_selector_reference(&data, executor).await?;
                load_owner_account(&db, data.owner_user_id.as_str(), executor).await?;
                ProcurementResponsibilityService::new(db.clone()).create_rule(&rule, executor).await?;
                db.audit_logs().create(&audit, executor).await?;
                Ok(rule)
            })
        })
        .await
    }

    /// 以负责人授权策略版本为提交栅栏整项更新规则与审计。
    ///
    /// # 参数
    /// * `id` - 待更新规则主键
    /// * `version` - 客户端期望乐观锁版本
    /// * `data` - 完整新规则数据
    /// * `updated_by` - 更新操作人账号 ID
    /// * `audit` - 成功审计日志
    /// * `policy_revision` - 负责人资格校验使用的策略版本
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
        updated_by: &str,
        audit: AuditLog,
        policy_revision: u64,
    ) -> Result<ProcurementResponsibilityRule> {
        let db = self.db.clone();
        let validation = Self::new(db.clone(), self.rbac.clone());
        let rbac = self.rbac.clone();
        let id = id.to_string();
        let updated_by = updated_by.to_string();
        rbac.run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                validation.validate_selector_reference(&data, executor).await?;
                load_owner_account(&db, data.owner_user_id.as_str(), executor).await?;
                let rule = ProcurementResponsibilityService::new(db.clone())
                    .update_rule(&id, version, data, &updated_by, executor)
                    .await?;
                db.audit_logs().create(&audit, executor).await?;
                Ok(rule)
            })
        })
        .await
    }

    /// 逐行预览采购责任解析结果。
    ///
    /// # 参数
    /// * `request` - 行键、SKU 与服务区域
    ///
    /// # 返回
    /// 返回与请求顺序一致的逐行成功或失败诊断。
    ///
    /// # 错误
    /// 请求行键重复时整体返回错误；单行业务失败写入对应结果。
    pub async fn resolve_preview(
        &self,
        request: ProcurementResponsibilityResolveRequest,
    ) -> Result<ProcurementResponsibilityResolveView> {
        let inputs = request
            .lines
            .into_iter()
            .map(|line| ResolutionInput::new(line.line_key, line.sku_id, line.service_region))
            .collect::<erp_core::Result<Vec<_>>>()
            .map_err(Error::Logic)?;
        ProcurementResponsibilityResolutionBatch::new(&inputs).map_err(Error::Logic)?;

        let mut lines = Vec::with_capacity(inputs.len());
        for input in inputs {
            let line_key = input.line_key.clone();
            let result = self.resolve_strict(std::slice::from_ref(&input)).await;
            lines.push(match result {
                Ok(plan) => {
                    let resolution = plan.views().into_iter().next().expect("单行解析必须返回单行");
                    success_preview(resolution)
                },
                Err(error) => failed_preview(line_key, error),
            });
        }
        Ok(ProcurementResponsibilityResolveView { lines })
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

/// 将成功解析转换为逐行预览视图。
fn success_preview(
    resolution: erp_procurement::dto::procurement_responsibility::ProcurementResponsibilityResolutionView,
) -> ProcurementResponsibilityResolveLineView {
    ProcurementResponsibilityResolveLineView {
        line_key: resolution.line_key,
        resolved: true,
        owner_user_id: Some(resolution.owner_user_id),
        owner_name: Some(resolution.owner_name),
        rule_id: Some(resolution.rule_id),
        rule_type: Some(resolution.rule_type),
        error: None,
    }
}

/// 将单行解析错误转换为失败预览。
fn failed_preview(line_key: String, error: Error) -> ProcurementResponsibilityResolveLineView {
    ProcurementResponsibilityResolveLineView {
        line_key,
        resolved: false,
        owner_user_id: None,
        owner_name: None,
        rule_id: None,
        rule_type: None,
        error: Some(error.to_string()),
    }
}
