use std::collections::{HashMap, HashSet};

use entities::ids::{PartyId, SupplierAccountId};
use entities::party::{Party, PartyAddress, PartyBankAccount, PartyContact, PartyRevision, PartyTaxProfile};
use entities::supplier::{
    CapabilityCode, QualificationType, SupplierAccount, SupplierAccountStatus, SupplierCapability,
    SupplierCommercialProfileRevision, SupplierQualification, SupplierQualificationCapability,
    SupplierRatingRevision,
};

use super::super::extensions::PartyExt;
use super::super::{PageResult, Repository};
use super::account::SupplierAccountFilter;
use super::{SupplierAccountRow, SupplierRepository, SUPPLIER_ACCOUNTS, SUPPLIER_QUALIFICATION_CAPABILITIES};
use crate::executor::Executor;
use crate::{Error, Result};

/// 供应商列表仓储侧资质健康状态筛选。
///
/// # 约束
/// 仓储自有类型，避免数据库层依赖 Service DTO；语义与 Service 侧
/// `SupplierQualificationHealth` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierQualificationHealthFilter {
    /// 当前有效。
    Valid,
    /// 当前有效且 30 天内到期。
    Expiring30,
    /// 已失效。
    Expired,
    /// 尚未登记对应资质。
    NotRegistered,
}

/// 资质筛选约束的纯分支种类（`PROC-R03`）。
///
/// # 约束
/// 纯内存判定，不触及 I/O；`list_filter_qualification_constraints` 按此 branching
/// 选择命中集合或排除集合查询，分支语义与本枚举一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualificationConstraintKind {
    /// 未筛选资质，不产生候选约束。
    Unconstrained,
    /// 按类型或健康状态产生命中集合。
    Included,
    /// `NotRegistered` 产生排除集合，命中集合为空。
    Excluded,
}

/// 判定资质筛选约束的纯分支种类。
///
/// 未传健康状态且资质类型为空时无约束；`NotRegistered` 无论类型是否为空均走
/// 排除分支；其余组合均走命中分支。
///
/// # 参数
/// * `qualification_types` - 资质类型；空集合表示不限制类型
/// * `health` - 资质健康状态；`None` 表示仅按类型命中
///
/// # 返回
/// 返回命中、排除或无约束的分支种类。
///
/// # 错误
/// 无；仅做内存分支判定。
///
/// # 约束
/// 纯内存判定，不触及 I/O；分支结果必须与
/// `list_filter_qualification_constraints` 的查询选择保持一致。
fn qualification_constraint_kind(
    qualification_types: &[QualificationType],
    health: Option<SupplierQualificationHealthFilter>,
) -> QualificationConstraintKind {
    match health {
        None if qualification_types.is_empty() => QualificationConstraintKind::Unconstrained,
        None => QualificationConstraintKind::Included,
        Some(SupplierQualificationHealthFilter::NotRegistered) => QualificationConstraintKind::Excluded,
        Some(_) => QualificationConstraintKind::Included,
    }
}

/// 供应商列表仓储搜索输入。
///
/// # 约束
/// 全部字段均为 Service 已规范化的业务值；分页排序在仓储白名单内校验。
#[derive(Debug, Clone)]
pub struct SupplierListSearchInput {
    /// 供应商编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体精确匹配。
    pub party_id: Option<PartyId>,
    /// 启停状态。
    pub status: Option<SupplierAccountStatus>,
    /// 能力代码。
    pub capability_codes: Vec<CapabilityCode>,
    /// 资质类型。
    pub qualification_types: Vec<QualificationType>,
    /// 资质健康状态。
    pub qualification_health: Option<SupplierQualificationHealthFilter>,
    /// 当前业务日字符串。
    pub as_of: String,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段。
    pub sort_by: Option<String>,
    /// 是否升序。
    pub sort_ascending: bool,
}

/// 供应商列表事实束。
///
/// # 约束
/// 仅承载持久化事实与投影行，不含 View 映射与授权结论。
#[derive(Debug)]
pub struct SupplierListBundle {
    /// 当前页投影行与总数。
    pub page: PageResult<SupplierAccountRow>,
    /// 命中的主体集合。
    pub parties: Vec<Party>,
    /// 命中的主体当前修订集合。
    pub revisions: Vec<PartyRevision>,
    /// 命中的商务资料版本集合。
    pub profiles: Vec<SupplierCommercialProfileRevision>,
}

/// 供应商详情事实束。
///
/// # 约束
/// 仅承载持久化事实；敏感令牌签发与 View 映射保留在 Service。
#[derive(Debug)]
pub struct SupplierDetailBundle {
    /// 供应商角色。
    pub supplier: SupplierAccount,
    /// 关联主体；缺失时为 `None`。
    pub party: Option<Party>,
    /// 主体当前修订；指针缺失或目标缺失时为 `None`。
    pub party_revision: Option<PartyRevision>,
    /// 联系人历史集合。
    pub contacts: Vec<PartyContact>,
    /// 地址历史集合。
    pub addresses: Vec<PartyAddress>,
    /// 税务资料历史集合。
    pub tax_profiles: Vec<PartyTaxProfile>,
    /// 银行账户历史集合。
    pub bank_accounts: Vec<PartyBankAccount>,
    /// 能力集合。
    pub capabilities: Vec<SupplierCapability>,
    /// 资质集合。
    pub qualifications: Vec<SupplierQualification>,
    /// 资质适用能力关联集合。
    pub qualification_links: Vec<SupplierQualificationCapability>,
    /// 评级历史（最新优先）。
    pub ratings: Vec<SupplierRatingRevision>,
    /// 商务资料历史（最新优先）。
    pub commercial_profiles: Vec<SupplierCommercialProfileRevision>,
    /// 商务版本引用的签约/付款主体当前法定名称。
    pub commercial_party_names: HashMap<String, String>,
}

