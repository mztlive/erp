//! 采购责任解析所需的目录事实加载、规则选择与负责人资格校验。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, Executor, NoTransaction, ProcurementResponsibilityExt};
use entities::catalog::{Product, ProductCategory, ProductRevision, Sku};
use entities::procurement_responsibility::{
    build_catalog_facts, EligibleProcurementOwner, ProcurementResponsibilityContext,
    ProcurementResponsibilityResolutionBatch, ProcurementResponsibilityResolutionIdentity,
    ProcurementResponsibilityResolutionLine, ProcurementResponsibilityRuleSet,
    ProcurementResponsibilityRuleType,
};
use entities::{AccountCore, AccountKind, Permission};

use super::dto::ProcurementResponsibilityResolutionView;
use super::ProcurementResponsibilityService;
use crate::errors::{Error, Result};
use crate::iam::subject;

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 内部批量解析输入；实体负责行键与区域规范化。
pub(crate) type ResolutionInput = ProcurementResponsibilityResolutionLine;

/// 已授权的批量解析计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizedResolutionPlan {
    /// 按输入顺序排列的逐行结果。
    pub lines: Vec<ProcurementResponsibilityResolutionView>,
    /// 校验负责人权限使用的策略版本。
    pub policy_revision: u64,
}

/// 尚未经过 RBAC 校验的候选结果。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateResolution {
    line_key: String,
    owner_user_id: String,
    owner_name: String,
    rule_id: String,
    rule_type: ProcurementResponsibilityRuleType,
}

impl ProcurementResponsibilityService {
    /// 批量解析并校验每位负责人拥有采购单创建权限。
    ///
    /// # 参数
    /// * `inputs` - 稳定行键、SKU 与服务区域
    ///
    /// # 返回
    /// 返回逐行具体负责人及授权策略版本。
    ///
    /// # 错误
    /// 目录事实缺失、规则零/多命中、负责人不可登录或无采购建单权限时失败关闭。
    pub(crate) async fn resolve_strict(
        &self,
        inputs: &[ResolutionInput],
    ) -> Result<AuthorizedResolutionPlan> {
        let candidates = self.resolve_candidates(inputs, &mut NoTransaction).await?;
        self.authorize_candidates(candidates).await
    }

    /// 在销售形式化事务中重新解析并比对既定计划。
    ///
    /// # 参数
    /// * `inputs` - 原始稳定行事实
    /// * `expected` - 事务外已授权计划
    /// * `executor` - 销售形式化事务执行器
    ///
    /// # 返回
    /// 规则、目录、账号和策略版本均未变化时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一事实或策略版本变化时失败关闭，使销售生效事务回滚。
    pub(crate) async fn revalidate_plan(
        &self,
        inputs: &[ResolutionInput],
        expected: &AuthorizedResolutionPlan,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let candidates = self.resolve_candidates(inputs, executor).await?;
        let actual = candidates.into_iter().map(candidate_view).collect::<Vec<_>>();
        let actual_identity = resolution_identities(&actual)?;
        let expected_identity = resolution_identities(&expected.lines)?;
        if actual_identity != expected_identity {
            return Err(Error::ConflictError(
                "采购责任规则或目录事实已变化，请重新提交审批".to_string(),
            ));
        }
        let revision = self.rbac.policy_revision_with_executor(executor).await?;
        if revision != expected.policy_revision {
            return Err(Error::ConflictError(
                "采购负责人授权策略已变化，请重新提交审批".to_string(),
            ));
        }
        Ok(())
    }

