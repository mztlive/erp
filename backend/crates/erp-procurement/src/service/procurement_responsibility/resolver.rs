//! 采购责任候选解析：目录完整性、规则优先级和账号资格由本域顺序执行。

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use super::ProcurementResponsibilityService;
use crate::entity::facts::IdentityOwnerFact;
use crate::entity::procurement_responsibility::{
    EligibleProcurementOwner, ProcurementResponsibilityContext, ProcurementResponsibilityResolutionBatch,
    ProcurementResponsibilityResolutionLine, ProcurementResponsibilityRule, ProcurementResponsibilityRuleSet,
    ProcurementResponsibilityRuleType, build_catalog_facts,
};
use crate::ports::procurement_responsibility::ProcurementResponsibilityFactsPort;
use crate::repository::ProcurementResponsibilityExt;
use crate::{Error, Result};

/// 保持调用方稳定行键、SKU 与区域的解析输入。
pub type ResolutionInput = ProcurementResponsibilityResolutionLine;

/// 尚未经过 RBAC 校验的候选结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateResolution {
    /// 调用方稳定行键。
    pub line_key: String,
    /// 通过账号资格校验的负责人 ID。
    pub owner_user_id: String,
    /// 负责人展示姓名，不参与授权或身份比较。
    pub owner_name: String,
    /// 按确定性优先级选中的规则 ID。
    pub rule_id: String,
    /// 命中的规则类型，必须纳入后续身份比较。
    pub rule_type: ProcurementResponsibilityRuleType,
}

impl ProcurementResponsibilityService {
    /// 按输入顺序解析候选责任；所有事实读取复用调用方执行器。
    ///
    /// 目录、规则或负责人事实缺失时立即失败，不执行授权或改变规则。
    pub async fn resolve_candidates(
        &self,
        inputs: &[ResolutionInput],
        port: &dyn ProcurementResponsibilityFactsPort,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CandidateResolution>> {
        resolve_candidates(&DatabaseRuleStore { db: &self.db }, port, inputs, executor).await
    }
}

#[async_trait]
trait ResponsibilityRuleStore: Send + Sync {
    async fn active_rules(&self, executor: &mut dyn Executor) -> Result<Vec<ProcurementResponsibilityRule>>;
}

struct DatabaseRuleStore<'a> {
    db: &'a mongodb::Database,
}

#[async_trait]
impl ResponsibilityRuleStore for DatabaseRuleStore<'_> {
    async fn active_rules(&self, executor: &mut dyn Executor) -> Result<Vec<ProcurementResponsibilityRule>> {
        Ok(self
            .db
            .procurement_responsibility_rules()
            .list_active_procurement_responsibility_rules(executor)
            .await?)
    }
}

async fn resolve_candidates(
    store: &dyn ResponsibilityRuleStore,
    port: &dyn ProcurementResponsibilityFactsPort,
    inputs: &[ResolutionInput],
    executor: &mut dyn Executor,
) -> Result<Vec<CandidateResolution>> {
    let inputs = ProcurementResponsibilityResolutionBatch::new(inputs).map_err(Error::Logic)?.lines();
    let sku_ids = unique_sku_ids(inputs);
    let bundle = port.load_catalog(&sku_ids, executor).await.map_err(|e| Error::Internal(e.to_string()))?;
    let facts =
        build_catalog_facts(inputs, &bundle.skus, &bundle.products, &bundle.revisions, &bundle.categories)
            .map_err(Error::Logic)?;
    let rules = store.active_rules(executor).await?;
    let rule_set = ProcurementResponsibilityRuleSet::new(&rules);
    let mut selected = Vec::with_capacity(inputs.len());
    for input in inputs {
        let fact = facts
            .get(input.line_key.as_str())
            .ok_or_else(|| Error::Internal("采购责任目录事实未完整返回".to_string()))?;
        let context = ProcurementResponsibilityContext::new(
            input.sku_id.clone(),
            fact.category_chain.clone(),
            input.service_region.clone(),
            fact.product_kind,
        )
        .map_err(Error::Logic)?;
        let rule = rule_set.resolve(&context).map_err(Error::Logic)?;
        selected.push((input.line_key.clone(), rule));
    }
    attach_owner_accounts(port, selected, executor).await
}

