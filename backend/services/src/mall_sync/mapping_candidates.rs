//! 映射候选目标批量装载（INT-R19/INT-R20/INT-R21/INT-R22）。
//!
//! 候选装载按“先收集去重身份、一次批量读取、内存按序组装”进行，数据库访问
//! 次数不随候选项数量增长。排序、去重上限、缺失跳过与 data scope 语义与逐行
//! 旧路径一致；RBAC 归属判定与候选排序仍由 Service 解释。

use std::collections::{HashMap, HashSet};

use database::{CatalogExt, ContractExt, CustomerExt, NoTransaction, PartyExt};
use entities::catalog::{EnableStatus, Sku, SkuRevision};
use entities::common::time::BusinessDate;
use entities::contract::ContractStatus;
use entities::customer::{CustomerAccount, CustomerAssignment};
use entities::ids::{CustomerAccountId, PartyId, SkuId, SkuRevisionId};

use super::dto::MappingCandidateTargetView;
use super::{ContractFilter, MallSyncService, VoucherCategoryProfileRevisionFilter};
use crate::errors::Result;

/// 客户候选上限（与旧逐行路径一致）。
const CUSTOMER_CANDIDATE_LIMIT: usize = 50;
/// 卡券候选上限（与旧逐行路径一致）。
const VOUCHER_CANDIDATE_LIMIT: usize = 50;

/// 按首次出现顺序去重（保留归属/画像顺序，内存分页与旧路径一致）。
///
/// # 参数
/// * `ids` - 原始 ID 迭代器（可含重复、无序）
///
/// # 返回
/// 返回按首次出现排序的去重 ID。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存函数，不访问数据库；调用方保证输入已按业务顺序排列。
fn ordered_unique(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique
}

/// 组装客户候选视图（INT-R19 纯组装）。
///
/// 按归属顺序遍历，跳过缺失/停用客户、缺失主体与缺失修订，达到上限停止；
/// 与旧逐行路径的跳过与截断语义一致。
///
/// # 参数
/// * `ordered_customer_ids` - 按归属顺序去重的客户 ID
/// * `accounts` - 按客户 ID 索引的客户账号
/// * `parties` - 按主体 ID 索引的共用主体
/// * `revisions` - 按修订 ID 索引的主体修订
///
/// # 返回
/// 返回按归属顺序的候选视图（至多 `CUSTOMER_CANDIDATE_LIMIT` 条）。
///
/// # 错误
/// 无错误返回：缺失与停用只跳过，由批量装载的调用方保证事实完整。
///
/// # 约束
/// 纯内存组装，不访问数据库；上限与跳过语义与旧逐行路径一致。
fn assemble_customer_candidates(
    ordered_customer_ids: &[String],
    accounts: &HashMap<String, CustomerAccount>,
    parties: &HashMap<String, entities::party::Party>,
    revisions: &HashMap<String, entities::party::PartyRevision>,
) -> Vec<MappingCandidateTargetView> {
    let mut candidates = Vec::new();
    for customer_id in ordered_customer_ids {
        if candidates.len() >= CUSTOMER_CANDIDATE_LIMIT {
            break;
        }
        let Some(customer) = accounts.get(customer_id) else {
            continue;
        };
        if !customer.stable.status.is_active() {
            continue;
        }
        let party_key = customer.party_id.to_string();
        let Some(party) = parties.get(&party_key) else {
            continue;
        };
        let Some(revision_id) = party.stable.current_revision_id.clone() else {
            continue;
        };
        let Some(revision) = revisions.get(revision_id.as_str()) else {
            continue;
        };
        candidates.push(MappingCandidateTargetView {
            object_type: "CUSTOMER".to_string(),
            object_id: customer.base.id.clone(),
            stable_no: customer.customer_no.clone(),
            label: revision.legal_name.clone(),
            current_revision_id: revision_id.to_string(),
            eligibility: "ELIGIBLE".to_string(),
            reason: "当前账号对该客户具有有效销售归属".to_string(),
        });
    }
    candidates
}

