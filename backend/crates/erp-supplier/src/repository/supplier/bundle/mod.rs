use erp_core::ids::PartyId;
use persistence_core::PageResult;

use super::SupplierAccountRow;
use crate::entity::supplier::{
    CapabilityCode, QualificationType, SupplierAccount, SupplierAccountStatus, SupplierCapability,
    SupplierCommercialProfileRevision, SupplierQualification, SupplierQualificationCapability,
    SupplierRatingRevision,
};

mod detail;
mod list;

/// `PROC-R03`/`PROC-R04` 列表与详情事实束的真实 MongoDB 验收（隔离库，Quality 单独执行）。
#[cfg(test)]
mod proc_supplier_io_mongo_tests;
#[cfg(test)]
mod tests;

/// 供应商列表仓储侧资质健康状态筛选。
///
/// # 约束
/// 仓储自有类型，避免数据库层依赖 Service DTO；语义与 Service 侧
/// `SupplierQualificationHealth` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierQualificationHealthFilter {
    /// 合同有效期缺少起始日或截止日。
    Unverified,
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
    /// 关键词命中的主体 ID，由消费方 PartyFactsPort 预先解析。
    pub keyword_party_ids: Option<Vec<PartyId>>,
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
    /// 已证明的供应商责任范围。
    pub authorized_scope: crate::repository::scope::SupplierReadScope,
    /// 维护人筛选；`None` 表示不筛选。
    pub maintainer_user_ids: Option<Vec<String>>,
    /// 业务组织筛选；`None` 表示不筛选。
    pub business_org_unit_ids: Option<Vec<String>>,
    /// 能力负责人筛选得到的供应商 ID；`None` 表示不筛选。
    pub capability_owner_user_ids: Vec<String>,
}

/// 供应商列表事实束。
///
/// # 约束
/// 仅承载持久化事实与投影行，不含 View 映射与授权结论。
#[derive(Debug)]
pub struct SupplierListBundle {
    /// 当前页投影行与总数。
    pub page: PageResult<SupplierAccountRow>,
    /// 当前页供应商关联的主体 ID，供消费方组装主体事实。
    pub party_ids: Vec<PartyId>,
    /// 命中的商务资料版本集合。
    pub profiles: Vec<SupplierCommercialProfileRevision>,
    /// 当前页供应商的能力集合，供列表折叠有效能力代码。
    pub capabilities: Vec<SupplierCapability>,
    /// 当前页供应商的资质集合，供列表折叠健康状态与类型摘要。
    pub qualifications: Vec<SupplierQualification>,
}

/// 供应商详情事实束。
///
/// # 约束
/// 仅承载持久化事实；敏感令牌签发与 View 映射保留在 Service。
#[derive(Debug)]
pub struct SupplierDetailBundle {
    /// 供应商角色。
    pub supplier: SupplierAccount,
    /// 关联主体 ID。
    pub party_id: PartyId,
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
    /// 商务版本引用的签约/付款主体 ID。
    pub commercial_party_ids: Vec<PartyId>,
}