/// 合并两个供应商角色候选集合；两个条件同时存在时取交集。
///
/// # 参数
/// * `current` - 已有筛选条件命中的候选集合
/// * `matched` - 新筛选条件命中的候选集合
///
/// # 返回
/// 两者均存在时返回交集，仅一者存在时原样返回，均不存在时返回 `None`。
///
/// # 错误
/// 无。
///
/// # 约束
/// 纯内存集合运算，不触及 I/O；输入顺序按 `current` 保留，交集判定经哈希集合完成。
fn intersect_supplier_ids(
    current: Option<Vec<SupplierAccountId>>,
    matched: Option<Vec<SupplierAccountId>>,
) -> Option<Vec<SupplierAccountId>> {
    let (current, matched) = match (current, matched) {
        (Some(current), Some(matched)) => (current, matched),
        (Some(current), None) => return Some(current),
        (None, Some(matched)) => return Some(matched),
        (None, None) => return None,
    };
    let matched: HashSet<String> = matched.into_iter().map(|id| id.to_string()).collect();
    Some(
        current
            .into_iter()
            .filter(|id| matched.contains(&id.to_string()))
            .collect(),
    )
}

/// 计算“30 天内到期”筛选窗口的结束业务日。
///
/// # 参数
/// * `as_of` - 窗口起始业务日字符串
///
/// # 返回
/// 返回起始日后第三十个自然日的稳定日期字符串。
///
/// # 错误
/// 日期格式非法或计算溢出时返回错误。
///
/// # 约束
/// 纯日期计算，不触及 I/O；窗口长度固定为 30 天。
fn qualification_expiry_cutoff(as_of: &str) -> Result<String> {
    let as_of = as_of
        .parse::<entities::common::time::BusinessDate>()
        .map_err(|_| Error::EntityMetadataOutOfRange("supplier business date"))?;
    Ok(SupplierQualification::expiry_cutoff(as_of, 30)
        .map_err(|_| Error::EntityMetadataOutOfRange("supplier expiry cutoff"))?
        .to_string())
}

impl<'a> SupplierRepository<'a> {
    /// 分页前生效的供应商列表事实束查询（`PROC-R03`）。
    ///
    /// 将关键词主体命中、能力与资质候选约束、分页投影查询与列表水合所需的
    /// 主体/修订/商务资料批量读取收敛为固定上界的仓储调用；关键词、能力与
    /// 资质健康状态均在分页计数前生效，保证总数与分页内容一致。
    ///
    /// # 参数
    /// * `input` - 已规范化的列表业务筛选、分页排序与业务日
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行、总数及水合所需的批量事实。
    ///
    /// # 错误
    /// 业务日期非法或任一批量查询失败时返回错误。
    ///
    /// # 约束
    /// 不开启或提交事务；查询次数与页大小无关且有固定上界；软删除由基查询
    /// 过滤，排序字段经仓储白名单校验。
    pub async fn load_supplier_list_bundle(
        &self,
        input: &SupplierListSearchInput,
        executor: &mut dyn Executor,
    ) -> Result<SupplierListBundle> {
        let party_ids = match input.keyword.as_deref() {
            Some(keyword) => Some(
                self.db
                    .party()
                    .matching_current_party_ids_by_name(keyword, executor)
                    .await?,
            ),
            None => None,
        };
        let (supplier_ids, excluded_supplier_ids) = self
            .list_filter_id_constraints(
                &input.capability_codes,
                &input.qualification_types,
                input.qualification_health,
                &input.as_of,
                executor,
            )
            .await?;
        let filter = SupplierAccountFilter {
            keyword: input.keyword.clone(),
            party_id: input.party_id.clone(),
            party_ids,
            status: input.status,
            supplier_ids,
            excluded_supplier_ids,
            page: input.page,
            page_size: input.page_size,
            sort_by: input.sort_by.clone(),
            sort_ascending: input.sort_ascending,
        };
        let page = Repository::new(self.db, SUPPLIER_ACCOUNTS)
            .search_supplier_accounts(&filter, executor)
            .await?;
        let party_ids: Vec<PartyId> = page.items.iter().map(|row| PartyId::new(&row.party_id)).collect();
        let (parties, revisions) = self
            .db
            .party()
            .list_with_current_revisions(&party_ids, executor)
            .await?;
        let profile_ids: Vec<String> = page
            .items
            .iter()
            .filter_map(|row| row.current_commercial_profile_revision_id.clone())
            .collect();
        let profiles = self
            .list_commercial_profiles_by_ids(&profile_ids, executor)
            .await?;
        Ok(SupplierListBundle {
            page,
            parties,
            revisions,
            profiles,
        })
    }

