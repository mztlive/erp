//! 采购责任规则管理和预览用例编排。

use super::dto::{
    CreateProcurementResponsibilityRuleRequest, ProcurementResponsibilityResolveLineView,
    ProcurementResponsibilityResolveRequest, ProcurementResponsibilityResolveView,
    ProcurementResponsibilityRuleListParams, ProcurementResponsibilityRulePageView,
    ProcurementResponsibilityRuleView, UpdateProcurementResponsibilityRuleRequest,
};
use super::resolver::ResolutionInput;
use super::ProcurementResponsibilityService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use database::{
    AccessControlExt, CatalogExt, NoTransaction, ProcurementResponsibilityExt,
    ProcurementResponsibilityRuleFilter, Transactional,
};
use entities::ids::ProcurementResponsibilityRuleId;
use entities::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
};

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
            .procurement_responsibility_rules()
            .search_procurement_responsibility_rules(&filter, &mut NoTransaction)
            .await?;
        Ok(ProcurementResponsibilityRulePageView {
            items: page.items.into_iter().map(Into::into).collect(),
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
    /// 选择器无效、负责人不合格、启用选择器冲突或事务失败时返回错误。
    pub async fn create_rule(
        &self,
        request: CreateProcurementResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let data = request.into_data();
        self.validate_selector_reference(&data).await?;
        self.ensure_owner_eligible(data.owner_user_id.as_str()).await?;
        let id = ProcurementResponsibilityRuleId::new(id_generator::next_id());
        let rule = ProcurementResponsibilityRule::new(id, data, actor.id()).map_err(Error::Logic)?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.create",
            "procurement_responsibility_rule",
            rule.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let persisted = rule.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.procurement_responsibility_rules()
                        .create(&persisted, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(rule.into())
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
    /// 规则不存在、版本冲突、选择器或负责人无效、唯一索引冲突时返回错误。
    pub async fn update_rule(
        &self,
        id: &str,
        request: UpdateProcurementResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRuleView> {
        let (version, data) = request.into_parts();
        self.validate_selector_reference(&data).await?;
        self.ensure_owner_eligible(data.owner_user_id.as_str()).await?;
        let mut rule = self
            .db
            .procurement_responsibility_rules()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购责任规则不存在".to_string()))?;
        if rule.base.version != version {
            return Err(Error::ConflictError("采购责任规则版本已变化".to_string()));
        }
        rule.update(data, actor.id()).map_err(Error::Logic)?;
        let audit = actor.clone().resource_log(
            "procurement_responsibility_rule.update",
            "procurement_responsibility_rule",
            rule.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut persisted = rule.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.procurement_responsibility_rules()
                        .update(&mut persisted, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        let updated = self
            .db
            .procurement_responsibility_rules()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购责任规则不存在".to_string()))?;
        Ok(updated.into())
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
        let request = request.ensure_unique_line_keys()?;
        let mut lines = Vec::with_capacity(request.lines.len());
        for line in request.lines {
            let line_key = line.line_key.clone();
            let result = self.resolve_strict(&[ResolutionInput::from(line)]).await;
            lines.push(match result {
                Ok(plan) => success_preview(plan.lines.into_iter().next().expect("单行解析必须返回单行")),
                Err(error) => failed_preview(line_key, error),
            });
        }
        Ok(ProcurementResponsibilityResolveView { lines })
    }

    /// 校验规则选择器引用的目录实体存在。
    ///
    /// # 参数
    /// * `data` - 待维护规则数据
    ///
    /// # 返回
    /// 引用存在或规则不需要目录引用时返回 `Ok(())`。
    ///
    /// # 错误
    /// SKU 或分类不存在时返回校验错误。
    async fn validate_selector_reference(&self, data: &ProcurementResponsibilityRuleData) -> Result<()> {
        match data.rule_type {
            ProcurementResponsibilityRuleType::Sku => {
                let id = data.sku_id.as_ref().ok_or_else(selector_shape_error)?;
                ensure_reference_exists(
                    self.db.skus().find_by_id(id.as_ref(), &mut NoTransaction).await?,
                    "SKU不存在或已删除",
                )
            }
            ProcurementResponsibilityRuleType::Category
            | ProcurementResponsibilityRuleType::CategoryServiceRegion => {
                let id = data.category_id.as_ref().ok_or_else(selector_shape_error)?;
                ensure_reference_exists(
                    self.db
                        .product_categories()
                        .find_by_id(id.as_ref(), &mut NoTransaction)
                        .await?,
                    "商品分类不存在或已删除",
                )
            }
            ProcurementResponsibilityRuleType::ProductKind
            | ProcurementResponsibilityRuleType::DefaultDispatcher => Ok(()),
        }
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

/// 校验目录引用查询命中实体。
fn ensure_reference_exists<T>(value: Option<T>, message: &str) -> Result<()> {
    value
        .map(|_| ())
        .ok_or_else(|| Error::ValidationError(message.to_string()))
}

/// 返回选择器形状错误。
fn selector_shape_error() -> Error {
    Error::ValidationError("采购责任规则类型与选择器字段不一致".to_string())
}
