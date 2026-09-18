//! 双口径一致快照装载：授权客户集合、订单事实与版本指纹。
//!
//! 任一来源失败时整体失败；超限整体拒绝，不截断汇总或导出。

use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};
use erp_core::ids::{CustomerAccountId, PartyId};
use erp_customer::repository::prelude::*;
use erp_customer::{AssignmentRole, CustomerExt};
use erp_identity::AccessControlExt;
use erp_identity::repository::OrganizationRepository;
use erp_identity::repository::prelude::*;
use erp_party::PartyExt;
use erp_sales::entity::sales_order::SalesAttribution;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::quality::QualityOrderFilter;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use mongodb::Database;
use persistence_core::Executor;
use rust_decimal::Decimal;

use crate::{Error, Result};

/// 单次分析装载的订单上限；与销售域仓储的有界查询共用同一限额。
pub(super) const QUALITY_ORDER_LIMIT: usize =
    erp_sales::repository::sales_order::quality::QUALITY_ORDER_LIMIT;

/// 快照装载的销售行：正式版本金额与冻结归属的最小集合。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct QualityOrder {
    pub id: String,
    pub order_no: String,
    pub version: u64,
    pub customer_id: String,
    pub current_revision_id: Option<String>,
    pub effective_at: Option<i64>,
    pub sales_owner_user_id: String,
    pub business_org_unit_id: String,
    pub attribution: Option<SalesAttribution>,
}

/// 当前口径快照：现任客户事实与期间订单。
pub(super) struct CurrentSnapshot {
    pub customers: Vec<CustomerFact>,
    pub orders: Vec<QualityOrder>,
    pub revision_gross: BTreeMap<String, Decimal>,
    pub org_names: BTreeMap<String, String>,
    pub account_names: BTreeMap<String, String>,
}

/// 历史口径快照：冻结归属订单与展示名。
#[allow(dead_code)]
pub(super) struct HistorySnapshot {
    pub orders: Vec<QualityOrder>,
    pub revision_gross: BTreeMap<String, Decimal>,
    pub customer_names: BTreeMap<String, String>,
    pub account_names: BTreeMap<String, String>,
    pub org_names: BTreeMap<String, String>,
}

/// 兼容旧名的当前快照装载器。
pub(super) struct CurrentSources;

