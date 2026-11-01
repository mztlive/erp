use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ports::connector::common::SupplierSku;
use crate::{Error, Result};

/// 来源账号与本次完整授权商品池集合；不能由单页缺项推导授权减少。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPoolScope {
    pub account_scope: String,
    pub pool_ids: Vec<String>,
}

impl CatalogPoolScope {
    /// 规范化一个完整的来源授权范围。
    /// # 参数
    /// `account_scope` 为部署账号命名空间，`pool_ids` 为完整商品池列表。
    /// # 返回
    /// 排序且去重的范围，空池集合保留为来源明确的空范围。
    /// # 错误
    /// 账号、池身份或范围大小无效时拒绝。
    pub fn new(account_scope: String, mut pool_ids: Vec<String>) -> Result<Self> {
        pool_ids.sort();
        pool_ids.dedup();
        let scope = Self { account_scope, pool_ids };
        scope.validate()?;
        Ok(scope)
    }

    /// 核对已经规范化的完整授权范围。
    /// # 参数
    /// 无。
    /// # 返回
    /// 账号与严格递增池集合满足边界时成功。
    /// # 错误
    /// 未排序、重复、空身份或超限时拒绝。
    pub fn validate(&self) -> Result<()> {
        if !identifier(&self.account_scope)
            || self.pool_ids.len() > 1000
            || self.pool_ids.iter().any(|id| !identifier(id))
            || self.pool_ids.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(Error::ValidationError("供应商授权商品池范围无效".into()));
        }
        Ok(())
    }
}

/// 一页成员身份及来源翻页事实；末页须另行重验完整账号范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogMembershipPage {
    pub scope: CatalogPoolScope,
    pub pool_id: Option<String>,
    pub offset: String,
    pub next_offset: Option<String>,
    pub pool_finished: bool,
    pub scan_finished: bool,
    pub confirmed_scope: Option<CatalogPoolScope>,
    pub members: Vec<SupplierSku>,
}

impl CatalogMembershipPage {
    /// 核对单页来源证据，不把本地任务结束当作外部扫描完成。
    /// # 参数
    /// 无。
    /// # 返回
    /// 范围、来源池和成员身份满足边界时成功。
    /// # 错误
    /// 不完整翻页事实、越范围池或成员身份无效时拒绝。
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        if let Some(scope) = &self.confirmed_scope {
            scope.validate()?;
        }
        if !identifier(&self.offset)
            || self.members.len() > 1000
            || self.members.iter().any(|sku| {
                !identifier(&sku.spec_id) || sku.product_id.as_deref().is_some_and(|id| !identifier(id))
            })
            || self.next_offset.as_deref().is_some_and(|offset| !identifier(offset) || offset == self.offset)
            || self.pool_finished == self.next_offset.is_some()
            || (!self.scan_finished && self.confirmed_scope.is_some())
            || (self.scan_finished && !self.pool_finished)
        {
            return Err(Error::ValidationError("商品池页面或成员证据不完整".into()));
        }
        self.validate_pool()
    }

    fn validate_pool(&self) -> Result<()> {
        if self.scope.pool_ids.is_empty() {
            if self.pool_id.is_some() || !self.members.is_empty() || !self.scan_finished {
                return Err(Error::ValidationError("空商品池范围不能携带成员或后续页面".into()));
            }
        } else if self.pool_id.as_ref().is_none_or(|pool| !self.scope.pool_ids.contains(pool)) {
            return Err(Error::ValidationError("成员页面不属于已冻结的商品池范围".into()));
        }
        Ok(())
    }
}

/// 完整范围只在全部页面闭合且末次范围一致时成立。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CatalogMembershipState {
    Scanning,
    Complete,
    Uncertain,
    Reconciled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogMembershipScan {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub supplier_id: String,
    pub scope: CatalogPoolScope,
    pub next_pool_index: u32,
    pub next_offset: String,
    pub pages: u64,
    pub state: CatalogMembershipState,
    /// 部分唯一索引围栏：同账号连接一次只允许一个扫描推进或核对。
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub negative_allowed: bool,
    #[serde(default)]
    pub completed_at: Option<Instant>,
}

impl CatalogMembershipScan {
    /// 核对扫描永久绑定的连接、供应商和账号范围。
    /// # 参数
    /// 所有期望身份来自服务端当前任务及已绑定连接。
    /// # 返回
    /// 归属与完整范围精确匹配时成功。
    /// # 错误
    /// 跨连接、跨供应商或账号范围变化时拒绝复用。
    pub fn validate_binding(&self, connection: &str, supplier: &str, scope: &CatalogPoolScope) -> Result<()> {
        if self.connection_id != connection || self.supplier_id != supplier || &self.scope != scope {
            return Err(Error::ConflictError("目录池扫描账号、连接或供应商归属已变化".into()));
        }
        Ok(())
    }