    /// 组装能力与资质条件对应的角色 ID 约束。
    ///
    /// # 参数
    /// * `capability_codes` - 命中的能力代码；空集合表示不限制
    /// * `qualification_types` - 命中的资质类型；空集合表示不限制类型
    /// * `health` - 资质健康状态；`None` 表示不按健康状态过滤
    /// * `as_of` - 当前业务日字符串
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回必须命中的候选 ID 与必须排除的 ID 集合。
    ///
    /// # 错误
    /// 业务日期非法或任一仓储查询失败时返回错误。
    ///
    /// # 约束
    /// 纯仓储侧候选计算，不返回 Service DTO；交集语义与旧 Service 一致。
    async fn list_filter_id_constraints(
        &self,
        capability_codes: &[CapabilityCode],
        qualification_types: &[QualificationType],
        health: Option<SupplierQualificationHealthFilter>,
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        let capability_ids = if capability_codes.is_empty() {
            None
        } else {
            Some(
                self.list_supplier_ids_by_active_capability_codes(capability_codes, as_of, executor)
                    .await?,
            )
        };
        let (qualification_ids, excluded_qualification_ids) = self
            .list_filter_qualification_constraints(qualification_types, health, as_of, executor)
            .await?;
        Ok((
            intersect_supplier_ids(capability_ids, qualification_ids),
            excluded_qualification_ids,
        ))
    }

    /// 查询资质类型和健康状态对应的供应商角色 ID 约束。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `health` - 资质健康状态；`None` 表示仅按类型命中
    /// * `as_of` - 当前业务日字符串
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回应命中与应排除的供应商 ID 集合；未筛选资质时均为 `None`。
    ///
    /// # 错误
    /// 到期窗口计算或任一仓储查询失败时返回错误。
    ///
    /// # 约束
    /// `Expiring30` 窗口为起始日后第三十个自然日；`NotRegistered` 仅返回排除集合。
    async fn list_filter_qualification_constraints(
        &self,
        qualification_types: &[QualificationType],
        health: Option<SupplierQualificationHealthFilter>,
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        match qualification_constraint_kind(qualification_types, health) {
            QualificationConstraintKind::Unconstrained => Ok((None, None)),
            QualificationConstraintKind::Excluded => Ok((
                None,
                Some(
                    self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                        .await?,
                ),
            )),
            QualificationConstraintKind::Included => match health {
                None => Ok((
                    Some(
                        self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                            .await?,
                    ),
                    None,
                )),
                Some(SupplierQualificationHealthFilter::Valid) => Ok((
                    Some(
                        self.list_supplier_ids_by_valid_qualifications(qualification_types, as_of, executor)
                            .await?,
                    ),
                    None,
                )),
                Some(SupplierQualificationHealthFilter::Expiring30) => {
                    let expires_by = qualification_expiry_cutoff(as_of)?;
                    Ok((
                        Some(
                            self.list_supplier_ids_by_expiring_qualifications(
                                qualification_types,
                                as_of,
                                &expires_by,
                                executor,
                            )
                            .await?,
                        ),
                        None,
                    ))
                }
                Some(SupplierQualificationHealthFilter::Expired) => Ok((
                    Some(
                        self.list_supplier_ids_by_expired_qualifications(
                            qualification_types,
                            as_of,
                            executor,
                        )
                        .await?,
                    ),
                    None,
                )),
                Some(SupplierQualificationHealthFilter::NotRegistered) => Ok((
                    None,
                    Some(
                        self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                            .await?,
                    ),
                )),
            },
        }
    }

