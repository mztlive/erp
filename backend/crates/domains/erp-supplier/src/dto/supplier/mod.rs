//! 域 D09 `supplier` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；业务日期一律 `YYYY-MM-DD`；时间一律秒级时间戳；
//! 金额/税率按 P0 约定序列化为字符串（`invoice_tax_rate` 为 `Rate` 定点小数）。
//!
//! 按视图归属拆分（erp-supplier-001）：`list` 承载列表查询与资质健康折叠，
//! `views` 承载角色/商务/能力/资质/评级视图与 `From` 映射，`reveal` 承载敏感
//! 揭示契约，`profile_mutation` 承载资料输入与根请求；分页基段留在此处。
//! 序列化形状与校验语义保持不变，既有 `crate::dto::supplier::X` 路径经重导出兼容。

pub mod list;
pub mod profile_mutation;
pub mod reveal;
pub mod views;

/// 排序方向。
pub use application_core::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use application_core::normalize_sort;
pub(crate) use list::SupplierListQuery;
pub use list::{SupplierListParams, SupplierQualificationHealth};
pub use profile_mutation::{
    SaveSupplierProfileRequest, SupplierProfileAddressInput, SupplierProfileBankAccountInput,
    SupplierProfileCapabilityOwnerInput, SupplierProfileContactInput, SupplierProfileMutationView,
    SupplierProfileQualificationInput, SupplierProfileRatingInput,
};
pub use reveal::{RevealSupplierSensitiveRequest, SupplierSensitiveFieldView, SupplierSensitiveRevealView};
pub use views::{
    CommercialProfileView, SupplierCapabilityView, SupplierDetailView, SupplierQualificationView,
    SupplierRatingView, SupplierView,
};