/// 对解析输入去重并保持首次出现顺序收集 SKU。
///
/// # 参数
/// * `inputs` - 采购责任解析输入
///
/// # 返回
/// 返回去重后的 SKU 集合。
///
/// # 错误
/// 无。
fn unique_sku_ids(inputs: &[ResolutionInput]) -> Vec<erp_core::ids::SkuId> {
    let mut unique = Vec::new();
    for input in inputs {
        if !unique.contains(&input.sku_id) {
            unique.push(input.sku_id.clone());
        }
    }
    unique
}

/// 对字符串迭代器去重并稳定排序。
///
/// # 参数
/// * `values` - 待批量查询的字符串引用
///
/// # 返回
/// 返回按字典序排序且无重复项的拥有型字符串。
///
/// # 错误
/// 无。
fn unique_strings<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values = values.map(ToOwned::to_owned).collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

/// 校验批量查询完整返回全部请求 ID。
///
/// # 参数
/// * `label` - 缺失事实的业务名称
/// * `ids` - 调用方请求的稳定 ID
/// * `map` - 仓储实际返回的 ID 映射
///
/// # 返回
/// 全部 ID 均存在时返回 `Ok(())`。
///
/// # 错误
/// 任一目录事实缺失或已删除时返回校验错误。
fn ensure_all_ids_present<T>(label: &str, ids: &[String], map: &HashMap<String, T>) -> Result<()> {
    if let Some(missing) = ids.iter().find(|id| !map.contains_key(id.as_str())) {
        return Err(Error::ValidationError(format!("{label}不存在或已删除：{missing}")));
    }
    Ok(())
}

/// 批量加载并校验候选负责人的账号状态。
async fn attach_owner_accounts(
    port: &dyn ProcurementResponsibilityFactsPort,
    selected: Vec<(String, &ProcurementResponsibilityRule)>,
    executor: &mut dyn Executor,
) -> Result<Vec<CandidateResolution>> {
    let owner_ids = unique_strings(selected.iter().map(|(_, rule)| rule.owner_user_id.as_str()));
    let accounts = port.load_owners(&owner_ids, executor).await?;
    let account_map = accounts.into_iter().map(|account| (account.id.clone(), account)).collect();
    ensure_all_ids_present("采购负责人账号", &owner_ids, &account_map)?;
    selected
        .into_iter()
        .map(|(line_key, rule)| {
            let account = account_map.get(rule.owner_user_id.as_str()).expect("完整性已校验");
            let owner = eligible_owner(account)?;
            Ok(CandidateResolution {
                line_key,
                owner_user_id: owner.user_id().to_string(),
                owner_name: owner.name().to_string(),
                rule_id: rule.base.id.clone(),
                rule_type: rule.rule_type,
            })
        })
        .collect()
}