    /// 稳定校验指定账号可作为具体采购负责人。
    ///
    /// # 参数
    /// * `owner_user_id` - 负责人账号 ID
    ///
    /// # 返回
    /// 账号可登录、为后台管理员且拥有 `purchase_order:create` 时返回稳定策略版本。
    ///
    /// # 错误
    /// 账号不存在、状态/类型不符、缺少权限或策略持续变化时返回校验错误。
    pub(crate) async fn authorize_owner_eligibility(&self, owner_user_id: &str) -> Result<u64> {
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = self.rbac.current_policy_revision().await?;
            let owner = load_owner_account(&self.db, owner_user_id, &mut NoTransaction).await?;
            ensure_purchase_create_permission(&self.rbac, &owner).await?;
            let after = self.rbac.current_policy_revision().await?;
            if before == after {
                return Ok(before);
            }
        }
        Err(Error::Rbac(
            "采购负责人授权策略持续变化，无法形成稳定快照".to_string(),
        ))
    }

    /// 批量加载目录、规则和账号事实并形成候选责任。
    ///
    /// # 参数
    /// * `inputs` - 解析输入
    /// * `executor` - 数据库执行器
    ///
    /// # 返回
    /// 返回保持输入顺序的候选责任。
    ///
    /// # 错误
    /// 输入为空、目录事实缺失、分类成环、规则冲突或账号状态不合法时返回错误。
    async fn resolve_candidates(
        &self,
        inputs: &[ResolutionInput],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CandidateResolution>> {
        let inputs = ProcurementResponsibilityResolutionBatch::new(inputs)
            .map_err(Error::Logic)?
            .lines();
        let sku_ids = unique_sku_ids(inputs);
        let bundle = self
            .db
            .load_procurement_catalog_bundle(&sku_ids, executor)
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        let facts = build_catalog_facts(
            inputs,
            &bundle.skus,
            &bundle.products,
            &bundle.revisions,
            &bundle.categories,
        )
        .map_err(Error::Logic)?;
        let rules = self
            .db
            .procurement_responsibility_rules()
            .list_active_procurement_responsibility_rules(executor)
            .await?;
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
        attach_owner_accounts(&self.db, selected, executor).await
    }

    /// 对候选负责人执行 RBAC 校验并冻结策略版本。
    ///
    /// # 参数
    /// * `candidates` - 账号状态已通过的候选责任
    ///
    /// # 返回
    /// 返回授权计划和稳定策略版本。
    ///
    /// # 错误
    /// 任一负责人缺少采购建单权限或策略无法稳定读取时返回错误。
    async fn authorize_candidates(
        &self,
        candidates: Vec<CandidateResolution>,
    ) -> Result<AuthorizedResolutionPlan> {
        let policy_revision = self.rbac.current_policy_revision().await?;
        let mut owners = HashSet::new();
        for candidate in &candidates {
            if owners.insert(candidate.owner_user_id.as_str()) {
                let permission = purchase_create_permission()?;
                if !self
                    .rbac
                    .enforce(
                        &subject(AccountKind::Admin, candidate.owner_user_id.as_str()),
                        &permission,
                    )
                    .await?
                {
                    return Err(Error::ValidationError(format!(
                        "采购负责人 {} 缺少 purchase_order:create 权限",
                        candidate.owner_user_id
                    )));
                }
            }
        }
        let after = self.rbac.current_policy_revision().await?;
        if after != policy_revision {
            return Err(Error::ConflictError(
                "采购负责人授权策略正在变化，请重试".to_string(),
            ));
        }
        Ok(AuthorizedResolutionPlan {
            lines: candidates.into_iter().map(candidate_view).collect(),
            policy_revision,
        })
    }
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
fn unique_sku_ids(inputs: &[ResolutionInput]) -> Vec<entities::ids::SkuId> {
    let mut unique = Vec::new();
    for input in inputs {
        if !unique.contains(&input.sku_id) {
            unique.push(input.sku_id.clone());
        }
    }
    unique
}

/// 提供批量目录映射所需的实体主键。
trait EntityId {
    /// 返回实体主键。
    fn entity_id(&self) -> &str;
}

impl EntityId for Sku {
    fn entity_id(&self) -> &str {
        self.base.id.as_str()
    }
}

impl EntityId for Product {
    fn entity_id(&self) -> &str {
        self.base.id.as_str()
    }
}

impl EntityId for ProductRevision {
    fn entity_id(&self) -> &str {
        self.base.id.as_str()
    }
}

impl EntityId for ProductCategory {
    fn entity_id(&self) -> &str {
        self.base.id.as_str()
    }
}

impl EntityId for AccountCore {
    fn entity_id(&self) -> &str {
        self.base.id.as_str()
    }
}

/// 按实体稳定 ID 构造批量目录映射。
///
/// # 参数
/// * `items` - 仓储批量返回的同类实体
///
/// # 返回
/// 返回实体 ID 到实体的映射。
///
/// # 错误
/// 无；重复 ID 由后出现的实体覆盖，数据库唯一约束应防止该情况。
fn by_id<T: EntityId>(items: Vec<T>) -> HashMap<String, T> {
    items
        .into_iter()
        .map(|item| (item.entity_id().to_string(), item))
        .collect()
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
        return Err(Error::ValidationError(format!(
            "{label}不存在或已删除：{missing}"
        )));
    }
    Ok(())
}