    /// 批量加载供应商详情所需的全部事实（`PROC-R04`）。
    ///
    /// 一次调用返回供应商、主体当前指针及联系人、地址、税务、银行、能力、
    /// 资质关联、评级与商务版本历史；资质与能力关联一次批量读取，查询次数
    /// 有固定上界，不随资质或能力数量增长。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 供应商不存在时返回 `None`；存在时返回全部事实束，主体缺失、当前指针
    /// 缺失或历史集合为空时以 `None`/空集合表达，由 Service 映射为明确语义。
    ///
    /// # 错误
    /// 任一批量仓储查询失败时返回错误。
    ///
    /// # 约束
    /// 不开启或提交事务；不记录敏感明文日志；不返回 Service DTO 或 View。
    pub async fn load_supplier_detail_bundle(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDetailBundle>> {
        let Some(supplier) = self.account(supplier_id, executor).await? else {
            return Ok(None);
        };
        let party_bundle = self
            .db
            .party()
            .find_with_current_revision(&supplier.party_id, executor)
            .await?;
        let (party, party_revision) = match party_bundle {
            Some((party, revision)) => (Some(party), revision),
            None => (None, None),
        };
        let contacts = self
            .db
            .party_contacts()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let addresses = self
            .db
            .party_addresses()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let tax_profiles = self
            .db
            .party_tax_profiles()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let bank_accounts = self
            .db
            .party_bank_accounts()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let capabilities = self.list_capabilities(supplier_id, executor).await?;
        let qualifications = self.list_qualifications(supplier_id, executor).await?;
        let qualification_ids: Vec<entities::ids::SupplierQualificationId> = qualifications
            .iter()
            .map(|item| entities::ids::SupplierQualificationId::new(&item.base.id))
            .collect();
        let qualification_links =
            Repository::<SupplierQualificationCapability>::new(self.db, SUPPLIER_QUALIFICATION_CAPABILITIES)
                .list_by_qualification_ids(&qualification_ids, executor)
                .await?;
        let ratings = self.list_ratings_latest_first(supplier_id, executor).await?;
        let commercial_profiles = self
            .list_commercial_profiles_latest_first(supplier_id, executor)
            .await?;
        let commercial_party_names = self
            .commercial_party_names(&commercial_profiles, executor)
            .await?;
        Ok(Some(SupplierDetailBundle {
            supplier,
            party,
            party_revision,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            capabilities,
            qualifications,
            qualification_links,
            ratings,
            commercial_profiles,
            commercial_party_names,
        }))
    }

    /// 批量读取商务版本引用的签约与付款主体当前法定名称。
    ///
    /// # 参数
    /// * `profiles` - 已加载的商务资料历史
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回主体 ID 字符串到当前法定名称的映射；缺少当前修订时无键。
    ///
    /// # 错误
    /// 主体或当前修订批量查询失败时返回错误。
    ///
    /// # 约束
    /// 只经主体域属主访问器组装，不在供应商仓储内直查外域集合。
    async fn commercial_party_names(
        &self,
        profiles: &[SupplierCommercialProfileRevision],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let party_ids: Vec<PartyId> = profiles
            .iter()
            .flat_map(|profile| {
                [
                    profile.signing_entity_party_id.to_string(),
                    profile.payment_entity_party_id.to_string(),
                ]
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .map(PartyId::new)
            .collect();
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        self.db
            .party()
            .current_legal_names_by_party_ids(&party_ids, executor)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{
        intersect_supplier_ids, qualification_constraint_kind, qualification_expiry_cutoff,
        QualificationConstraintKind, SupplierQualificationHealthFilter,
    };

    #[test]
    fn supplier_list_candidate_intersection_preserves_order_and_empty() {
        use entities::ids::SupplierAccountId;
        // 能力与资质双维度同时命中时取交集，且保留能力侧顺序。
        let capability_ids = Some(vec![
            SupplierAccountId::new("s-1"),
            SupplierAccountId::new("s-2"),
            SupplierAccountId::new("s-3"),
        ]);
        let qualification_ids = Some(vec![
            SupplierAccountId::new("s-2"),
            SupplierAccountId::new("s-3"),
            SupplierAccountId::new("s-4"),
        ]);
        assert_eq!(
            intersect_supplier_ids(capability_ids, qualification_ids)
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["s-2".to_string(), "s-3".to_string()]
        );
        // 仅一侧约束时原样透传。
        assert_eq!(
            intersect_supplier_ids(Some(vec![SupplierAccountId::new("s-1")]), None)
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["s-1".to_string()]
        );
        assert_eq!(
            intersect_supplier_ids(None, Some(vec![SupplierAccountId::new("s-9")]))
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["s-9".to_string()]
        );
        assert!(intersect_supplier_ids(None, None).is_none());
        assert_eq!(
            intersect_supplier_ids(
                Some(vec![SupplierAccountId::new("s-1")]),
                Some(vec![SupplierAccountId::new("s-9")])
            )
            .unwrap()
            .len(),
            0
        );
    }

    /// 资质约束分支覆盖类型、健康状态与未登记排除路径。
    #[test]
    fn qualification_constraint_kind_covers_all_branches() {
        use entities::supplier::QualificationType;
        use QualificationConstraintKind::{Excluded, Included, Unconstrained};
        // 未筛选资质时无约束。
        assert_eq!(qualification_constraint_kind(&[], None), Unconstrained);
        // 仅按类型命中。
        assert_eq!(
            qualification_constraint_kind(&[QualificationType::FoodLicense], None),
            Included
        );
        // 各健康状态均走命中分支。
        for health in [
            SupplierQualificationHealthFilter::Valid,
            SupplierQualificationHealthFilter::Expiring30,
            SupplierQualificationHealthFilter::Expired,
        ] {
            assert_eq!(
                qualification_constraint_kind(&[QualificationType::FoodLicense], Some(health)),
                Included,
                "健康状态 {health:?} 应走命中分支"
            );
            assert_eq!(
                qualification_constraint_kind(&[], Some(health)),
                Included,
                "空类型下健康状态 {health:?} 仍应走命中分支"
            );
        }
        // 未登记资质走排除分支，命中集合为空。
        assert_eq!(
            qualification_constraint_kind(
                &[QualificationType::FoodLicense],
                Some(SupplierQualificationHealthFilter::NotRegistered)
            ),
            Excluded
        );
        assert_eq!(
            qualification_constraint_kind(&[], Some(SupplierQualificationHealthFilter::NotRegistered)),
            Excluded
        );
    }

    /// 到期窗口固定为起始日后第三十个自然日。
    #[test]
    fn supplier_list_expiry_cutoff_is_thirty_days_after_as_of() {
        assert_eq!(qualification_expiry_cutoff("2026-08-31").unwrap(), "2026-09-30");
        assert_eq!(qualification_expiry_cutoff("2026-01-31").unwrap(), "2026-03-02");
        assert_eq!(qualification_expiry_cutoff("2026-02-01").unwrap(), "2026-03-03");
        assert!(qualification_expiry_cutoff("not-a-date").is_err());
        assert!(qualification_expiry_cutoff("2026-13-01").is_err());
    }
}

/// `PROC-R03`/`PROC-R04` 列表与详情事实束的真实 MongoDB 验收（隔离库，Quality 单独执行）。
#[cfg(test)]
mod proc_supplier_io_mongo_tests {
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::ids::{
        PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId,
        SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
        SupplierRatingRevisionId,
    };
    use entities::money::Rate;
    use entities::party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
    use entities::supplier::{
        CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
        ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
        SupplierCapability, SupplierCapabilityData, SupplierCommercialProfileRevision,
        SupplierCommercialProfileRevisionData, SupplierQualification, SupplierQualificationCapability,
        SupplierQualificationCapabilityData, SupplierQualificationData, SupplierRating,
        SupplierRatingRevision, SupplierRatingRevisionData,
    };
    use mongodb::bson::doc;
    use test_support::{require_mongo, TestDb};

    use super::super::SUPPLIER_CAPABILITIES;
    use super::{SupplierListSearchInput, SupplierQualificationHealthFilter, SUPPLIER_ACCOUNTS};
    use crate::{ensure_indexes, NoTransaction, PartyExt, SupplierExt, Transactional};

    /// 列表与详情验收的业务日。
    const AS_OF: &str = "2026-08-31";

    /// 构造带当前修订的主体。
    ///
    /// # 参数
    /// * `id` - 主体稳定 ID
    /// * `revision_id` - 当前修订 ID
    /// * `legal_name` - 法定名称
    ///
    /// # 返回
    /// 返回主体与其当前修订。
    fn party_with_revision(id: &str, revision_id: &str, legal_name: &str) -> (Party, PartyRevision) {
        let mut party = Party::new(
            PartyId::new(id),
            PartyData {
                party_no: format!("P-{id}"),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "test",
        )
        .expect("主体构造失败");
        let revision = PartyRevision::new(
            PartyRevisionId::new(revision_id),
            PartyRevisionData {
                party_id: PartyId::new(id),
                revision_no: 1,
                legal_name: legal_name.to_string(),
                short_name: None,
                change_reason: "初始登记".to_string(),
            },
        )
        .expect("主体修订构造失败");
        party.stable.current_revision_id = Some(revision_id.to_string());
        (party, revision)
    }

    /// 构造供应商角色。
    ///
    /// # 参数
    /// * `id` - 供应商稳定 ID
    /// * `party_id` - 所属主体 ID
    /// * `supplier_no` - 供应商编号
    /// * `profile_id` - 当前商务资料 ID；`None` 表示无当前指针
    ///
    /// # 返回
    /// 返回未删除的启用供应商角色。
    fn supplier_account(
        id: &str,
        party_id: &str,
        supplier_no: &str,
        profile_id: Option<&str>,
    ) -> SupplierAccount {
        SupplierAccount::new(
            SupplierAccountId::new(id),
            SupplierAccountData {
                party_id: PartyId::new(party_id),
                supplier_no: supplier_no.to_string(),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: profile_id
                    .map(SupplierCommercialProfileRevisionId::new),
                status: SupplierAccountStatus::Active,
            },
            "test",
        )
        .expect("供应商角色构造失败")
    }

    /// 构造首版商务资料。
    ///
    /// # 参数
    /// * `id` - 修订 ID
    /// * `supplier_id` - 所属供应商 ID
    /// * `party_id` - 签约与付款主体 ID
    ///
    /// # 返回
    /// 返回修订号为 1 的商务资料。
    fn commercial_profile(id: &str, supplier_id: &str, party_id: &str) -> SupplierCommercialProfileRevision {
        SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new(id),
            SupplierCommercialProfileRevisionData {
                supplier_id: SupplierAccountId::new(supplier_id),
                revision_no: 1,
                settlement_mode: SettlementMode::PayAfterUse,
                reconciliation_cycle: ReconciliationCycle::Monthly,
                payment_term_snapshot: "NET-30".to_string(),
                business_category: Some("经营类目".to_string()),
                invoice_type: InvoiceType::VatSpecial,
                invoice_tax_rate: Rate::from_str("0.13").unwrap(),
                signing_entity_party_id: PartyId::new(party_id),
                payment_entity_party_id: PartyId::new(party_id),
                change_reason: "初始登记".to_string(),
            },
        )
        .expect("商务资料构造失败")
    }

    /// 构造长期有效的启用能力。
    ///
    /// # 参数
    /// * `id` - 能力稳定 ID
    /// * `supplier_id` - 所属供应商 ID
    /// * `code` - 能力代码
    ///
    /// # 返回
    /// 返回自 2026-01-01 生效的启用能力。
    fn capability(id: &str, supplier_id: &str, code: CapabilityCode) -> SupplierCapability {
        SupplierCapability::new(
            SupplierCapabilityId::new(id),
            SupplierCapabilityData {
                supplier_id: SupplierAccountId::new(supplier_id),
                capability_code: code,
                service_region: None,
                owner_user_id: "test".to_string(),
                fulfillment_note: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                status: CapabilityStatus::Active,
            },
            "test",
        )
        .expect("供应商能力构造失败")
    }

    /// 构造资质。
    ///
    /// # 参数
    /// * `id` - 资质稳定 ID
    /// * `supplier_id` - 所属供应商 ID
    /// * `qualification_type` - 资质类型
    /// * `certificate_no` - 证书编号
    /// * `valid_to` - 失效日；`None` 表示长期有效
    ///
    /// # 返回
    /// 返回自 2026-01-01 生效的启用资质。
    fn qualification(
        id: &str,
        supplier_id: &str,
        qualification_type: QualificationType,
        certificate_no: &str,
        valid_to: Option<BusinessDate>,
    ) -> SupplierQualification {
        SupplierQualification::new(
            SupplierQualificationId::new(id),
            SupplierQualificationData {
                supplier_id: SupplierAccountId::new(supplier_id),
                qualification_type,
                certificate_no: certificate_no.to_string(),
                issuer: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to,
                attachment_id: None,
                status: QualificationStatus::Active,
            },
            "test",
        )
        .expect("供应商资质构造失败")
    }

    /// 写入列表与详情验收夹具。
    ///
    /// sup-a（Physical 能力 + 10 天内到期的有效食品资质 + 第二份合同资质 +
    /// 评级 + 商务资料）；sup-b（Physical 能力，无资质）；sup-c（Api 能力 +
    /// 已过期食品资质）；sup-orphan（主体缺失）。
    ///
    /// # 参数
    /// * `db` - 隔离测试库
    ///
    /// # 错误
    /// 任一夹具写入失败时 panic。
    async fn seed_supplier_io_fixture(db: &mongodb::Database) {
        for (party_id, revision_id, legal_name) in [
            ("party-a", "partyrev-a", "供应商甲"),
            ("party-b", "partyrev-b", "供应商乙"),
            ("party-c", "partyrev-c", "供应商丙"),
        ] {
            let (party, revision) = party_with_revision(party_id, revision_id, legal_name);
            db.parties()
                .create(&party, &mut NoTransaction)
                .await
                .expect("主体写入失败");
            db.party_revisions()
                .create(&revision, &mut NoTransaction)
                .await
                .expect("主体修订写入失败");
        }
        for (id, party_id, supplier_no, profile_id) in [
            ("sup-a", "party-a", "SUP-A", Some("profile-a")),
            ("sup-b", "party-b", "SUP-B", None),
            ("sup-c", "party-c", "SUP-C", None),
            ("sup-orphan", "party-missing", "SUP-ORPHAN", None),
        ] {
            db.supplier_accounts()
                .create(
                    &supplier_account(id, party_id, supplier_no, profile_id),
                    &mut NoTransaction,
                )
                .await
                .expect("供应商角色写入失败");
        }
        db.supplier_commercial_profile_revisions()
            .create(
                &commercial_profile("profile-a", "sup-a", "party-a"),
                &mut NoTransaction,
            )
            .await
            .expect("商务资料写入失败");
        for (id, supplier_id, code) in [
            ("cap-a1", "sup-a", CapabilityCode::Physical),
            ("cap-b1", "sup-b", CapabilityCode::Physical),
            ("cap-c1", "sup-c", CapabilityCode::Api),
        ] {
            db.supplier_capabilities()
                .create(&capability(id, supplier_id, code), &mut NoTransaction)
                .await
                .expect("供应商能力写入失败");
        }
        let expiring = BusinessDate::from_ymd(2026, 9, 10).unwrap();
        let expired = BusinessDate::from_ymd(2026, 6, 1).unwrap();
        for (id, supplier_id, qualification_type, certificate_no, valid_to) in [
            (
                "qual-a",
                "sup-a",
                QualificationType::FoodLicense,
                "FOOD-A",
                Some(expiring),
            ),
            ("qual-a2", "sup-a", QualificationType::Contract, "HT-A", None),
            (
                "qual-c",
                "sup-c",
                QualificationType::FoodLicense,
                "FOOD-C",
                Some(expired),
            ),
        ] {
            db.supplier_qualifications()
                .create(
                    &qualification(id, supplier_id, qualification_type, certificate_no, valid_to),
                    &mut NoTransaction,
                )
                .await
                .expect("供应商资质写入失败");
        }
        for (id, qualification_id, capability_id) in
            [("link-a1", "qual-a", "cap-a1"), ("link-a2", "qual-a2", "cap-a1")]
        {
            db.supplier_qualification_capabilities()
                .create(
                    &SupplierQualificationCapability::new(
                        SupplierQualificationCapabilityId::new(id),
                        SupplierQualificationCapabilityData {
                            qualification_id: SupplierQualificationId::new(qualification_id),
                            capability_id: SupplierCapabilityId::new(capability_id),
                        },
                    )
                    .expect("资质关联构造失败"),
                    &mut NoTransaction,
                )
                .await
                .expect("资质关联写入失败");
        }
        db.supplier_rating_revisions()
            .create(
                &SupplierRatingRevision::new(
                    SupplierRatingRevisionId::new("rating-a"),
                    SupplierRatingRevisionData {
                        supplier_id: SupplierAccountId::new("sup-a"),
                        revision_no: 1,
                        initial_score: Some(80),
                        rating: SupplierRating::A,
                        current_score: 85,
                        valid_from: BusinessDate::from_ymd(2026, 8, 1).unwrap(),
                        valid_to: None,
                        change_reason: "初始评级".to_string(),
                    },
                )
                .expect("供应商评级构造失败"),
                &mut NoTransaction,
            )
            .await
            .expect("供应商评级写入失败");
    }

    /// 构造列表搜索输入。
    ///
    /// # 参数
    /// * `build` - 输入调整闭包，由调用方设置筛选维度
    ///
    /// # 返回
    /// 返回业务日固定为验收日的搜索输入。
    fn list_input(build: impl FnOnce(&mut SupplierListSearchInput)) -> SupplierListSearchInput {
        let mut input = SupplierListSearchInput {
            keyword: None,
            party_id: None,
            status: None,
            capability_codes: Vec::new(),
            qualification_types: Vec::new(),
            qualification_health: None,
            as_of: AS_OF.to_string(),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        build(&mut input);
        input
    }

    /// 返回事实束页中的供应商 ID 集合（字典序）。
    ///
    /// # 参数
    /// * `bundle` - 列表事实束
    ///
    /// # 返回
    /// 返回当前页供应商 ID 的稳定排序集合。
    fn bundle_ids(bundle: &super::SupplierListBundle) -> Vec<String> {
        let mut ids: Vec<String> = bundle.page.items.iter().map(|row| row.id.clone()).collect();
        ids.sort();
        ids
    }

    /// 关键词、能力、资质健康状态均在分页计数前生效，且总数不受页大小影响。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 组合筛选、`NotRegistered` 排除集、能力与资质交集及分页总数全部符合预期时通过。
    ///
    /// # 错误
    /// 任一筛选总数、分页总数或水合事实与预期不一致时测试失败。
    ///
    /// # 约束
    /// `#[ignore]` 由 Quality 在隔离副本集执行；总数断言覆盖分页前生效语义。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_list_bundle_applies_all_prefilters_before_paging() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_list")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let bundle = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(&list_input(|_| {}), &mut NoTransaction)
                .await
                .expect("列表事实束加载失败");
            assert_eq!(bundle.page.total, 4, "无筛选时总数应覆盖全部供应商");
            assert_eq!(bundle_ids(&bundle), vec!["sup-a", "sup-b", "sup-c", "sup-orphan"]);

            let paged = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.page_size = 1;
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("分页事实束加载失败");
            assert_eq!(paged.page.total, 4, "总数不得随页大小变化");
            assert_eq!(paged.page.items.len(), 1);

            let keyword = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.keyword = Some("供应商甲".to_string());
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("关键词事实束加载失败");
            assert_eq!(bundle_ids(&keyword), vec!["sup-a"], "主体名称命中应在分页前生效");

            let capability = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.capability_codes = vec![CapabilityCode::Physical];
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("能力筛选事实束加载失败");
            assert_eq!(bundle_ids(&capability), vec!["sup-a", "sup-b"]);

            let valid = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Valid);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("有效资质事实束加载失败");
            assert_eq!(bundle_ids(&valid), vec!["sup-a"]);

            let expiring = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Expiring30);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("临期资质事实束加载失败");
            assert_eq!(bundle_ids(&expiring), vec!["sup-a"]);

            let expired = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Expired);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("失效资质事实束加载失败");
            assert_eq!(bundle_ids(&expired), vec!["sup-c"]);

            let not_registered = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::NotRegistered);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("未登记资质事实束加载失败");
            assert_eq!(
                bundle_ids(&not_registered),
                vec!["sup-b", "sup-orphan"],
                "未登记分支应返回排除集，命中集合为空"
            );

            let intersection = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.capability_codes = vec![CapabilityCode::Physical];
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Valid);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("交集筛选事实束加载失败");
            assert_eq!(bundle_ids(&intersection), vec!["sup-a"]);

