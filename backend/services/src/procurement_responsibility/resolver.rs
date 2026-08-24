//! 采购责任解析所需的目录事实加载、规则选择与负责人资格校验。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, CatalogExt, Executor, NoTransaction, ProcurementResponsibilityExt};
use entities::catalog::{Product, ProductCategory, ProductRevision, Sku};
use entities::ids::{ProductCategoryId, SkuId};
use entities::procurement_responsibility::{
    ProcurementResponsibilityContext, ProcurementResponsibilityRuleSet, ProcurementResponsibilityRuleType,
};
use entities::{AccountCore, AccountKind, Permission};
use mongodb::bson::doc;

use super::dto::{ProcurementResponsibilityResolutionView, ProcurementResponsibilityResolveLineRequest};
use super::ProcurementResponsibilityService;
use crate::errors::{Error, Result};

/// 内部批量解析输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolutionInput {
    /// 稳定行键；销售生效时使用稳定销售行 ID。
    pub line_key: String,
    /// 精确 SKU。
    pub sku_id: SkuId,
    /// 可选服务区域。
    pub service_region: Option<String>,
}

impl From<ProcurementResponsibilityResolveLineRequest> for ResolutionInput {
    /// 将预览行请求转换为内部解析输入。
    ///
    /// # 参数
    /// * `line` - API 预览行
    ///
    /// # 返回
    /// 返回不信任客户端分类与商品类型的内部输入。
    fn from(line: ProcurementResponsibilityResolveLineRequest) -> Self {
        Self {
            line_key: line.line_key,
            sku_id: line.sku_id,
            service_region: line.service_region,
        }
    }
}

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
        let unchanged = actual.len() == expected.lines.len()
            && actual
                .iter()
                .zip(&expected.lines)
                .all(|(actual, expected)| same_resolution_identity(actual, expected));
        if !unchanged {
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

    /// 校验指定账号可作为具体采购负责人。
    ///
    /// # 参数
    /// * `owner_user_id` - 负责人账号 ID
    ///
    /// # 返回
    /// 账号可登录、为后台管理员且拥有 `purchase_order:create` 时返回账号。
    ///
    /// # 错误
    /// 账号不存在、状态/类型不符或缺少权限时返回校验错误。
    pub(crate) async fn ensure_owner_eligible(&self, owner_user_id: &str) -> Result<AccountCore> {
        let account = self
            .db
            .accounts()
            .find_by_id(owner_user_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError("采购负责人账号不存在".to_string()))?;
        ensure_account_ready(&account)?;
        ensure_purchase_create_permission(&self.rbac, &account).await?;
        Ok(account)
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
        if inputs.is_empty() {
            return Err(Error::ValidationError("采购责任解析行不能为空".to_string()));
        }
        ensure_unique_line_keys(inputs)?;
        let facts = load_catalog_facts(&self.db, inputs, executor).await?;
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
                    .enforce(candidate.owner_user_id.as_str(), &permission)
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

/// 单行目录解析事实。
struct CatalogFact {
    category_chain: Vec<ProductCategoryId>,
    product_kind: entities::catalog::ProductKind,
}

/// 批量加载 SKU、商品、当前修订及分类父链。
async fn load_catalog_facts(
    db: &mongodb::Database,
    inputs: &[ResolutionInput],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, CatalogFact>> {
    let sku_ids = unique_strings(inputs.iter().map(|line| line.sku_id.as_ref()));
    let skus = db
        .skus()
        .find_many(doc! { "id": { "$in": &sku_ids } }, executor)
        .await?;
    let sku_map = by_id(skus);
    ensure_all_ids_present("SKU", &sku_ids, &sku_map)?;
    let product_ids = unique_strings(sku_map.values().map(|sku| sku.product_id.as_ref()));
    let products = db
        .products()
        .find_many(doc! { "id": { "$in": &product_ids } }, executor)
        .await?;
    let product_map = by_id(products);
    ensure_all_ids_present("商品", &product_ids, &product_map)?;
    let revision_ids = current_revision_ids(&product_map)?;
    let revisions = db
        .product_revisions()
        .find_many(doc! { "id": { "$in": &revision_ids } }, executor)
        .await?;
    let revision_map = by_id(revisions);
    ensure_all_ids_present("商品当前修订", &revision_ids, &revision_map)?;
    let category_ids = unique_strings(
        revision_map
            .values()
            .map(|revision| revision.category_id.as_ref()),
    );
    let categories = load_category_ancestors(db, category_ids, executor).await?;
    build_catalog_facts(inputs, &sku_map, &product_map, &revision_map, &categories)
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

/// 按实体 ID 构造映射。
fn by_id<T: EntityId>(items: Vec<T>) -> HashMap<String, T> {
    items
        .into_iter()
        .map(|item| (item.entity_id().to_string(), item))
        .collect()
}

/// 对字符串迭代器去重并稳定排序。
fn unique_strings<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values = values.map(ToOwned::to_owned).collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

/// 校验批量查询完整返回全部请求 ID。
fn ensure_all_ids_present<T>(label: &str, ids: &[String], map: &HashMap<String, T>) -> Result<()> {
    if let Some(missing) = ids.iter().find(|id| !map.contains_key(id.as_str())) {
        return Err(Error::ValidationError(format!(
            "{label}不存在或已删除：{missing}"
        )));
    }
    Ok(())
}

/// 提取全部商品当前修订 ID。
fn current_revision_ids(products: &HashMap<String, Product>) -> Result<Vec<String>> {
    let mut ids = Vec::with_capacity(products.len());
    for product in products.values() {
        let revision_id = product
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::ValidationError(format!("商品 {} 没有当前修订", product.base.id)))?;
        ids.push(revision_id.to_string());
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// 分层批量加载当前分类及全部父分类，并检测父级环。
async fn load_category_ancestors(
    db: &mongodb::Database,
    initial_ids: Vec<String>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, ProductCategory>> {
    let mut categories = HashMap::new();
    let mut pending = initial_ids;
    while !pending.is_empty() {
        let rows = db
            .product_categories()
            .find_many(doc! { "id": { "$in": &pending } }, executor)
            .await?;
        let row_map = by_id(rows);
        ensure_all_ids_present("商品分类", &pending, &row_map)?;
        pending = row_map
            .values()
            .filter_map(|category| category.parent_category_id.as_ref())
            .map(ToString::to_string)
            .filter(|id| !categories.contains_key(id))
            .collect();
        pending.sort();
        pending.dedup();
        categories.extend(row_map);
    }
    Ok(categories)
}

/// 由批量目录映射构造每行解析事实。
fn build_catalog_facts(
    inputs: &[ResolutionInput],
    skus: &HashMap<String, Sku>,
    products: &HashMap<String, Product>,
    revisions: &HashMap<String, ProductRevision>,
    categories: &HashMap<String, ProductCategory>,
) -> Result<HashMap<String, CatalogFact>> {
    let mut facts = HashMap::with_capacity(inputs.len());
    for input in inputs {
        let sku = skus.get(input.sku_id.as_ref()).expect("完整性已校验");
        let product = products.get(sku.product_id.as_ref()).expect("完整性已校验");
        let revision_id = product
            .stable
            .current_revision_id
            .as_deref()
            .expect("完整性已校验");
        let revision = revisions.get(revision_id).expect("完整性已校验");
        let category_chain = category_chain(&revision.category_id, categories)?;
        facts.insert(
            input.line_key.clone(),
            CatalogFact {
                category_chain,
                product_kind: product.product_kind,
            },
        );
    }
    Ok(facts)
}

/// 构造当前分类到根分类的有序链并检测环。
fn category_chain(
    first: &ProductCategoryId,
    categories: &HashMap<String, ProductCategory>,
) -> Result<Vec<ProductCategoryId>> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(first.clone());
    while let Some(category_id) = current {
        if !seen.insert(category_id.to_string()) {
            return Err(Error::ConflictError("商品分类父级关系存在环".to_string()));
        }
        let category = categories
            .get(category_id.as_ref())
            .ok_or_else(|| Error::ValidationError(format!("商品分类不存在：{category_id}")))?;
        chain.push(category_id);
        current = category.parent_category_id.clone();
    }
    Ok(chain)
}

/// 批量加载并校验候选负责人的账号状态。
async fn attach_owner_accounts<'a>(
    db: &mongodb::Database,
    selected: Vec<(
        String,
        &'a entities::procurement_responsibility::ProcurementResponsibilityRule,
    )>,
    executor: &mut dyn Executor,
) -> Result<Vec<CandidateResolution>> {
    let owner_ids = unique_strings(selected.iter().map(|(_, rule)| rule.owner_user_id.as_str()));
    let accounts = db
        .accounts()
        .find_many(doc! { "id": { "$in": &owner_ids } }, executor)
        .await?;
    let account_map = by_id(accounts);
    ensure_all_ids_present("采购负责人账号", &owner_ids, &account_map)?;
    selected
        .into_iter()
        .map(|(line_key, rule)| {
            let account = account_map
                .get(rule.owner_user_id.as_str())
                .expect("完整性已校验");
            ensure_account_ready(account)?;
            Ok(CandidateResolution {
                line_key,
                owner_user_id: account.base.id.clone(),
                owner_name: account.name.clone(),
                rule_id: rule.base.id.clone(),
                rule_type: rule.rule_type,
            })
        })
        .collect()
}

/// 校验账号为可登录的后台管理员。
fn ensure_account_ready(account: &AccountCore) -> Result<()> {
    if account.can_login() && account.is_kind(AccountKind::Admin) {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "采购负责人 {} 必须为可登录后台账号",
        account.base.id
    )))
}

/// 校验单个账号拥有采购建单权限。
async fn ensure_purchase_create_permission(
    rbac: &crate::iam::SharedRbacService,
    account: &AccountCore,
) -> Result<()> {
    let permission = purchase_create_permission()?;
    if rbac.enforce(account.base.id.as_str(), &permission).await? {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "采购负责人 {} 缺少 purchase_order:create 权限",
        account.base.id
    )))
}