    /// 建立账号及连接专属的扫描证据，不修改任何旧成员状态。
    /// # 参数
    /// 身份均来自服务端任务和已绑定连接，`scope` 来自首次完整池列表。
    /// # 返回
    /// 从首池零偏移开始的扫描记录。
    /// # 错误
    /// 任务、归属或范围无效时拒绝。
    pub fn new(job: &str, connection: &str, supplier: &str, scope: CatalogPoolScope) -> Result<Self> {
        scope.validate()?;
        if !identifier(job) || !identifier(connection) || !identifier(supplier) {
            return Err(Error::ValidationError("商品池扫描任务或归属无效".into()));
        }
        Ok(Self {
            base: BaseModel::new(job.into()),
            connection_id: connection.into(),
            supplier_id: supplier.into(),
            scope,
            next_pool_index: 0,
            next_offset: "0".into(),
            pages: 0,
            state: CatalogMembershipState::Scanning,
            active: true,
            negative_allowed: false,
            completed_at: None,
        })
    }

    /// 按冻结池顺序及来源偏移应用单页；失败不改变原扫描进度。
    /// # 参数
    /// `page` 为适配器本页证据，`at` 为成功观察时刻。
    /// # 返回
    /// 更新下一池及偏移，末页范围不一致时保留为 Uncertain。
    /// # 错误
    /// 跳页、重复页、范围变化或未完整闭合的完成声明时拒绝。
    pub fn observe_page(&mut self, page: &CatalogMembershipPage, at: Instant) -> Result<()> {
        page.validate()?;
        if !self.active
            || self.state != CatalogMembershipState::Scanning
            || page.scope != self.scope
            || page.offset != self.next_offset
            || page.pool_id.as_ref()
                != self.scope.pool_ids.get(usize::try_from(self.next_pool_index).map_err(|_| range_error())?)
        {
            return Err(Error::ConflictError("商品池页面与原检查点不一致，请恢复原页面".into()));
        }
        let next_pool = self
            .next_pool_index
            .checked_add(u32::from(page.pool_finished && !self.scope.pool_ids.is_empty()))
            .ok_or_else(range_error)?;
        let finished = usize::try_from(next_pool).map_err(|_| range_error())? == self.scope.pool_ids.len();
        if finished != page.scan_finished {
            return Err(Error::ConflictError("商品池范围尚未完整扫描，不能声明结束".into()));
        }
        let pages = self.pages.checked_add(1).ok_or_else(range_error)?;
        self.next_pool_index = next_pool;
        self.next_offset = page.next_offset.clone().unwrap_or_else(|| "0".into());
        self.pages = pages;
        if finished {
            self.state = if page.confirmed_scope.as_ref() == Some(&self.scope) {
                CatalogMembershipState::Complete
            } else {
                CatalogMembershipState::Uncertain
            };
            self.active = self.state == CatalogMembershipState::Complete;
            self.completed_at = Some(at);
        }
        Ok(())
    }

    /// 仅在完整扫描与上一确认授权范围一致时允许负向核对。
    /// # 参数
    /// `previous` 为同账号连接最近已完成核对的完整范围，首次可缺失。
    /// # 返回
    /// 保持完整或转为授权范围待核实；不修改成员或目录。
    /// # 错误
    /// 不返回错误，不完整范围永远不能授权撤回。
    pub fn authorize_reconciliation(&mut self, previous: Option<&CatalogPoolScope>) {
        self.negative_allowed = self.state == CatalogMembershipState::Complete
            && previous.is_none_or(|scope| scope == &self.scope);
        if self.state == CatalogMembershipState::Complete && !self.negative_allowed {
            self.state = CatalogMembershipState::Uncertain;
            self.active = false;
        }
    }

    /// 判定本扫描是否具备负向核对资格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整且已核对授权范围时返回真。
    /// # 错误
    /// 不返回错误。
    pub fn can_reconcile(&self) -> bool {
        !self.base.is_deleted()
            && self.active
            && self.state == CatalogMembershipState::Complete
            && self.negative_allowed
    }

