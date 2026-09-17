use application_core::{normalized_text, page_or_default, page_size_or_default};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{PageParams, normalize_sort};
use crate::entity::supplier::{
    CapabilityCode, QualificationHealth, QualificationType, SupplierAccountStatus,
};
use crate::error::{Error, Result};

/// 供应商角色列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SUPPLIER_SORT_FIELDS: &[&str] = &["created_at", "supplier_no", "status"];

/// 供应商角色列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierListParams {
    /// 跨页与导出必须使用前一页的当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 当前整体维护人 ID，逗号分隔，最多 100 项。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 供给能力负责人 ID，逗号分隔，最多 100 项。
    pub capability_owner_user_ids: Option<application_core::QueryIds>,
    /// 当前业务组织，逗号分隔，最多 100 项。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 供应商编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID（精确匹配）。
    pub party_id: Option<erp_core::ids::PartyId>,
    /// 启停状态筛选。
    pub status: Option<SupplierAccountStatus>,
    /// 供应能力代码，多项以逗号分隔；命中任一当前有效能力即可。
    pub capability_codes: Option<String>,
    /// 资质类型代码，多项以逗号分隔；命中任一类型即可。
    pub qualification_types: Option<String>,
    /// 资质资料状态筛选。
    pub qualification_health: Option<SupplierQualificationHealth>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`supplier_no`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的供应商角色列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupplierListQuery {
    /// 当前整体维护人。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 供给能力负责人。
    pub capability_owner_user_ids: Option<application_core::QueryIds>,
    /// 当前业务组织。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 是否包含下级组织。
    pub include_descendants: Option<bool>,
    /// 供应商编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID。
    pub party_id: Option<erp_core::ids::PartyId>,
    /// 启停状态筛选。
    pub status: Option<SupplierAccountStatus>,
    /// 命中任一当前有效能力的能力代码。
    pub capability_codes: Vec<CapabilityCode>,
    /// 命中任一资质类型的类型代码。
    pub qualification_types: Vec<QualificationType>,
    /// 资质资料状态筛选。
    pub qualification_health: Option<SupplierQualificationHealth>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierListParams {
    /// 归一化供应商角色列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SupplierListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SUPPLIER_SORT_FIELDS)?;
        Ok(SupplierListQuery {
            owner_user_ids: self.owner_user_ids.clone(),
            capability_owner_user_ids: self.capability_owner_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            keyword: normalized_text(self.keyword.as_deref()),
            party_id: self.party_id.clone(),
            status: self.status,
            capability_codes: normalize_capability_codes(self.capability_codes.as_deref())?,
            qualification_types: normalize_qualification_types(self.qualification_types.as_deref())?,
            qualification_health: self.qualification_health,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商资质资料的当前状态。
///
/// 状态以当前业务日、资质启停状态和有效期共同计算；`NotRegistered` 仅表示
/// 尚未登记对应资质记录，不推断业务规则中的必备资质。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierQualificationHealth {
    /// 合同有效期缺少起始日或截止日。
    Unverified,
    /// 当前有效。
    Valid,
    /// 当前有效，且将在 30 天内到期。
    Expiring30,
    /// 已标记失效，或有效期已过。
    Expired,
    /// 尚未登记资质记录。
    NotRegistered,
}

impl From<QualificationHealth> for SupplierQualificationHealth {
    /// 将实体折叠结果映射为列表响应枚举。
    ///
    /// DTO→仓储的同名单向转换见 [`SupplierQualificationHealth::to_repository_filter`]
    /// （erp-supplier-008）；调用处只剩一句转换，映射关系集中一处维护。
    ///    ///
    /// # 参数
    /// * `health` - 实体层资质健康状态
    ///
    /// # 返回
    /// 返回同名的 HTTP 契约枚举值。
    fn from(health: QualificationHealth) -> Self {
        match health {
            QualificationHealth::Unverified => Self::Unverified,
            QualificationHealth::Valid => Self::Valid,
            QualificationHealth::Expiring30 => Self::Expiring30,
            QualificationHealth::Expired => Self::Expired,
            QualificationHealth::NotRegistered => Self::NotRegistered,
        }
    }
}

impl SupplierQualificationHealth {
    /// 映射为仓储侧健康筛选（erp-supplier-008）。
    ///
    /// 两枚举变体一一对应；新增健康态时只改此处与上方的 `From`，
    /// 调用处的 5 分支 `match` 已收敛为一句转换。
    ///
    /// # 参数
    /// * `self` - DTO 资质健康状态
    ///
    /// # 返回
    /// 返回仓储侧同名筛选枚举。
    pub(crate) fn to_repository_filter(self) -> crate::repository::SupplierQualificationHealthFilter {
        type HealthFilter = crate::repository::SupplierQualificationHealthFilter;
        match self {
            Self::Unverified => HealthFilter::Unverified,
            Self::Valid => HealthFilter::Valid,
            Self::Expiring30 => HealthFilter::Expiring30,
            Self::Expired => HealthFilter::Expired,
            Self::NotRegistered => HealthFilter::NotRegistered,
        }
    }
}

/// 归一化逗号分隔的供应能力代码。
fn normalize_capability_codes(value: Option<&str>) -> Result<Vec<CapabilityCode>> {
    normalize_code_list(value, "供应能力", |code| match code {
        "physical" => Some(CapabilityCode::Physical),
        "virtual" => Some(CapabilityCode::Virtual),
        "offline_service" => Some(CapabilityCode::OfflineService),
        "api" => Some(CapabilityCode::Api),
        "printing" => Some(CapabilityCode::Printing),
        _ => None,
    })
}

/// 归一化逗号分隔的资质类型代码。
fn normalize_qualification_types(value: Option<&str>) -> Result<Vec<QualificationType>> {
    normalize_code_list(value, "资质类型", |code| match code {
        "certificate" => Some(QualificationType::Certificate),
        "contract" => Some(QualificationType::Contract),
        "authorization" => Some(QualificationType::Authorization),
        "food_license" => Some(QualificationType::FoodLicense),
        "legal_person_id" => Some(QualificationType::LegalPersonId),
        _ => None,
    })
}

/// 清理、去重并校验逗号分隔的固定枚举代码。
fn normalize_code_list<T: Copy + Eq>(
    value: Option<&str>,
    field: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    let mut codes = Vec::new();
    for raw in value.split(',').map(str::trim).filter(|code| !code.is_empty()) {
        let code = parse(raw).ok_or_else(|| Error::ValidationError(format!("不支持的{field}: {raw}")))?;
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    Ok(codes)
}
