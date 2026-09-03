//! 采购责任规则管理和预览用例编排.

use super::dto::{
    CreateProcurementResponsibilityRuleRequest, ProcurementResponsibilityResolveLineView,
    ProcurementResponsibilityResolveRequest, ProcurementResponsibilityResolveView,
    ProcurementResponsibilityRuleListParams, ProcurementResponsibilityRulePageView,
    ProcurementResponsibilityRuleView, UpdateProcurementResponsibilityRuleRequest,
};
use super::resolver::{load_owner_account, ResolutionInput};
use super::rule_list::{apply_rule_list_facts, to_rule_list_views};
use super::ProcurementResponsibilityService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use database::{
    AccessControlExt, CatalogExt, Executor, NoTransaction, ProcurementResponsibilityExt,
    ProcurementResponsibilityRuleFilter,
};
use entities::ids::ProcurementResponsibilityRuleId;
use entities::procurement_responsibility::{
    ProcurementResponsibilityResolutionBatch, ProcurementResponsibilityRule,
    ProcurementResponsibilityRuleData, ProcurementResponsibilitySelectorReference,
};
use entities::AuditLog;

impl ProcurementResponsibilityService {
    /// 分页查询采购责任规则。
    ///
    /// # 参数
    /// * `params` - 类型、负责人、状态及分页筛选
    ///
    /// # 返回
    /// 返回规则管理分页视图。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn rule_list(
        &self,
        params: ProcurementResponsibilityRuleListParams,
    ) -> Result<ProcurementResponsibilityRulePageView> {
        let filter = ProcurementResponsibilityRuleFilter {
            rule_type: params.rule_type,
            owner_user_id: params.owner_user_id,
            status: params.status,
            page: params.page,
            page_size: params.page_size,
        };
        let page = self
            .db
            .load_procurement_rule_list_page(&filter, &mut NoTransaction)
            .await?;
        let items = to_rule_list_views(page.items, &page.facts);
        Ok(ProcurementResponsibilityRulePageView {
            items,
            total: page.total,
            page: params.page,
            page_size: params.page_size,
        })
    }

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
        self.validate_selector_reference(&data, &mut NoTransaction)
            .await?;
        let policy_revision = self
            .authorize_owner_eligibility(data.owner_user_id.as_str())
            .await?;
        let id = ProcurementResponsibilityRuleId::new(id_generator::next_id());
        let rule = ProcurementResponsibilityRule::new(id, data.clone(), actor.id()).map_err(Error::Logic)?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.create",
            "procurement_responsibility_rule",
            rule.base.id.clone(),
        )?;
        let rule = self
            .persist_created_rule(rule, data, audit, policy_revision)
            .await?;
        let facts = self
            .db
            .load_procurement_rule_list_facts(std::slice::from_ref(&rule), &mut NoTransaction)
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
        self.validate_selector_reference(&data, &mut NoTransaction)
            .await?;
        let policy_revision = self
            .authorize_owner_eligibility(data.owner_user_id.as_str())
            .await?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.update",
            "procurement_responsibility_rule",
            id.to_string(),
        )?;
        let rule = self
            .persist_updated_rule(id, version, data, actor.id(), audit, policy_revision)
            .await?;
        let facts = self
            .db
            .load_procurement_rule_list_facts(std::slice::from_ref(&rule), &mut NoTransaction)
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
        rbac.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                validation.validate_selector_reference(&data, session).await?;
                load_owner_account(&db, data.owner_user_id.as_str(), session).await?;
                db.procurement_responsibility_rules()
                    .create(&rule, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
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
        rbac.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                validation.validate_selector_reference(&data, session).await?;
                load_owner_account(&db, data.owner_user_id.as_str(), session).await?;
                let mut rule = db
                    .procurement_responsibility_rules()
                    .find_procurement_responsibility_rule(&id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("采购责任规则不存在".to_string()))?;
                if rule.base.version != version {
                    return Err(Error::ConflictError("采购责任规则版本已变化".to_string()));
                }
                rule.update(data, updated_by).map_err(Error::Logic)?;
                db.procurement_responsibility_rules()
                    .update(&mut rule, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
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
            .collect::<entities::Result<Vec<_>>>()
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
                }
                Err(error) => failed_preview(line_key, error),
            });
        }
        Ok(ProcurementResponsibilityResolveView { lines })
    }

    /// 校验规则选择器引用的目录实体存在。
    ///
    /// # 参数
    /// * `data` - 待维护规则数据
    /// * `executor` - 数据库执行器，可为规则写事务会话
    ///
    /// # 返回
    /// 引用存在或规则不需要目录引用时返回 `Ok(())`。
    ///
    /// # 错误
    /// SKU 或分类不存在时返回校验错误。
    async fn validate_selector_reference(
        &self,
        data: &ProcurementResponsibilityRuleData,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let exists = match data.selector_reference().map_err(Error::Logic)? {
            ProcurementResponsibilitySelectorReference::Sku(id) => {
                self.db
                    .skus()
                    .has_procurement_responsibility_sku(id, executor)
                    .await?
            }
            ProcurementResponsibilitySelectorReference::Category(id) => {
                self.db
                    .product_categories()
                    .has_procurement_responsibility_category(id, executor)
                    .await?
            }
            ProcurementResponsibilitySelectorReference::None => return Ok(()),
        };
        if exists {
            return Ok(());
        }
        Err(Error::ValidationError(
            "采购责任规则引用的目录实体不存在或已删除".to_string(),
        ))
    }
}

/// 将成功解析转换为逐行预览视图。
fn success_preview(
    resolution: super::dto::ProcurementResponsibilityResolutionView,
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

#[cfg(test)]
mod tests {
    /// 验证规则创建与更新的授权提交栅栏。
    ///
    /// 两条写路径都必须在 policy CAS 事务内重验负责人账号和选择器引用，
    /// 任一调用缺失时测试失败。
    #[test]
    fn rule_writes_bind_owner_authorization_and_revalidate_references() {
        let production = include_str!("service.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert_eq!(
            production
                .matches("run_authorized_policy_transaction(policy_revision")
                .count(),
            2
        );
        assert!(production.contains("validation.validate_selector_reference(&data, session)"));
        assert!(production.contains("load_owner_account(&db, data.owner_user_id.as_str(), session)"));
    }
}