            assert!(
                bundle.parties.iter().any(|party| party.base.id == "party-a"),
                "水合事实应包含命中主体"
            );
            assert!(
                bundle
                    .revisions
                    .iter()
                    .any(|revision| revision.base.id == "partyrev-a"),
                "水合事实应包含主体当前修订"
            );
            assert!(
                bundle
                    .profiles
                    .iter()
                    .any(|profile| profile.base.id == "profile-a"),
                "水合事实应包含当前商务资料"
            );
        });
    }

    /// 详情事实束一次批量返回全部历史集合，缺失指针有明确语义。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 资质与关联批量齐套、商务名称齐套、主体缺失与供应商缺失语义正确时通过。
    ///
    /// # 错误
    /// 任一集合缺失、关联不齐或缺失语义不符时测试失败。
    ///
    /// # 约束
    /// `#[ignore]` 由 Quality 在隔离副本集执行；关联一次批量读取，不断言查询次数。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_detail_bundle_returns_batch_facts_and_missing_pointer_semantics() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_detail")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let bundle = fixture
                .db()
                .supplier()
                .load_supplier_detail_bundle(&SupplierAccountId::new("sup-a"), &mut NoTransaction)
                .await
                .expect("详情事实束加载失败")
                .expect("sup-a 事实束缺失");
            assert_eq!(bundle.supplier.base.id, "sup-a");
            assert_eq!(bundle.party.as_ref().expect("主体缺失").base.id, "party-a");
            assert_eq!(
                bundle.party_revision.as_ref().expect("当前修订缺失").base.id,
                "partyrev-a"
            );
            assert_eq!(bundle.capabilities.len(), 1);
            assert_eq!(bundle.qualifications.len(), 2);
            assert_eq!(
                bundle.qualification_links.len(),
                2,
                "两份资质的适用关联应一次批量读回"
            );
            assert_eq!(bundle.ratings.len(), 1);
            assert_eq!(bundle.commercial_profiles.len(), 1);
            assert_eq!(
                bundle.commercial_party_names.get("party-a").map(String::as_str),
                Some("供应商甲")
            );

            let orphan = fixture
                .db()
                .supplier()
                .load_supplier_detail_bundle(&SupplierAccountId::new("sup-orphan"), &mut NoTransaction)
                .await
                .expect("孤儿事实束加载失败")
                .expect("sup-orphan 事实束缺失");
            assert!(orphan.party.is_none(), "主体缺失时应为 None");
            assert!(orphan.party_revision.is_none(), "主体缺失时修订应为 None");
            assert!(orphan.capabilities.is_empty());
            assert!(orphan.qualifications.is_empty());

            let missing = fixture
                .db()
                .supplier()
                .load_supplier_detail_bundle(&SupplierAccountId::new("sup-missing"), &mut NoTransaction)
                .await
                .expect("缺失供应商查询失败");
            assert!(missing.is_none(), "供应商缺失时应返回 None");
        });
    }

    /// 列表候选约束查询的执行计划必须命中唯一索引且无集合扫描。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// `explain` 命中 `uk_supplier_accounts_id` 的 `IXSCAN` 且无 `COLLSCAN` 时通过。
    ///
    /// # 错误
    /// 索引未命中或出现集合扫描时测试失败。
    ///
    /// # 约束
    /// 不使用 `hint`；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_list_candidate_query_uses_index_without_collscan() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_explain")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": SUPPLIER_ACCOUNTS,
                        "filter": {
                            "id": { "$in": ["sup-a", "sup-b"] },
                            "deleted_at": 0_i64,
                        },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("候选约束查询 explain 失败");
            let rendered = format!("{explain:?}");
            assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
            assert!(
                rendered.contains("uk_supplier_accounts_id"),
                "explain 未命中 uk_supplier_accounts_id：{rendered}"
            );
            assert!(
                !rendered.contains("COLLSCAN"),
                "explain 出现 COLLSCAN：{rendered}"
            );

            let capability_explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": SUPPLIER_CAPABILITIES,
                        "filter": {
                            "supplier_id": "sup-a",
                            "deleted_at": 0_i64,
                        },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("能力查询 explain 失败");
            let capability_rendered = format!("{capability_explain:?}");
            assert!(
                capability_rendered.contains("IXSCAN"),
                "能力查询 explain 未使用 IXSCAN：{capability_rendered}"
            );
            assert!(
                !capability_rendered.contains("COLLSCAN"),
                "能力查询 explain 出现 COLLSCAN：{capability_rendered}"
            );
        });
    }

    /// 事实束查询复用调用方执行器，事务内可见同一会话写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 事务内写入的供应商在同一会话的列表与详情事实束中均可见时通过。
    ///
    /// # 错误
    /// 事务内重验不可见或提交失败时测试失败。
    ///
    /// # 约束
    /// 事务内重验必须复用调用方 executor，不得另开连接或独立事务；
    /// `#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_bundles_see_same_session_writes() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_txn")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let (party, revision) = party_with_revision("party-txn", "partyrev-txn", "供应商丁");
            let supplier = supplier_account("sup-txn", "party-txn", "SUP-TXN-1", None);
            let trans_capability = capability("cap-txn", "sup-txn", CapabilityCode::Physical);

            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    let party = party.clone();
                    let revision = revision.clone();
                    let supplier = supplier.clone();
                    let trans_capability = trans_capability.clone();
                    Box::pin(async move {
                        db.parties().create(&party, session).await?;
                        db.party_revisions().create(&revision, session).await?;
                        db.supplier_accounts().create(&supplier, session).await?;
                        db.supplier_capabilities()
                            .create(&trans_capability, session)
                            .await?;
                        let bundle = db
                            .supplier()
                            .load_supplier_list_bundle(
                                &SupplierListSearchInput {
                                    keyword: Some("SUP-TXN-1".to_string()),
                                    party_id: None,
                                    status: None,
                                    capability_codes: Vec::new(),
                                    qualification_types: Vec::new(),
                                    qualification_health: None,
                                    as_of: AS_OF.to_string(),
                                    page: 1,
                                    page_size: 20,
                                    sort_by: Some("created_at".to_string()),
                                    sort_ascending: false,
                                },
                                session,
                            )
                            .await?;
                        assert_eq!(bundle.page.total, 1, "事务内应能 read-your-writes");
                        let detail = db
                            .supplier()
                            .load_supplier_detail_bundle(&SupplierAccountId::new("sup-txn"), session)
                            .await?;
                        let detail = detail.expect("事务内详情事实束缺失");
                        assert_eq!(detail.capabilities.len(), 1);
                        Ok(())
                    })
                })
                .await
                .expect("同一会话事务读写失败");
        });
    }
}