impl CurrentSources {
    /// 有界装载期间正式销售单；调用方必须对超限结果整体拒绝。
    pub async fn load_orders(
        db: &Database,
        filter: QualityOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<QualityOrder>> {
        load_orders(db, filter, executor).await
    }

    /// 按明确客户集合批量读取现任归属，并附带订单金额与展示名。
    pub async fn load_customers(
        db: &Database,
        customer_ids: Vec<String>,
        orders: &[QualityOrder],
        executor: &mut dyn Executor,
    ) -> Result<CurrentSnapshot> {
        let customers = load_customer_facts(db, Some(&customer_ids), executor).await?;
        let revision_gross = load_revision_gross(db, orders, executor).await?;
        let (org_names, account_names) = names_for_current(db, &customers, executor).await?;
        Ok(CurrentSnapshot { customers, orders: orders.to_vec(), revision_gross, org_names, account_names })
    }
}

/// 历史快照装载器。
impl HistorySnapshot {
    /// 装载授权销售单与客户展示名；客户展示只用于标签，不构成授权。
    pub async fn load(
        db: &Database,
        order_filter: QualityOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<HistorySnapshot> {
        let orders = load_orders(db, order_filter, executor).await?;
        let revision_gross = load_revision_gross(db, &orders, executor).await?;
        let customer_names = load_customer_names(
            db,
            &orders.iter().map(|o| o.customer_id.clone()).collect::<Vec<_>>(),
            executor,
        )
        .await?;
        let mut user_ids = BTreeSet::new();
        let mut org_ids = BTreeSet::new();
        for order in &orders {
            if let Some(attribution) = &order.attribution {
                user_ids.insert(attribution.attribution_user_id.clone());
                org_ids.insert(attribution.attribution_org_unit_id.clone());
                for node in &attribution.org_path {
                    org_ids.insert(node.id.clone());
                }
            }
        }
        let account_names = names_by_ids(db, &user_ids.into_iter().collect::<Vec<_>>(), executor).await?;
        let org_names = org_names_by_ids(db, &org_ids.into_iter().collect::<Vec<_>>(), executor).await?;
        Ok(HistorySnapshot { orders, revision_gross, customer_names, account_names, org_names })
    }
}

/// 客户现任归属事实：展示名与现任主责、组织。
#[derive(Debug, Clone)]
pub(super) struct CustomerFact {
    pub id: String,
    pub customer_no: String,
    pub name: String,
    pub owner_user_id: Option<String>,
    pub owner_org_unit_id: Option<String>,
}

/// 按明确客户集合批量读取现任归属与法定名称；空集合不访问数据库。
async fn load_customer_facts(
    db: &Database,
    customer_ids: Option<&[String]>,
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerFact>> {
    let Some(ids) = customer_ids else {
        return Ok(Vec::new());
    };
    if ids.len() > QUALITY_ORDER_LIMIT {
        return Err(Error::ValidationError("客户查询超过上限，请收窄组织或负责人条件".into()));
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut unique = ids.to_vec();
    unique.sort();
    unique.dedup();
    let typed = unique.iter().map(|id| CustomerAccountId::new(id.clone())).collect::<Vec<_>>();
    let accounts = db.customer_accounts().find_accounts_by_ids(&typed, executor).await?;
    let party_ids: Vec<PartyId> = accounts.iter().map(|a| a.party_id.clone()).collect();
    let names = db.party().current_legal_names_by_party_ids(&party_ids, executor).await?;
    let active =
        db.customer_assignments().list_active_for_customers(&unique, today_shanghai(), executor).await?;
    let owners = active
        .iter()
        .filter(|a| a.assignment_role == AssignmentRole::Owner)
        .map(|a| (a.customer_id.to_string(), a.user_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let orgs = owner_orgs(db, owners.values().cloned().collect(), executor).await?;
    Ok(accounts
        .into_iter()
        .map(|account| {
            let id = account.base.id.clone();
            let owner = owners.get(&id).cloned();
            let org = owner.as_ref().and_then(|user| orgs.get(user).cloned());
            CustomerFact {
                name: names
                    .get(&account.party_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| account.customer_no.clone()),
                customer_no: account.customer_no.clone(),
                id,
                owner_org_unit_id: org,
                owner_user_id: owner,
            }
        })
        .collect())
}

/// 按客户 ID 批量读取当前法定名称；缺失主体不补行。
async fn load_customer_names(
    db: &Database,
    customer_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<BTreeMap<String, String>> {
    if customer_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut unique = customer_ids.to_vec();
    unique.sort();
    unique.dedup();
    let typed = unique.iter().map(|id| CustomerAccountId::new(id.clone())).collect::<Vec<_>>();
    let accounts = db.customer_accounts().find_accounts_by_ids(&typed, executor).await?;
    let party_ids: Vec<PartyId> = accounts.iter().map(|a| a.party_id.clone()).collect();
    let names = db.party().current_legal_names_by_party_ids(&party_ids, executor).await?;
    Ok(accounts
        .into_iter()
        .map(|account| {
            let name = names
                .get(&account.party_id.to_string())
                .cloned()
                .unwrap_or_else(|| account.customer_no.clone());
            (account.base.id.clone(), name)
        })
        .collect())
}

/// 当前口径展示所需的组织与人员名称；缺失不补行。
async fn names_for_current(
    db: &Database,
    customers: &[CustomerFact],
    executor: &mut dyn Executor,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    let mut org_ids = BTreeSet::new();
    let mut user_ids = BTreeSet::new();
    for customer in customers {
        if let Some(org) = &customer.owner_org_unit_id {
            org_ids.insert(org.clone());
        }
        if let Some(user) = &customer.owner_user_id {
            user_ids.insert(user.clone());
        }
    }
    let org_names = org_names_by_ids(db, &org_ids.into_iter().collect::<Vec<_>>(), executor).await?;
    let account_names = names_by_ids(db, &user_ids.into_iter().collect::<Vec<_>>(), executor).await?;
    Ok((org_names, account_names))
}

/// 批量读取账号展示名。
async fn names_by_ids(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<BTreeMap<String, String>> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(db.accounts().names_by_ids(ids, executor).await?.into_iter().collect())
}

/// 按组织 ID 批量读取启用组织名称；停用或缺失不补行。
async fn org_names_by_ids(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<BTreeMap<String, String>> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let state = OrganizationRepository::new(db).state(executor).await?;
    Ok(state
        .units
        .iter()
        .filter(|unit| ids.contains(&unit.base.id))
        .map(|unit| (unit.base.id.clone(), unit.name.clone()))
        .collect())
}

/// 批量读取账号在当前组织快照中的主属组织；账号缺主属组织时该客户归未知组织。
async fn owner_orgs(
    db: &Database,
    users: Vec<String>,
    executor: &mut dyn Executor,
) -> Result<BTreeMap<String, String>> {
    let state = OrganizationRepository::new(db).state(executor).await?;
    let now = erp_core::common::time::Instant::now();
    let mut result = BTreeMap::new();
    for user in users {
        let org = state.own_org(&user, now).map_err(Error::from)?.map(str::to_string);
        if let Some(org) = org {
            result.insert(user, org);
        }
    }
    Ok(result)
}

/// 有界装载期间正式销售单；调用方必须对超限结果整体拒绝。
async fn load_orders(
    db: &Database,
    filter: QualityOrderFilter,
    executor: &mut dyn Executor,
) -> Result<Vec<QualityOrder>> {
    let rows = db.sales_orders().quality_orders(&filter, executor).await?;
    if rows.len() > QUALITY_ORDER_LIMIT {
        return Err(Error::ValidationError("匹配销售单超过 10000 单，请缩小期间或指定客户".into()));
    }
    Ok(rows
        .into_iter()
        .map(|order| QualityOrder {
            effective_at: order.effective_at.map(|at| at.as_utc().timestamp()),
            id: order.base.id.clone(),
            version: order.base.version,
            order_no: order.order_no.clone(),
            customer_id: order.customer_id.to_string(),
            current_revision_id: order.stable.current_revision_id.clone(),
            sales_owner_user_id: order.sales_owner_user_id.clone(),
            business_org_unit_id: order.business_org_unit_id.clone(),
            attribution: order.attribution.clone(),
        })
        .collect())
}

/// 以批次读取当前正式版本含税金额；缺失版本在汇总时单列，不写作零。
async fn load_revision_gross(
    db: &Database,
    orders: &[QualityOrder],
    executor: &mut dyn Executor,
) -> Result<BTreeMap<String, Decimal>> {
    let ids: BTreeSet<_> = orders.iter().filter_map(|o| o.current_revision_id.clone()).collect();
    let mut gross = BTreeMap::new();
    for chunk in ids.into_iter().collect::<Vec<_>>().chunks(500) {
        let revisions = db.sales_order_revisions().find_revisions_by_ids(chunk, executor).await?;
        for revision in revisions {
            gross.insert(revision.base.id.clone(), revision.gross_amount.to_decimal());
        }
    }
    Ok(gross)
}

/// 两口径共用的订单过滤条件：生效正式非删除单 + 销售范围 + 客户交集。
/// 条件编译在销售域仓储内完成；读模型只传递明确的授权与业务筛选。
pub(super) fn quality_order_filter(
    bounds: &super::query::PeriodBounds,
    customer_ids: Option<Vec<String>>,
    scope: &SalesReadScope,
) -> QualityOrderFilter {
    QualityOrderFilter {
        from: bounds.from,
        until: bounds.until,
        customer_ids,
        authorized_scope: scope.clone(),
    }
}

/// 查询版本包含完整订单集合及责任版本，不返回订单身份集合。
pub(super) fn version(scope_version: &str, orders: &[QualityOrder]) -> String {
    use std::hash::{Hash, Hasher};
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    let versions = orders.iter().map(|o| (&o.id, o.version)).collect::<BTreeMap<_, _>>();
    versions.hash(&mut fingerprint);
    format!("{scope_version}:{:x}", fingerprint.finish())
}

/// 客户归属自然日使用授权上下文的同一时点，避免跨上海零点混用两天资格。
#[allow(dead_code)]
pub(super) fn business_date(
    at: erp_core::common::time::Instant,
) -> Result<erp_core::common::time::BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
}

/// 组织快照的当日自然日；现任归属展示使用当日有效窗口。
fn today_shanghai() -> erp_core::common::time::BusinessDate {
    let now = Utc::now() + chrono::Duration::hours(8);
    now.format("%Y-%m-%d").to_string().parse().expect("合法业务日期")
}

/// 生效秒级时点转上海日期展示；缺失时点不伪造日期。
pub(super) fn effective_label(value: Option<i64>) -> Option<String> {
    value.map(|secs| {
        (Utc.timestamp_opt(secs, 0).single().unwrap_or_default() + chrono::Duration::hours(8))
            .format("%Y-%m-%d")
            .to_string()
    })
}
