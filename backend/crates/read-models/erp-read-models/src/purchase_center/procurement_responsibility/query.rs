//! 采购责任列表查询与显示事实装配。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::AccessTarget;
use erp_permissions::admin;
use erp_procurement::repository::procurement_responsibility::ProcurementResponsibilityRuleFilter;
use persistence_core::{Executor, Transactional};

use super::dto::{ProcurementResponsibilityRuleListParams, ProcurementResponsibilityRulePageView};
use super::{
    ProcurementResponsibilityReadService, ProcurementRuleListPage, load_procurement_rule_list_page,
    to_rule_list_views,
};
use crate::{Error, Result};

/// 规则列表只暴露固定资格检查与包含展示事实的完整分页读取。
#[async_trait]
trait RuleListSource: Send + Sync {
    /// 在调用方快照中检查规则列表功能资格。
    async fn require(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()>;

    /// 使用原执行器读取规则、总数和本页全部展示事实。
    async fn page(
        &self,
        filter: &ProcurementResponsibilityRuleFilter,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementRuleListPage>;
}

#[async_trait]
impl RuleListSource for ProcurementResponsibilityReadService {
    /// 固定资格由共享授权服务执行，不由列表调用方组合。
    async fn require(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(actor, &admin::PROCUREMENT_RESPONSIBILITY_LIST_RULES, AccessTarget::Operation, executor)
            .await?;
        Ok(())
    }

    /// 仓储在同一执行器内完成分页、计数和关联事实补齐。
    async fn page(
        &self,
        filter: &ProcurementResponsibilityRuleFilter,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementRuleListPage> {
        Ok(load_procurement_rule_list_page(&self.db, filter, executor).await?)
    }
}

impl ProcurementResponsibilityReadService {
    /// 在同一授权快照中分页查询采购责任规则及全部展示事实。
    ///
    /// # 参数
    /// * `actor` - 本次已认证账号。
    /// * `params` - 类型、负责人、状态及分页筛选
    ///
    /// # 返回
    /// 返回规则管理分页视图。
    ///
    /// # 错误
    /// 功能资格拒绝、快照失效或 MongoDB 查询失败时返回错误。
    pub async fn rule_list(
        &self,
        params: ProcurementResponsibilityRuleListParams,
        actor: &AuditActor,
    ) -> Result<ProcurementResponsibilityRulePageView> {
        let service = Self::new(self.db.clone(), self.authorization.clone());
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { rule_list_page(&service, params, &actor, executor).await })
            })
            .await
    }
}