/// 批量加载并校验候选负责人的账号状态。
async fn attach_owner_accounts(
    db: &mongodb::Database,
    selected: Vec<(
        String,
        &entities::procurement_responsibility::ProcurementResponsibilityRule,
    )>,
    executor: &mut dyn Executor,
) -> Result<Vec<CandidateResolution>> {
    let owner_ids = unique_strings(selected.iter().map(|(_, rule)| rule.owner_user_id.as_str()));
    let accounts = db
        .accounts()
        .list_procurement_responsibility_owners(&owner_ids, executor)
        .await?;
    let account_map = by_id(accounts);
    ensure_all_ids_present("采购负责人账号", &owner_ids, &account_map)?;
    selected
        .into_iter()
        .map(|(line_key, rule)| {
            let account = account_map
                .get(rule.owner_user_id.as_str())
                .expect("完整性已校验");
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

/// 加载并校验单个采购负责人账号仍可登录.
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `owner_user_id` - 采购负责人账号 ID
/// * `executor` - 数据库执行器，可为规则写事务会话
///
/// # 返回
/// 返回存在、为后台管理员且可登录的负责人账号。
///
/// # 错误
/// 账号不存在、类型或状态不合格，以及仓储查询失败时返回错误。
pub(super) async fn load_owner_account(
    db: &mongodb::Database,
    owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<EligibleProcurementOwner> {
    let account = db
        .accounts()
        .find_procurement_responsibility_owner(owner_user_id, executor)
        .await?
        .ok_or_else(|| Error::ValidationError("采购负责人账号不存在".to_string()))?;
    eligible_owner(&account)
}

/// 将账号事实转换为合格采购负责人.
///
/// # 参数
/// * `account` - 仓储返回的统一账号事实
///
/// # 返回
/// 返回已验证可登录且为后台管理员的负责人值对象.
///
/// # 错误
/// 账号状态或类型不满足采购负责人约束时返回校验错误。
fn eligible_owner(account: &AccountCore) -> Result<EligibleProcurementOwner> {
    EligibleProcurementOwner::from_account(account)
        .map_err(|_| Error::ValidationError(format!("采购负责人 {} 必须为可登录后台账号", account.base.id)))
}

/// 校验单个账号拥有采购建单权限.
///
/// # 参数
/// * `rbac` - 当前应用共享的 RBAC 服务
/// * `account` - 已通过后台账号状态校验的采购负责人
///
/// # 返回
/// 账号拥有 `purchase_order:create` 时返回 `Ok(())`。
///
/// # 错误
/// 权限解析、Casbin 判定失败或账号缺少权限时返回错误。
async fn ensure_purchase_create_permission(
    rbac: &crate::iam::SharedRbacService,
    owner: &EligibleProcurementOwner,
) -> Result<()> {
    let permission = purchase_create_permission()?;
    if rbac
        .enforce(&subject(AccountKind::Admin, owner.user_id()), &permission)
        .await?
    {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "采购负责人 {} 缺少 purchase_order:create 权限",
        owner.user_id()
    )))
}

/// 构造采购建单权限值对象.
///
/// # 返回
/// 返回固定 `purchase_order:create` 权限.
///
/// # 错误
/// 固定权限代码无法解析时返回实体错误。
fn purchase_create_permission() -> Result<Permission> {
    Permission::parse("purchase_order:create").map_err(Error::Logic)
}

/// 把解析视图转换为忽略展示姓名的稳定责任身份.
///
/// # 参数
/// * `views` - 待比对的采购责任解析视图
///
/// # 返回
/// 返回保持输入顺序的稳定责任身份集合。
///
/// # 错误
/// 任一视图缺少合法行键、负责人或规则 ID 时返回实体错误。
fn resolution_identities(
    views: &[ProcurementResponsibilityResolutionView],
) -> Result<Vec<ProcurementResponsibilityResolutionIdentity>> {
    views
        .iter()
        .map(|view| {
            ProcurementResponsibilityResolutionIdentity::new(
                view.line_key.clone(),
                view.owner_user_id.clone(),
                view.rule_id.clone(),
                view.rule_type,
            )
            .map_err(Error::Logic)
        })
        .collect()
}

/// 将候选责任转换为稳定授权视图.
///
/// # 参数
/// * `candidate` - 已完成目录与账号资格校验的候选责任
///
/// # 返回
/// 返回用于授权计划和 API 预览的解析视图。
///
/// # 错误
/// 无。
fn candidate_view(candidate: CandidateResolution) -> ProcurementResponsibilityResolutionView {
    ProcurementResponsibilityResolutionView {
        line_key: candidate.line_key,
        owner_user_id: candidate.owner_user_id,
        owner_name: candidate.owner_name,
        rule_id: candidate.rule_id,
        rule_type: candidate.rule_type,
    }
}
