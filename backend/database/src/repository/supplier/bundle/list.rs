use crate::repository::owned::SupplierAccountRepository;
use std::collections::HashSet;

use entities::supplier::{CapabilityCode, QualificationType, SupplierQualification};
use erp_core::ids::{PartyId, SupplierAccountId};

use super::super::super::extensions::PartyExt;
use super::super::account::SupplierAccountFilter;
use super::super::{SupplierRepository, SUPPLIER_ACCOUNTS};
use super::{
    QualificationConstraintKind, SupplierListBundle, SupplierListSearchInput,
    SupplierQualificationHealthFilter,
};
use persistence_core::Executor;
use persistence_core::{Error, Result};

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
pub(super) fn qualification_constraint_kind(
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
pub(super) fn intersect_supplier_ids(
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
pub(super) fn qualification_expiry_cutoff(as_of: &str) -> Result<String> {
    let as_of = as_of
        .parse::<erp_core::common::time::BusinessDate>()
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
        let page = SupplierAccountRepository::new(self.db, SUPPLIER_ACCOUNTS)
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
}
