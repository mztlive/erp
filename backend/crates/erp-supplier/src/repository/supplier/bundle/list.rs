use erp_core::ids::{PartyId, SupplierAccountId};
use persistence_core::{Error, Executor, PageResult, Result};

use super::super::account::{SupplierAccountFilter, SupplierAccountRepositoryExt, SupplierAccountRow};
use super::super::{SUPPLIER_ACCOUNTS, SupplierRepository};
use super::{
    QualificationConstraintKind, SupplierListBundle, SupplierListSearchInput,
    SupplierQualificationHealthFilter,
};
use crate::entity::supplier::supplier_account::intersect_supplier_ids;
use crate::entity::supplier::{CapabilityCode, QualificationType, SupplierQualification};
use crate::repository::owned::SupplierAccountRepository;

/// 命中集合查询允许的资质健康状态（不含 `NotRegistered`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IncludedQualificationHealth {
    /// 仅按资质类型命中，不限制健康状态。
    ByType,
    /// 合同有效期缺少起始日或截止日。
    Unverified,
    /// 当前有效。
    Valid,
    /// 当前有效且 30 天内到期。
    Expiring30,
    /// 已失效。
    Expired,
}

/// 将列表筛选转为命中集合健康状态。
///
/// `NotRegistered` 由排除集路径处理，不属于命中集合，返回 `None`。
pub(super) fn included_qualification_health(
    health: Option<SupplierQualificationHealthFilter>,
) -> Option<IncludedQualificationHealth> {
    match health {
        None => Some(IncludedQualificationHealth::ByType),
        Some(SupplierQualificationHealthFilter::Unverified) => Some(IncludedQualificationHealth::Unverified),
        Some(SupplierQualificationHealthFilter::Valid) => Some(IncludedQualificationHealth::Valid),
        Some(SupplierQualificationHealthFilter::Expiring30) => Some(IncludedQualificationHealth::Expiring30),
        Some(SupplierQualificationHealthFilter::Expired) => Some(IncludedQualificationHealth::Expired),
        Some(SupplierQualificationHealthFilter::NotRegistered) => None,
    }
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
        let party_ids = input.keyword_party_ids.clone();
        let (supplier_ids, excluded_supplier_ids) = self
            .list_filter_id_constraints(
                &input.capability_codes,
                &input.qualification_types,
                input.qualification_health,
                &input.as_of,
                executor,
            )
            .await?;
        let capability_owner_ids =
            self.supplier_ids_by_capability_owners(&input.capability_owner_user_ids, executor).await?;
        let supplier_ids = intersect_supplier_ids(supplier_ids, capability_owner_ids);
        let filter = SupplierAccountFilter {
            keyword: input.keyword.clone(),
            party_id: input.party_id.clone(),
            party_ids,
            status: input.status,
            supplier_ids,
            excluded_supplier_ids,
            authorized_scope: input.authorized_scope.clone(),
            maintainer_user_ids: input.maintainer_user_ids.clone(),
            business_org_unit_ids: input.business_org_unit_ids.clone(),
            page: input.page,
            page_size: input.page_size,
            sort_by: input.sort_by.clone(),
            sort_ascending: input.sort_ascending,
        };
        let page = SupplierAccountRepository::new(self.db, SUPPLIER_ACCOUNTS)
            .search_supplier_accounts(&filter, executor)
            .await?;
        self.hydrate_supplier_list_page(page, executor).await
    }

    /// 为当前页批量装载列表展示所需的商务资料、能力与资质。
    ///
    /// # 参数
    /// * `page` - 已完成筛选与分页的供应商投影页
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回带当前页水合事实的列表事实束。
    ///
    /// # 错误
    /// 任一批量查询失败时返回错误。
    ///
    /// # 约束
    /// 查询次数固定为商务资料、能力、资质各一次，不随页内关联行数增加往返。
    async fn hydrate_supplier_list_page(
        &self,
        page: PageResult<SupplierAccountRow>,
        executor: &mut dyn Executor,
    ) -> Result<SupplierListBundle> {
        let party_ids: Vec<PartyId> = page.items.iter().map(|row| PartyId::new(&row.party_id)).collect();
        let profile_ids: Vec<String> =
            page.items.iter().filter_map(|row| row.current_commercial_profile_revision_id.clone()).collect();
        let supplier_ids: Vec<SupplierAccountId> =
            page.items.iter().map(|row| SupplierAccountId::new(&row.id)).collect();
        let profiles = self.list_commercial_profiles_by_ids(&profile_ids, executor).await?;
        let capabilities = self.list_capabilities_by_supplier_ids(&supplier_ids, executor).await?;
        let qualifications = self.list_qualifications_by_supplier_ids(&supplier_ids, executor).await?;
        Ok(SupplierListBundle { page, party_ids, profiles, capabilities, qualifications })
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
            Some(self.list_supplier_ids_by_active_capability_codes(capability_codes, as_of, executor).await?)
        };
        let (qualification_ids, excluded_qualification_ids) =
            self.list_filter_qualification_constraints(qualification_types, health, as_of, executor).await?;
        Ok((intersect_supplier_ids(capability_ids, qualification_ids), excluded_qualification_ids))
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
                Some(self.list_supplier_ids_by_qualification_types(qualification_types, executor).await?),
            )),
            QualificationConstraintKind::Included => {
                debug_assert!(
                    !matches!(health, Some(SupplierQualificationHealthFilter::NotRegistered)),
                    "NotRegistered 应走 Excluded"
                );
                match included_qualification_health(health) {
                    Some(health) => {
                        self.list_included_qualification_constraints(
                            qualification_types,
                            health,
                            as_of,
                            executor,
                        )
                        .await
                    },
                    None => Ok((
                        None,
                        Some(
                            self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                                .await?,
                        ),
                    )),
                }
            },
        }
    }

    /// 查询命中集合内的资质类型和健康状态对应的供应商角色 ID 约束。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `health` - 命中集合健康状态；`ByType` 表示仅按类型命中
    /// * `as_of` - 当前业务日字符串
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回应命中的供应商 ID 集合。
    ///
    /// # 错误
    /// 到期窗口计算或任一仓储查询失败时返回错误。
    async fn list_included_qualification_constraints(
        &self,
        qualification_types: &[QualificationType],
        health: IncludedQualificationHealth,
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        match health {
            IncludedQualificationHealth::ByType => Ok((
                Some(self.list_supplier_ids_by_qualification_types(qualification_types, executor).await?),
                None,
            )),
            IncludedQualificationHealth::Unverified => Ok((
                Some(
                    self.list_supplier_ids_by_unverified_qualifications(qualification_types, executor)
                        .await?,
                ),
                None,
            )),
            IncludedQualificationHealth::Valid => Ok((
                Some(
                    self.list_supplier_ids_by_valid_qualifications(qualification_types, as_of, executor)
                        .await?,
                ),
                None,
            )),
            IncludedQualificationHealth::Expiring30 => {
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
            },
            IncludedQualificationHealth::Expired => Ok((
                Some(
                    self.list_supplier_ids_by_expired_qualifications(qualification_types, as_of, executor)
                        .await?,
                ),
                None,
            )),
        }
    }
}