/// 组装结算主体候选视图（INT-R20 纯组装）。
///
/// 按合同顺序对结算主体首次出现去重，跳过缺失/停用主体与缺失修订；与旧逐行
/// 路径的去重与跳过语义一致。
///
/// # 参数
/// * `settlement_party_ids` - 按合同顺序的结算主体 ID（含重复）
/// * `parties` - 按主体 ID 索引的共用主体
/// * `revisions` - 按修订 ID 索引的主体修订
///
/// # 返回
/// 返回按合同首次出现顺序的候选视图。
///
/// # 错误
/// 无错误返回：缺失与停用只跳过，由批量装载的调用方保证事实完整。
///
/// # 约束
/// 纯内存组装，不访问数据库；去重与跳过语义与旧逐行路径一致。
fn assemble_settlement_candidates(
    settlement_party_ids: &[String],
    parties: &HashMap<String, entities::party::Party>,
    revisions: &HashMap<String, entities::party::PartyRevision>,
) -> Vec<MappingCandidateTargetView> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for party_id in settlement_party_ids {
        if !seen.insert(party_id.clone()) {
            continue;
        }
        let Some(party) = parties.get(party_id) else {
            continue;
        };
        if !party.is_active() {
            continue;
        }
        let Some(revision_id) = party.stable.current_revision_id.clone() else {
            continue;
        };
        let Some(revision) = revisions.get(revision_id.as_str()) else {
            continue;
        };
        candidates.push(MappingCandidateTargetView {
            object_type: "SETTLEMENT_PARTY".to_string(),
            object_id: party.base.id.clone(),
            stable_no: party.party_no.clone(),
            label: revision.legal_name.clone(),
            current_revision_id: revision_id.to_string(),
            eligibility: "ELIGIBLE".to_string(),
            reason: "结算主体来自当前账号有效客户合同".to_string(),
        });
    }
    candidates
}

/// 组装卡券类目候选视图（INT-R21 纯组装）。
///
/// 按画像顺序对 SKU 首次出现去重，跳过缺失/停用未上架 SKU 与缺失修订，达到
/// 上限停止；与旧逐行路径的去重、过滤与截断语义一致。
///
/// # 参数
/// * `ordered_sku_ids` - 按画像顺序的 SKU ID（含重复）
/// * `skus` - 按 SKU ID 索引的 SKU
/// * `revisions` - 按修订 ID 索引的 SKU 修订
///
/// # 返回
/// 返回按画像首次出现顺序的候选视图（至多 `VOUCHER_CANDIDATE_LIMIT` 条）。
///
/// # 错误
/// 无错误返回：缺失、停用、未上架与缺失修订只跳过。
///
/// # 约束
/// 纯内存组装，不访问数据库；去重、过滤与截断语义与旧逐行路径一致。
fn assemble_voucher_candidates(
    ordered_sku_ids: &[String],
    skus: &HashMap<String, Sku>,
    revisions: &HashMap<String, SkuRevision>,
) -> Vec<MappingCandidateTargetView> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for sku_id in ordered_sku_ids {
        if !seen.insert(sku_id.clone()) || candidates.len() >= VOUCHER_CANDIDATE_LIMIT {
            continue;
        }
        let Some(sku) = skus.get(sku_id) else {
            continue;
        };
        if !sku.is_active() || !sku.listing_status.is_listed() {
            continue;
        }
        let Some(revision_id) = sku.stable.current_revision_id.clone() else {
            continue;
        };
        let Some(revision) = revisions.get(&revision_id) else {
            continue;
        };
        candidates.push(MappingCandidateTargetView {
            object_type: "VOUCHER_CATEGORY".to_string(),
            object_id: sku.base.id.clone(),
            stable_no: sku.sku_no.clone(),
            label: revision.name.clone(),
            current_revision_id: revision_id,
            eligibility: "ELIGIBLE".to_string(),
            reason: "卡券类目扩展启用且 SKU 当前已启用上架".to_string(),
        });
    }
    candidates
}