/// 将身份提供方的窄事实验证为可登录后台负责人，展示姓名不参与资格判断。
pub fn eligible_owner(account: &IdentityOwnerFact) -> Result<EligibleProcurementOwner> {
    EligibleProcurementOwner::from_account(account)
        .map_err(|_| Error::ValidationError(format!("采购负责人 {} 必须为可登录后台账号", account.id)))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::ids::{ProcurementResponsibilityRuleId, ProductCategoryId, ProductId, SkuId};
    use mongodb::ClientSession;

    use super::*;
    use crate::entity::facts::{
        CurrentRevisionFact, FactIdentity, ProductCategoryFact, ProductFact, ProductKind,
        ProductRevisionFact, SkuFact,
    };
    use crate::entity::procurement_responsibility::{
        EnableStatus, ProcurementCatalogBundle, ProcurementResponsibilityRuleData,
    };

    struct Fixture {
        events: Mutex<Vec<&'static str>>,
        executors: Mutex<Vec<usize>>,
        catalog: ProcurementCatalogBundle,
        rules: Vec<ProcurementResponsibilityRule>,
        owners: Vec<IdentityOwnerFact>,
        failure: Option<&'static str>,
    }

    impl Fixture {
        fn new() -> Self {
            let id = |value: &str| FactIdentity { id: value.into() };
            Self {
                events: Mutex::new(Vec::new()),
                executors: Mutex::new(Vec::new()),
                catalog: ProcurementCatalogBundle {
                    skus: HashMap::from([(
                        "sku-1".into(),
                        SkuFact { base: id("sku-1"), product_id: ProductId::new("product-1") },
                    )]),
                    products: HashMap::from([(
                        "product-1".into(),
                        ProductFact {
                            base: id("product-1"),
                            stable: CurrentRevisionFact { current_revision_id: Some("revision-1".into()) },
                            product_kind: ProductKind::Physical,
                        },
                    )]),
                    revisions: HashMap::from([(
                        "revision-1".into(),
                        ProductRevisionFact { category_id: ProductCategoryId::new("category-1") },
                    )]),
                    categories: HashMap::from([("category-1".into(), ProductCategoryFact::default())]),
                },
                rules: vec![
                    ProcurementResponsibilityRule::new(
                        ProcurementResponsibilityRuleId::new("rule-1"),
                        ProcurementResponsibilityRuleData {
                            rule_type: ProcurementResponsibilityRuleType::Sku,
                            sku_id: Some(SkuId::new("sku-1")),
                            category_id: None,
                            service_region: None,
                            product_kind: None,
                            owner_user_id: "owner-1".into(),
                            status: EnableStatus::Active,
                        },
                        "actor-1",
                    )
                    .unwrap(),
                ],
                owners: vec![
                    IdentityOwnerFact::new("owner-1", "张三").with_can_login(true).with_is_admin(true),
                ],
                failure: None,
            }
        }
        fn record(&self, event: &'static str, executor: &mut dyn Executor) -> persistence_core::Result<()> {
            self.events.lock().unwrap().push(event);
            self.executors.lock().unwrap().push(executor as *mut dyn Executor as *mut () as usize);
            assert!(executor.session().is_none());
            if self.failure == Some(event) {
                return Err(persistence_core::Error::OptimisticLockingError);
            }
            Ok(())
        }
        async fn run(&self, executor: &mut dyn Executor) -> Result<Vec<CandidateResolution>> {
            resolve_candidates(
                self,
                self,
                &[
                    ResolutionInput::new("line-2".into(), SkuId::new("sku-1"), None).unwrap(),
                    ResolutionInput::new("line-1".into(), SkuId::new("sku-1"), None).unwrap(),
                ],
                executor,
            )
            .await
        }
    }

    #[async_trait]
    impl ResponsibilityRuleStore for Fixture {
        async fn active_rules(
            &self,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ProcurementResponsibilityRule>> {
            self.record("rules", executor)?;
            Ok(self.rules.clone())
        }
    }
    #[async_trait]
    impl ProcurementResponsibilityFactsPort for Fixture {
        async fn load_catalog(
            &self,
            sku_ids: &[SkuId],
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<ProcurementCatalogBundle> {
            assert_eq!(sku_ids, &[SkuId::new("sku-1")]);
            self.record("catalog", executor)?;
            Ok(self.catalog.clone())
        }
        async fn load_owners(
            &self,
            owner_ids: &[String],
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<Vec<IdentityOwnerFact>> {
            assert_eq!(owner_ids, &["owner-1".to_string()]);
            self.record("owners", executor)?;
            Ok(self.owners.clone())
        }
        async fn load_owner(
            &self,
            _: &str,
            _: &mut dyn Executor,
        ) -> persistence_core::Result<Option<IdentityOwnerFact>> {
            panic!("批量解析不得逐账号读取")
        }
        async fn sku_exists(&self, _: &SkuId, _: &mut dyn Executor) -> persistence_core::Result<bool> {
            panic!("解析不得改用单条选择器读取")
        }
        async fn category_exists(
            &self,
            _: &ProductCategoryId,
            _: &mut dyn Executor,
        ) -> persistence_core::Result<bool> {
            panic!("解析不得改用单条选择器读取")
        }
    }

    #[derive(Default)]
    struct RecordingExecutor {
        visits: usize,
    }
    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.visits += 1;
            None
        }
    }

    #[tokio::test]
    async fn candidate_resolution_preserves_order_and_executor_across_real_ports() {
        let fixture = Fixture::new();
        let mut executor = RecordingExecutor::default();
        let address = &mut executor as *mut RecordingExecutor as usize;
        let candidates = fixture.run(&mut executor).await.unwrap();
        assert_eq!(*fixture.events.lock().unwrap(), vec!["catalog", "rules", "owners"]);
        assert_eq!(*fixture.executors.lock().unwrap(), vec![address; 3]);
        assert_eq!(executor.visits, 3);
        assert_eq!(
            candidates.iter().map(|line| line.line_key.as_str()).collect::<Vec<_>>(),
            ["line-2", "line-1"]
        );
        assert!(candidates.iter().all(|line| line.owner_user_id == "owner-1"
            && line.owner_name == "张三"
            && line.rule_id == "rule-1"));
    }

    #[tokio::test]
    async fn each_failed_read_stops_before_later_reads_with_original_error_class() {
        for (failure, expected) in [
            ("catalog", vec!["catalog"]),
            ("rules", vec!["catalog", "rules"]),
            ("owners", vec!["catalog", "rules", "owners"]),
        ] {
            let mut fixture = Fixture::new();
            fixture.failure = Some(failure);
            let error = fixture.run(&mut RecordingExecutor::default()).await.unwrap_err();
            if failure == "catalog" {
                assert!(
                    matches!(error, Error::Internal(message) if message == persistence_core::Error::OptimisticLockingError.to_string())
                );
            } else {
                assert!(
                    matches!(error, Error::ConflictError(message) if message == "数据已被其他请求修改，请刷新后重试")
                );
            }
            assert_eq!(*fixture.events.lock().unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn missing_catalog_fails_before_rules_or_owner_reads() {
        let mut fixture = Fixture::new();
        fixture.catalog.revisions.clear();
        let error = fixture.run(&mut RecordingExecutor::default()).await.unwrap_err();
        assert!(
            matches!(error, Error::Logic(error) if error.to_string().contains("商品当前修订不存在或已删除：revision-1"))
        );
        assert_eq!(*fixture.events.lock().unwrap(), vec!["catalog"]);
    }

    #[tokio::test]
    async fn duplicate_input_fails_before_any_port_read() {
        let fixture = Fixture::new();
        let line = ResolutionInput::new("line-1".into(), SkuId::new("sku-1"), None).unwrap();
        let error =
            resolve_candidates(&fixture, &fixture, &[line.clone(), line], &mut RecordingExecutor::default())
                .await
                .unwrap_err();
        assert!(
            matches!(error, Error::Logic(error) if error.to_string().contains("采购责任解析行键不能重复"))
        );
        assert!(fixture.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_owner_stays_a_validation_error() {
        let mut fixture = Fixture::new();
        fixture.owners.clear();
        let error = fixture.run(&mut RecordingExecutor::default()).await.unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "采购负责人账号不存在或已删除：owner-1")
        );
    }

    #[tokio::test]
    async fn can_login_and_admin_are_independent_required_facts() {
        for (can_login, is_admin) in [(false, true), (true, false), (false, false)] {
            let mut fixture = Fixture::new();
            fixture.owners[0].can_login = can_login;
            fixture.owners[0].is_admin = is_admin;
            fixture.owners[0].name = "超级管理员".into();
            let error = fixture.run(&mut RecordingExecutor::default()).await.unwrap_err();
            assert!(
                matches!(error, Error::ValidationError(message) if message == "采购负责人 owner-1 必须为可登录后台账号")
            );
        }
    }
}