/// 固定资格、完整分页和响应补齐只能在原快照读取阶段完成。
async fn rule_list_page(
    source: &dyn RuleListSource,
    params: ProcurementResponsibilityRuleListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ProcurementResponsibilityRulePageView> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("采购责任列表读取必须使用快照事务".into()));
    }

    source.require(actor, executor).await?;
    let boundary = executor
        .boundary()
        .filter(|boundary| boundary.is_snapshot())
        .ok_or_else(|| Error::ValidationError("采购责任列表缺少授权快照".into()))?;

    let filter = ProcurementResponsibilityRuleFilter {
        rule_type: params.rule_type,
        owner_user_id: params.owner_user_id,
        status: params.status,
        page: params.page,
        page_size: params.page_size,
    };
    let page = source.page(&filter, executor).await?;
    let items = to_rule_list_views(page.items, &page.facts);

    if !boundary.is_current(executor) {
        return Err(Error::ValidationError("采购责任列表读取跨越授权快照".into()));
    }
    Ok(ProcurementResponsibilityRulePageView {
        items,
        total: page.total,
        page: params.page,
        page_size: params.page_size,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_core::ids::{ProcurementResponsibilityRuleId, SkuId};
    use erp_procurement::entity::procurement_responsibility::{
        EnableStatus, ProcurementResponsibilityRule, ProcurementResponsibilityRuleData,
        ProcurementResponsibilityRuleType,
    };
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::purchase_center::procurement_responsibility::ProcurementRuleListDisplayFacts;

    /// 每个阶段记录原执行器；错误分别在授权、读取和阶段变化处注入。
    #[derive(Default)]
    struct Source {
        denied: bool,
        failed: bool,
        advance: bool,
        calls: Mutex<Vec<(&'static str, usize)>>,
    }

    impl Source {
        /// 验证资格与读取均使用活动快照并记录执行器身份。
        fn record(&self, stage: &'static str, executor: &mut dyn Executor) {
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            self.calls.lock().unwrap().push((stage, executor as *mut dyn Executor as *mut () as usize));
        }
    }

    #[async_trait]
    impl RuleListSource for Source {
        /// 模拟真实授权入口推进读取阶段并检查当前账号。
        async fn require(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
            self.record("require", executor);
            assert_eq!(actor.id(), "actor-1");
            executor.next_read();
            if self.denied {
                return Err(Error::Forbidden("缺少规则列表权限".into()));
            }
            Ok(())
        }

        /// 返回一页真实规则实体与稀疏展示事实，或注入读取失败。
        async fn page(
            &self,
            filter: &ProcurementResponsibilityRuleFilter,
            executor: &mut dyn Executor,
        ) -> Result<ProcurementRuleListPage> {
            self.record("page", executor);
            assert_eq!(filter.rule_type, Some(ProcurementResponsibilityRuleType::Sku));
            assert_eq!(filter.owner_user_id.as_deref(), Some("owner-1"));
            assert_eq!(filter.status, Some(EnableStatus::Active));
            assert_eq!((filter.page, filter.page_size), (2, 3));
            if self.failed {
                return Err(Error::ConflictError("读取规则失败".into()));
            }
            if self.advance {
                executor.next_read();
            }
            Ok(sample_page())
        }
    }

    /// 构造显式筛选，验证生产编排保留全部分页参数。
    fn params() -> ProcurementResponsibilityRuleListParams {
        ProcurementResponsibilityRuleListParams {
            rule_type: Some(ProcurementResponsibilityRuleType::Sku),
            owner_user_id: Some("owner-1".into()),
            status: Some(EnableStatus::Active),
            page: 2,
            page_size: 3,
        }
    }

    /// 构造当前调用身份。
    fn actor() -> AuditActor {
        AuditActor::new("actor-1".into(), "操作者".into(), AccountKind::Admin)
    }

    /// 用真实规则实体和展示事实验证最终回包。
    fn sample_page() -> ProcurementRuleListPage {
        let rule = ProcurementResponsibilityRule::new(
            ProcurementResponsibilityRuleId::new("rule-1"),
            ProcurementResponsibilityRuleData {
                rule_type: ProcurementResponsibilityRuleType::Sku,
                sku_id: Some(SkuId::new("sku-1")),
                category_id: None,
                product_kind: None,
                owner_user_ids: vec!["owner-1".into()],
                strategy: Default::default(),
                status: EnableStatus::Active,
            },
            "creator-1",
        )
        .unwrap();
        ProcurementRuleListPage {
            items: vec![rule],
            total: 7,
            facts: ProcurementRuleListDisplayFacts {
                owner_names: [("owner-1".into(), "张三".into())].into(),
                sku_nos: [("sku-1".into(), "SKU-001".into())].into(),
                sku_names: [("sku-1".into(), "零件".into())].into(),
                ..Default::default()
            },
        }
    }

    /// 允许路径使用同一执行器完成授权、完整读取和最终展示补齐。
    #[tokio::test]
    async fn allowed_page_keeps_original_executor_and_fills_display_facts() {
        let source = Source::default();
        let mut executor = SnapshotExecutor::default();
        let address = &mut executor as *mut SnapshotExecutor as usize;

        let view = rule_list_page(&source, params(), &actor(), &mut executor).await.unwrap();

        assert_eq!(*source.calls.lock().unwrap(), [("require", address), ("page", address)]);
        assert_eq!((view.total, view.page, view.page_size), (7, 2, 3));
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].id, "rule-1");
        assert_eq!(view.items[0].owner_names, [Some("张三".into())]);
        assert_eq!(view.items[0].sku_no.as_deref(), Some("SKU-001"));
        assert_eq!(view.items[0].sku_name.as_deref(), Some("零件"));
    }

    /// 缺少功能资格时终止，不读取分页或人员事实。
    #[tokio::test]
    async fn denied_permission_stops_before_page_reading() {
        let source = Source { denied: true, ..Default::default() };
        let mut executor = SnapshotExecutor::default();
        let result = rule_list_page(&source, params(), &actor(), &mut executor).await;
        assert!(matches!(result, Err(Error::Forbidden(message)) if message == "缺少规则列表权限"));
        assert_eq!(source.calls.lock().unwrap().len(), 1);
    }

    /// 读取错误原样返回，不交付部分页面。
    #[tokio::test]
    async fn page_failure_is_preserved() {
        let source = Source { failed: true, ..Default::default() };
        let mut executor = SnapshotExecutor::default();
        let result = rule_list_page(&source, params(), &actor(), &mut executor).await;
        assert!(matches!(result, Err(Error::ConflictError(message)) if message == "读取规则失败"));
        assert_eq!(source.calls.lock().unwrap().len(), 2);
    }

    /// 非快照读取在访问身份和业务数据之前拒绝。
    #[tokio::test]
    async fn non_snapshot_executor_is_rejected_before_authorization() {
        let source = Source::default();
        let result = rule_list_page(&source, params(), &actor(), &mut ReadExecutor::default()).await;
        assert!(
            matches!(result, Err(Error::ValidationError(message)) if message == "采购责任列表读取必须使用快照事务")
        );
        assert!(source.calls.lock().unwrap().is_empty());
    }

    /// 读取阶段被推进后丢弃已取出的页面。
    #[tokio::test]
    async fn changed_read_phase_discards_page() {
        let source = Source { advance: true, ..Default::default() };
        let mut executor = SnapshotExecutor::default();
        let result = rule_list_page(&source, params(), &actor(), &mut executor).await;
        assert!(
            matches!(result, Err(Error::ValidationError(message)) if message == "采购责任列表读取跨越授权快照")
        );
    }
}