impl MallSyncService {
    /// 批量装载客户映射候选（INT-R19）。
    ///
    /// 先取调用方当天的生效归属，一次批量读取客户账号，再一次批量读取主体、
    /// 一次批量读取修订；内存按归属顺序组装。缺失与停用跳过、去重与上限语义
    /// 与旧逐行路径一致。
    ///
    /// # 参数
    /// * `actor_id` - 当前操作人
    ///
    /// # 返回
    /// 返回按归属顺序的候选视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误；缺失实体只跳过不报错。
    ///
    /// # 约束
    /// data scope 与候选排序仍由 Service 解释；不改变软删除与业务日期边界。
    pub(super) async fn customer_mapping_candidates(
        &self,
        actor_id: &str,
    ) -> Result<Vec<MappingCandidateTargetView>> {
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        let ordered_ids = ordered_unique(
            assignments
                .iter()
                .map(|assignment: &CustomerAssignment| assignment.customer_id.to_string()),
        );
        if ordered_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids = ordered_ids
            .iter()
            .map(CustomerAccountId::new)
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .customer_accounts()
            .find_accounts_by_ids(&account_ids, &mut NoTransaction)
            .await?;
        let account_map = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        let party_ids = ordered_unique(account_map.values().map(|account| account.party_id.to_string()))
            .into_iter()
            .map(PartyId::new)
            .collect::<Vec<_>>();
        let parties = self
            .db
            .parties()
            .find_parties_by_ids(&party_ids, &mut NoTransaction)
            .await?;
        let party_map = parties
            .into_iter()
            .map(|party| (party.base.id.clone(), party))
            .collect::<HashMap<_, _>>();
        let revision_ids = ordered_unique(
            party_map
                .values()
                .filter_map(|party| party.stable.current_revision_id.clone().map(|id| id.to_string())),
        );
        let revisions = self
            .db
            .party_revisions()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let revision_map = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(assemble_customer_candidates(
            &ordered_ids,
            &account_map,
            &party_map,
            &revision_map,
        ))
    }