/// 构造采购建单权限值对象。
fn purchase_create_permission() -> Result<Permission> {
    Permission::parse("purchase_order:create").map_err(Error::Logic)
}

/// 校验内部行键唯一。
fn ensure_unique_line_keys(inputs: &[ResolutionInput]) -> Result<()> {
    let mut keys = HashSet::with_capacity(inputs.len());
    if inputs.iter().any(|input| !keys.insert(input.line_key.as_str())) {
        return Err(Error::ValidationError("采购责任解析行键不能重复".to_string()));
    }
    Ok(())
}

/// 比较两条解析结果的稳定责任身份，忽略可变展示名称。
fn same_resolution_identity(
    left: &ProcurementResponsibilityResolutionView,
    right: &ProcurementResponsibilityResolutionView,
) -> bool {
    left.line_key == right.line_key
        && left.owner_user_id == right.owner_user_id
        && left.rule_id == right.rule_id
        && left.rule_type == right.rule_type
}

/// 将候选责任转换为稳定授权视图。
fn candidate_view(candidate: CandidateResolution) -> ProcurementResponsibilityResolutionView {
    ProcurementResponsibilityResolutionView {
        line_key: candidate.line_key,
        owner_user_id: candidate.owner_user_id,
        owner_name: candidate.owner_name,
        rule_id: candidate.rule_id,
        rule_type: candidate.rule_type,
    }
}