    /// 完成本轮负向核对，建立下一轮可比较的明确基线。
    /// # 参数
    /// 无；调用方须在同一事务证明没有剩余待核对成员。
    /// # 返回
    /// 状态变为 Reconciled。
    /// # 错误
    /// 无完整范围资格时拒绝。
    pub fn finish_reconciliation(&mut self) -> Result<()> {
        if !self.can_reconcile() {
            return Err(Error::ConflictError("商品池范围未核实，不能完成负向核对".into()));
        }
        self.state = CatalogMembershipState::Reconciled;
        self.active = false;
        Ok(())
    }

    /// 放弃失败扫描，仅解除本地推进围栏，不撤回已观察成员。
    /// # 参数
    /// 无；发布交接后计划恢复的任务不得调用此方法。
    /// # 返回
    /// 未完成核对的扫描变为 Uncertain 并保留全部页面证据。
    /// # 错误
    /// 不返回错误，已建立的 Reconciled 基线保持不变。
    pub fn abort(&mut self) {
        if self.state != CatalogMembershipState::Reconciled {
            self.state = CatalogMembershipState::Uncertain;
            self.active = false;
            self.negative_allowed = false;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CatalogPoolMember {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub supplier_id: String,
    pub account_scope: String,
    pub pool_id: String,
    pub sku: SupplierSku,
    pub last_seen_run: String,
    pub observed_at: Instant,
    pub active: bool,
}

impl CatalogPoolMember {
    /// 取得同账号连接、池与订货规格的稳定成员身份。
    /// # 参数
    /// 账号、连接、池与规格均为已经校验的来源身份。
    /// # 返回
    /// 用于持久化及唯一去重的固定标识，不使用商品名称。
    /// # 错误
    /// 不返回错误；构造入口仍负责身份校验。
    pub fn identity(connection: &str, account: &str, pool: &str, spec: &str) -> String {
        let mut hash = Sha256::new();
        for value in [connection, account, pool, spec] {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
        format!("catalog-pool-member-{}", hex::encode(hash.finalize()))
    }

    /// 建立本页明确观察到的正向成员事实。
    /// # 参数
    /// `scan` 为本页所属扫描，`pool` 为已核实池，`sku` 为订货身份。
    /// # 返回
    /// 当前可见且归属唯一的成员记录。
    /// # 错误
    /// 成员越范围或来源身份无效时拒绝。
    pub fn observed(scan: &CatalogMembershipScan, pool: &str, sku: SupplierSku, at: Instant) -> Result<Self> {
        if !scan.scope.pool_ids.iter().any(|id| id == pool)
            || !identifier(&sku.spec_id)
            || sku.product_id.as_deref().is_some_and(|id| !identifier(id))
        {
            return Err(Error::ValidationError("商品池成员不属于本轮来源范围".into()));
        }
        Ok(Self {
            base: BaseModel::new(Self::identity(
                &scan.connection_id,
                &scan.scope.account_scope,
                pool,
                &sku.spec_id,
            )),
            connection_id: scan.connection_id.clone(),
            supplier_id: scan.supplier_id.clone(),
            account_scope: scan.scope.account_scope.clone(),
            pool_id: pool.into(),
            sku,
            last_seen_run: scan.base.id.clone(),
            observed_at: at,
            active: true,
        })
    }

    /// 判断成员是否属于完整范围内本轮未见的旧记录。
    /// # 参数
    /// `scan` 为完成范围复核的当前扫描。
    /// # 返回
    /// 满足同账号、连接、范围及未观察条件时为真。
    /// # 错误
    /// 不返回错误；未知范围一律为假。
    pub fn retirement_candidate(&self, scan: &CatalogMembershipScan) -> bool {
        !self.base.is_deleted()
            && scan.can_reconcile()
            && self.active
            && self.connection_id == scan.connection_id
            && self.supplier_id == scan.supplier_id
            && self.account_scope == scan.scope.account_scope
            && scan.scope.pool_ids.contains(&self.pool_id)
            && self.last_seen_run != scan.base.id
    }

    /// 判断同订货规格是否在本轮任一授权池明确观察到。
    /// # 参数
    /// `scan` 为已冻结的当前账号范围，`spec_id` 为待核对订货身份。
    /// # 返回
    /// 本轮正向成员归属和池范围均一致时为真。
    /// # 错误
    /// 不返回错误；其他账号、旧轮次及失活成员不能提供当前覆盖。
    pub fn observed_in(&self, scan: &CatalogMembershipScan, spec_id: &str) -> bool {
        !self.base.is_deleted()
            && self.active
            && self.connection_id == scan.connection_id
            && self.supplier_id == scan.supplier_id
            && self.account_scope == scan.scope.account_scope
            && self.sku.spec_id == spec_id
            && self.last_seen_run == scan.base.id
            && scan.scope.pool_ids.contains(&self.pool_id)
    }
}

fn identifier(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim() == value
        && value.len() <= 512
        && !value.chars().any(char::is_control)
}
fn range_error() -> Error {
    Error::ValidationError("商品池扫描进度超出范围".into())
}

#[cfg(test)]
mod tests {
    use serde_json::{from_value, to_value};

    use super::*;

    fn scope(pools: &[&str]) -> CatalogPoolScope {
        CatalogPoolScope::new("account-1".into(), pools.iter().map(|id| (*id).into()).collect()).unwrap()
    }

    fn scan(pools: &[&str]) -> CatalogMembershipScan {
        let mut scan = CatalogMembershipScan::new("job-1", "connection", "supplier", scope(pools)).unwrap();
        scan.base = BaseModel { id: "job-1".into(), ..BaseModel::fake() };
        scan
    }

    fn page(
        pools: &[&str],
        pool: Option<&str>,
        offset: &str,
        next: Option<&str>,
        final_page: bool,
    ) -> CatalogMembershipPage {
        CatalogMembershipPage {
            scope: scope(pools),
            pool_id: pool.map(str::to_owned),
            offset: offset.into(),
            next_offset: next.map(str::to_owned),
            pool_finished: next.is_none(),
            scan_finished: final_page,
            confirmed_scope: final_page.then(|| scope(pools)),
            members: vec![],
        }
    }

    fn complete_scan(pools: &[&str]) -> CatalogMembershipScan {
        let mut scan = scan(pools);
        if pools.is_empty() {
            scan.observe_page(&page(pools, None, "0", None, true), Instant::from_unix_secs(10)).unwrap();
        } else {
            for (index, pool) in pools.iter().enumerate() {
                scan.observe_page(
                    &page(pools, Some(pool), "0", None, index + 1 == pools.len()),
                    Instant::from_unix_secs(10),
                )
                .unwrap();
            }
        }
        scan.authorize_reconciliation(None);
        scan
    }

    fn sku(spec: &str) -> SupplierSku {
        SupplierSku { product_id: None, spec_id: spec.into() }
    }

    #[test]
    fn scope_is_canonical_and_does_not_accept_ambiguous_raw_sets() {
        assert_eq!(
            CatalogPoolScope::new("account".into(), vec!["b".into(), "a".into(), "a".into()])
                .unwrap()
                .pool_ids,
            ["a", "b"]
        );
        assert!(
            CatalogPoolScope { account_scope: "account".into(), pool_ids: vec!["b".into(), "a".into()] }
                .validate()
                .is_err()
        );
        assert!(CatalogPoolScope::new(" ".into(), vec![]).is_err());
    }

    #[test]
    fn empty_page_with_next_retains_progress_and_requires_every_pool() {
        let mut scan = scan(&["a", "b"]);
        scan.observe_page(&page(&["a", "b"], Some("a"), "0", Some("100"), false), Instant::from_unix_secs(1))
            .unwrap();
        assert_eq!(scan.next_offset, "100");
        assert_eq!(scan.next_pool_index, 0);
        assert!(!scan.can_reconcile());
        scan.observe_page(&page(&["a", "b"], Some("a"), "100", None, false), Instant::from_unix_secs(2))
            .unwrap();
        assert_eq!(scan.next_pool_index, 1);
        assert_eq!(scan.next_offset, "0");
        scan.observe_page(&page(&["a", "b"], Some("b"), "0", None, true), Instant::from_unix_secs(3))
            .unwrap();
        scan.authorize_reconciliation(None);
        assert!(scan.can_reconcile());
        assert_eq!(scan.pages, 3);
    }

    #[test]
    fn skipped_replayed_or_falsely_completed_pages_do_not_mutate_checkpoint() {
        let mut scan = scan(&["a", "b"]);
        let before = to_value(&scan).unwrap();
        assert!(
            scan.observe_page(&page(&["a", "b"], Some("b"), "0", None, true), Instant::from_unix_secs(1))
                .is_err()
        );
        assert!(
            scan.observe_page(&page(&["a", "b"], Some("a"), "0", None, true), Instant::from_unix_secs(1))
                .is_err()
        );
        assert_eq!(to_value(&scan).unwrap(), before);
        scan.observe_page(&page(&["a", "b"], Some("a"), "0", Some("100"), false), Instant::from_unix_secs(1))
            .unwrap();
        let before = to_value(&scan).unwrap();
        assert!(
            scan.observe_page(
                &page(&["a", "b"], Some("a"), "0", Some("100"), false),
                Instant::from_unix_secs(1)
            )
            .is_err()
        );
        assert_eq!(to_value(&scan).unwrap(), before);
    }

    #[test]
    fn missing_or_changed_final_scope_releases_fence_without_negative_permission() {
        for confirmed in [None, Some(scope(&["b"]))] {
            let mut scan = scan(&["a"]);
            let mut last = page(&["a"], Some("a"), "0", None, true);
            last.confirmed_scope = confirmed;
            scan.observe_page(&last, Instant::from_unix_secs(1)).unwrap();
            scan.authorize_reconciliation(None);
            assert_eq!(scan.state, CatalogMembershipState::Uncertain);
            assert!(!scan.active);
            assert!(!scan.can_reconcile());
            assert!(scan.finish_reconciliation().is_err());
        }
    }

    #[test]
    fn changed_authorization_from_previous_baseline_is_observation_only() {
        let mut scan = complete_scan(&["a"]);
        scan.authorize_reconciliation(Some(&scope(&["a", "b"])));
        assert_eq!(scan.state, CatalogMembershipState::Uncertain);
        assert!(!scan.can_reconcile());
        assert!(!scan.active);
    }

    #[test]
    fn one_removed_pool_membership_cannot_withdraw_sku_still_seen_in_another_pool() {
        let scan = complete_scan(&["a", "b"]);
        let mut old =
            CatalogPoolMember::observed(&scan, "a", sku("sku-1"), Instant::from_unix_secs(1)).unwrap();
        old.last_seen_run = "old-run".into();
        let current =
            CatalogPoolMember::observed(&scan, "b", sku("sku-1"), Instant::from_unix_secs(2)).unwrap();
        assert!(old.retirement_candidate(&scan));
        assert!(!current.retirement_candidate(&scan));
        assert!(!old.observed_in(&scan, "sku-1"));
        assert!(current.observed_in(&scan, "sku-1"));
        assert_ne!(old.base.id, current.base.id);
    }

    #[test]
    fn failed_scan_and_unknown_legacy_fence_never_authorize_negative_results() {
        let mut scan = complete_scan(&["a"]);
        let mut old =
            CatalogPoolMember::observed(&scan, "a", sku("sku-1"), Instant::from_unix_secs(1)).unwrap();
        old.last_seen_run = "old-run".into();
        scan.abort();
        assert!(!old.retirement_candidate(&scan));
        assert!(!scan.active);
        let mut legacy = to_value(complete_scan(&["a"])).unwrap();
        legacy.as_object_mut().unwrap().remove("active");
        legacy.as_object_mut().unwrap().remove("negative_allowed");
        legacy.as_object_mut().unwrap().remove("completed_at");
        let legacy: CatalogMembershipScan = from_value(legacy).unwrap();
        assert!(!legacy.can_reconcile());
    }

    #[test]
    fn empty_scope_is_complete_only_after_explicit_recheck_and_preserves_old_authorization_boundary() {
        let mut scan = complete_scan(&[]);
        assert!(scan.can_reconcile());
        scan.authorize_reconciliation(Some(&scope(&["old-pool"])));
        assert!(!scan.can_reconcile());
        assert_eq!(scan.state, CatalogMembershipState::Uncertain);
    }

    #[test]
    fn member_identity_separates_accounts_connections_and_unknown_product_groups() {
        let scan = complete_scan(&["a"]);
        let member =
            CatalogPoolMember::observed(&scan, "a", sku("000123"), Instant::from_unix_secs(1)).unwrap();
        assert_eq!(member.sku.product_id, None);
        assert_eq!(member.sku.spec_id, "000123");
        assert_ne!(member.base.id, CatalogPoolMember::identity("connection", "another", "a", "000123"));
        assert_ne!(member.base.id, CatalogPoolMember::identity("another", "account-1", "a", "000123"));
        assert!(!member.observed_in(&scan, "123"));
    }

    #[test]
    fn finalization_is_monotonic_and_failed_progress_does_not_overflow() {
        let mut complete = complete_scan(&["a"]);
        complete.finish_reconciliation().unwrap();
        complete.abort();
        assert_eq!(complete.state, CatalogMembershipState::Reconciled);
        assert!(!complete.active);
        let mut scan = scan(&["a"]);
        scan.pages = u64::MAX;
        let before = to_value(&scan).unwrap();
        assert!(
            scan.observe_page(&page(&["a"], Some("a"), "0", None, true), Instant::from_unix_secs(1)).is_err()
        );
        assert_eq!(to_value(scan).unwrap(), before);
    }
}