    /// 批量装载结算主体映射候选（INT-R20）。
    ///
    /// 合同页一次查询后，一次批量读取结算主体、一次批量读取修订；内存按合同
    /// 顺序对结算主体去重组装。缺失与停用跳过语义与旧逐行路径一致。
    ///
    /// # 参数
    /// * `actor_id` - 当前操作人
    ///
    /// # 返回
    /// 返回按合同首次出现顺序的候选视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误；缺失实体只跳过不报错。
    ///
    /// # 约束
    /// 合同范围仍由调用方归属决定；不改变生效状态与软删除语义。
    pub(super) async fn settlement_mapping_candidates(
        &self,
        actor_id: &str,
    ) -> Result<Vec<MappingCandidateTargetView>> {
        let customer_ids = self.actor_customer_ids(actor_id).await?;
        let contracts = self
            .db
            .contracts()
            .search_contracts(
                &ContractFilter {
                    contract_no: None,
                    customer_id: None,
                    customer_ids: Some(customer_ids),
                    status: Some(ContractStatus::Effective),
                    page: 1,
                    page_size: 50,
                    sort_by: Some("created_at".to_string()),
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?;
        let settlement_ids = contracts
            .items
            .iter()
            .map(|contract| contract.settlement_party_id.clone())
            .collect::<Vec<_>>();
        if settlement_ids.is_empty() {
            return Ok(Vec::new());
        }
        let party_ids = ordered_unique(settlement_ids.iter().cloned())
            .into_iter()
            .map(PartyId::new)
            .collect::<Vec<_>>();
        let parties = self
            .db
            .parties()
            .find_parties_by_ids(&party_ids, &mut NoTransaction)
            .await?;
        let party_map = parties
            .into_iter()
            .map(|party| (party.base.id.clone(), party))
            .collect::<HashMap<_, _>>();
        let revision_ids = ordered_unique(
            party_map
                .values()
                .filter_map(|party| party.stable.current_revision_id.clone().map(|id| id.to_string())),
        );
        let revisions = self
            .db
            .party_revisions()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let revision_map = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(assemble_settlement_candidates(
            &settlement_ids,
            &party_map,
            &revision_map,
        ))
    }

    /// 批量装载卡券类目映射候选（INT-R21）。
    ///
    /// 画像页一次查询后，一次批量读取 SKU、一次批量读取 SKU 修订；内存按画像
    /// 顺序对 SKU 去重组装。缺失、停用、未上架与缺失修订跳过，上限语义与旧逐
    /// 行路径一致。
    ///
    /// # 返回
    /// 返回按画像首次出现顺序的候选视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误；缺失实体只跳过不报错。
    ///
    /// # 约束
    /// 不改变启用、上架与软删除语义。
    pub(super) async fn voucher_mapping_candidates(&self) -> Result<Vec<MappingCandidateTargetView>> {
        let profiles = self
            .db
            .voucher_category_profile_revisions()
            .search_voucher_category_profile_revisions(
                &VoucherCategoryProfileRevisionFilter {
                    sku_id: None,
                    status: Some(EnableStatus::Active),
                    page: 1,
                    page_size: 100,
                    sort_by: Some("revision_no".to_string()),
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?;
        let ordered_sku_ids = profiles
            .items
            .iter()
            .map(|profile| profile.sku_id.clone())
            .collect::<Vec<_>>();
        if ordered_sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let sku_ids = ordered_unique(ordered_sku_ids.iter().cloned())
            .into_iter()
            .map(SkuId::new)
            .collect::<Vec<_>>();
        let skus = self.db.skus().find_by_ids(&sku_ids, &mut NoTransaction).await?;
        let sku_map = skus
            .into_iter()
            .map(|sku| (sku.base.id.clone(), sku))
            .collect::<HashMap<_, _>>();
        let revision_ids = ordered_unique(
            sku_map
                .values()
                .filter_map(|sku| sku.stable.current_revision_id.clone()),
        )
        .into_iter()
        .map(SkuRevisionId::new)
        .collect::<Vec<_>>();
        let revisions = self
            .db
            .sku_revisions()
            .find_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let revision_map = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(assemble_voucher_candidates(
            &ordered_sku_ids,
            &sku_map,
            &revision_map,
        ))
    }

    /// 去重后的生效客户 ID 投影（INT-R22）。
    ///
    /// 直接复用仓储的 `distinct_active_customer_ids_for_user` 投影；排序去重与
    /// 业务日期边界归仓储所有，Service 只解释数据范围。
    ///
    /// # 参数
    /// * `actor_id` - 当前操作人
    ///
    /// # 返回
    /// 返回排序去重后的客户 ID；无生效归属时为空集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    pub(super) async fn actor_customer_ids(&self, actor_id: &str) -> Result<Vec<String>> {
        Ok(self
            .db
            .customer_assignments()
            .distinct_active_customer_ids_for_user(actor_id, BusinessDate::today(), &mut NoTransaction)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::{assemble_settlement_candidates, assemble_voucher_candidates, ordered_unique};

    #[test]
    fn ordered_unique_keeps_first_order_and_handles_empty() {
        assert!(ordered_unique(Vec::<String>::new()).is_empty());
        assert_eq!(
            ordered_unique(vec!["b".to_string(), "a".to_string(), "b".to_string(),]),
            vec!["b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn assemble_settlement_candidates_dedups_and_skips_missing() {
        let candidates = assemble_settlement_candidates(
            &["p-missing".to_string(), "p-1".to_string(), "p-1".to_string()],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn assemble_voucher_candidates_limits_and_skips_missing() {
        let ids = vec!["sku-missing".to_string(), "sku-missing".to_string()];
        let candidates = assemble_voucher_candidates(
            &ids,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(candidates.is_empty());
    }
}
