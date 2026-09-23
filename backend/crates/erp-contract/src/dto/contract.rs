//! 域 D12 `contract` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；业务日期 `YYYY-MM-DD`；
//! 金额按 P0 约定字符串序列化（本域无金额字段）。
//!
//! 契约来源：erp-client `features/contracts`（W04）；本域接口按后端实体字段
//! 形状提供，与前端 mock 的 `ContractCenterView` 差异见批次报告「契约变更」。

use application_core::{normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, FileAssetId, PartyId};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::contract::{ArchiveSource, ContractStatus};
use crate::error::Result;

/// 合同列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const CONTRACT_SORT_FIELDS: &[&str] = &[
    "created_at",
    "contract_no",
    "customer",
    "settlement",
    "validity",
    "revision",
    "sales",
    "owner",
    "expiry_priority",
];

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
/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 contract_no 需要按「空白视为空」拒绝，落入 HTTP 400）。
use application_core::non_blank;
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

/// 合同首次归档请求（W04 上传 PDF：合同身份 + 首个不可变版本 + PDF 关联原子形成）。
///
/// `contract_pdf_file_id` 由 D05 文件资产接口上传后获得；本域只记录关联。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateContractRequest {
    /// 合同编号（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "合同编号不能为空"))]
    pub contract_no: String,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 本版本已签署合同 PDF 的文件资产 ID（D05 上传产物）。
    pub contract_pdf_file_id: FileAssetId,
    /// 归档来源。
    #[serde(default)]
    pub archive_source: Option<ArchiveSource>,
    /// 客户名称快照。
    #[validate(custom(function = "non_blank", message = "客户名称不能为空"))]
    pub customer_name: String,
    /// 结算主体名称快照。
    #[validate(custom(function = "non_blank", message = "结算主体名称不能为空"))]
    pub settlement_party_name: String,
    /// 付款条件代码（结构化快照）。
    #[validate(custom(function = "non_blank", message = "付款条件代码不能为空"))]
    pub payment_term_code: String,
    /// 付款条件名称（结构化快照）。
    #[validate(custom(function = "non_blank", message = "付款条件名称不能为空"))]
    pub payment_term_name: String,
    /// 开票类型（结构化快照）。
    #[validate(custom(function = "non_blank", message = "开票类型不能为空"))]
    pub invoice_type: String,
    /// 税点（结构化快照）。
    #[validate(custom(function = "non_blank", message = "税点不能为空"))]
    pub tax_point: String,
    /// 合同有效期起（`YYYY-MM-DD`）。
    pub valid_from: BusinessDate,
    /// 合同有效期止（`YYYY-MM-DD`）；缺省表示长期。
    pub valid_to: Option<BusinessDate>,
    /// 签订日期（`YYYY-MM-DD`）。
    pub signed_at: BusinessDate,
}

/// 合同 PDF 一次上传命令。
///
/// 文件字节由 HTTP 层写入对象存储；文件资产元数据、合同与首个修订由服务端在
/// 同一个数据库事务登记。`settlement_party_id` 为空时使用客户自有主体。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UploadContractRequest {
    /// 合同编号（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "合同编号不能为空"))]
    pub contract_no: String,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 可选结算主体；空时由服务端取客户自有主体。
    pub settlement_party_id: Option<PartyId>,
    /// 客户名称快照。
    #[validate(custom(function = "non_blank", message = "客户名称不能为空"))]
    pub customer_name: String,
    /// 结算主体名称快照。
    #[validate(custom(function = "non_blank", message = "结算主体名称不能为空"))]
    pub settlement_party_name: String,
    /// 付款条件代码。
    #[validate(custom(function = "non_blank", message = "付款条件代码不能为空"))]
    pub payment_term_code: String,
    /// 付款条件名称。
    #[validate(custom(function = "non_blank", message = "付款条件名称不能为空"))]
    pub payment_term_name: String,
    /// 开票类型。
    #[validate(custom(function = "non_blank", message = "开票类型不能为空"))]
    pub invoice_type: String,
    /// 税点。
    #[validate(custom(function = "non_blank", message = "税点不能为空"))]
    pub tax_point: String,
    /// 合同有效期起。
    pub valid_from: BusinessDate,
    /// 合同有效期止；缺省表示长期。
    pub valid_to: Option<BusinessDate>,
    /// 签订日期。
    pub signed_at: BusinessDate,
}

/// 合同 PDF 一次上传结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UploadContractView {
    /// 合同稳定身份。
    pub id: String,
    /// 合同编号。
    pub contract_no: String,
    /// 首个不可变修订身份。
    pub revision_id: String,
    /// 首个修订序号，固定为 1。
    pub revision_no: u32,
    /// 文件资产身份。
    pub file_asset_id: String,
    /// 原始文件名。
    pub file_name: String,
    /// 创建时间。
    pub created_at: u64,
}

/// 追加合同版本请求（归档后续 PDF 版本，乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ArchiveContractRevisionRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 本版本已签署合同 PDF 的文件资产 ID（D05 上传产物）。
    pub contract_pdf_file_id: FileAssetId,
    /// 归档来源。
    #[serde(default)]
    pub archive_source: Option<ArchiveSource>,
    /// 客户名称快照。
    #[validate(custom(function = "non_blank", message = "客户名称不能为空"))]
    pub customer_name: String,
    /// 结算主体名称快照。
    #[validate(custom(function = "non_blank", message = "结算主体名称不能为空"))]
    pub settlement_party_name: String,
    /// 付款条件代码（结构化快照）。
    #[validate(custom(function = "non_blank", message = "付款条件代码不能为空"))]
    pub payment_term_code: String,
    /// 付款条件名称（结构化快照）。
    #[validate(custom(function = "non_blank", message = "付款条件名称不能为空"))]
    pub payment_term_name: String,
    /// 开票类型（结构化快照）。
    #[validate(custom(function = "non_blank", message = "开票类型不能为空"))]
    pub invoice_type: String,
    /// 税点（结构化快照）。
    #[validate(custom(function = "non_blank", message = "税点不能为空"))]
    pub tax_point: String,
    /// 合同有效期起（`YYYY-MM-DD`）。
    pub valid_from: BusinessDate,
    /// 合同有效期止（`YYYY-MM-DD`）；缺省表示长期。
    pub valid_to: Option<BusinessDate>,
    /// 签订日期（`YYYY-MM-DD`）。
    pub signed_at: BusinessDate,
}

/// 终止合同请求（乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TerminateContractRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 合同列表的客户可见范围。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractListScope {
    /// 当前 DataScope 已授权合同，不再表示未授权全量。
    #[default]
    All,
    /// 仅当前用户有效主责或协作客户下的合同；只收窄授权结果。
    Assigned,
}

/// 合同列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContractListParams {
    /// 跨页与导出必须使用前一页的当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 当前业务负责人 ID，逗号分隔，最多 100 项；只收窄授权结果。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 当前主负责人所属组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 合同号、客户编号/名称、结算主体、当前负责人关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
    /// 快捷状态或到期条件。
    pub metric: Option<ContractMetric>,
    /// 结算主体精确筛选。
    pub settlement_party_id: Option<String>,
    /// 合同编号（字面量模糊筛选）。
    pub contract_no: Option<String>,
    /// 客户筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 客户归属可见范围；缺省视为 [`ContractListScope::All`]。
    pub scope: Option<ContractListScope>,
    /// 合同状态筛选。
    pub status: Option<ContractStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`contract_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的合同列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContractListQuery {
    /// 当前负责人精确身份条件。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 当前主负责人所属组织，只收窄授权结果。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
    /// 合同号、客户编号/名称、结算主体、当前负责人关键词。
    pub q: Option<String>,
    /// 快捷状态或到期条件。
    pub metric: Option<ContractMetric>,
    /// 结算主体精确筛选。
    pub settlement_party_id: Option<String>,
    /// 合同编号筛选。
    pub contract_no: Option<String>,
    /// 客户筛选。
    pub customer_id: Option<String>,
    /// 客户归属可见范围。
    pub scope: ContractListScope,
    /// 合同状态筛选。
    pub status: Option<ContractStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ContractListParams {
    /// 归一化合同列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验；
    /// 未传 `scope` 时视为 [`ContractListScope::All`]。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ContractListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, CONTRACT_SORT_FIELDS)?;
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::error::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(ContractListQuery {
            owner_user_ids: self.owner_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            q: normalized_text(self.q.as_deref()),
            metric: self.metric,
            settlement_party_id: normalized_text(self.settlement_party_id.as_deref()),
            contract_no: normalized_text(self.contract_no.as_deref()),
            customer_id: self.customer_id.as_ref().map(ToString::to_string),
            scope: self.scope.unwrap_or_default(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 合同响应视图（列表行，契约形状）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContractView {
    /// 实体主键。
    pub id: String,
    /// 合同编号。
    pub contract_no: String,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 结算主体。
    pub settlement_party_id: String,
    /// 合同状态。
    pub status: ContractStatus,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 当前合同版本摘要；列表不得要求客户端逐行读取详情。
    pub current_revision: Option<ContractRevisionView>,
    /// 客户编号。
    pub customer_no: Option<String>,
    /// 当前客户负责人账号。
    pub owner_user_id: Option<String>,
    /// 当前客户负责人显示名。
    pub owner_user_name: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
}

/// 合同版本响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContractRevisionView {
    /// 实体主键。
    pub id: String,
    /// 聚合内版本号。
    pub revision_no: u32,
    /// 本版本已签署合同 PDF 的文件资产 ID。
    pub contract_pdf_file_id: String,
    /// 归档来源。
    pub archive_source: ArchiveSource,
    /// 客户名称快照。
    pub customer_name: String,
    /// 结算主体名称快照。
    pub settlement_party_name: String,
    /// 付款条件代码（结构化快照）。
    pub payment_term_code: String,
    /// 付款条件名称（结构化快照）。
    pub payment_term_name: String,
    /// 开票类型（结构化快照）。
    pub invoice_type: String,
    /// 税点（结构化快照）。
    pub tax_point: String,
    /// 合同有效期起（`YYYY-MM-DD`）。
    pub valid_from: BusinessDate,
    /// 合同有效期止。
    pub valid_to: Option<BusinessDate>,
    /// 签订日期（`YYYY-MM-DD`）。
    pub signed_at: BusinessDate,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 合同详情视图（合同 + 全部版本时间线，W04 对象中心）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContractDetailView {
    /// 当前客户跟进负责人。
    pub owner_user_id: Option<String>,
    pub owner_user_name: Option<String>,
    /// 实体主键。
    pub id: String,
    /// 合同编号。
    pub contract_no: String,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 结算主体。
    pub settlement_party_id: String,
    /// 合同状态。
    pub status: ContractStatus,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
    /// 版本时间线（新版本在前）。
    pub revisions: Vec<ContractRevisionView>,
}

impl From<crate::entity::contract::Contract> for ContractView {
    /// 从合同实体构造列表行视图。
    ///
    /// # 参数
    /// * `contract` - 合同实体
    ///
    /// # 返回
    /// 返回列表行视图。
    fn from(contract: crate::entity::contract::Contract) -> Self {
        Self {
            id: contract.base.id,
            contract_no: contract.contract_no,
            customer_id: contract.customer_id.to_string(),
            settlement_party_id: contract.settlement_party_id.to_string(),
            status: contract.stable.status,
            current_revision_id: contract.stable.current_revision_id,
            current_revision: None,
            customer_no: None,
            owner_user_id: None,
            owner_user_name: None,
            created_at: contract.base.created_at,
            version: contract.base.version,
        }
    }
}

impl From<crate::entity::contract::ContractRevision> for ContractRevisionView {
    /// 从合同版本实体构造版本视图。
    ///
    /// # 参数
    /// * `revision` - 合同版本实体
    ///
    /// # 返回
    /// 返回版本视图。
    fn from(revision: crate::entity::contract::ContractRevision) -> Self {
        Self {
            id: revision.base.id,
            revision_no: revision.revision.revision_no,
            contract_pdf_file_id: revision.contract_pdf_file_id.to_string(),
            archive_source: revision.archive_source,
            customer_name: revision.customer_snapshot.customer_name,
            settlement_party_name: revision.settlement_party_snapshot.settlement_party_name,
            payment_term_code: revision.payment_term_snapshot.payment_term_code,
            payment_term_name: revision.payment_term_snapshot.payment_term_name,
            invoice_type: revision.invoice_requirement_snapshot.invoice_type,
            tax_point: revision.invoice_requirement_snapshot.tax_point,
            valid_from: revision.valid_from,
            valid_to: revision.valid_to,
            signed_at: revision.signed_at,
            created_at: revision.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::CustomerAccountId;

    use super::{SortDir, normalize_sort};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" contract_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "contract_no"],
        )
        .unwrap();
        assert_eq!(field, "contract_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn contract_list_params_normalize_filters_and_paging() {
        use serde_json::json;

        use super::ContractListParams;
        use crate::entity::contract::ContractStatus;

        let params: ContractListParams = serde_json::from_value(json!({
            "contract_no": " HT-2026 ",
            "customer_id": "cust-1",
            "status": "EFFECTIVE",
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.contract_no.as_deref(), Some("HT-2026"));
        assert_eq!(query.customer_id.as_deref(), Some("cust-1"));
        assert_eq!(query.scope, super::ContractListScope::All);
        assert_eq!(query.status, Some(ContractStatus::Effective));

        let assigned: ContractListParams = serde_json::from_value(json!({
            "scope": "assigned",
        }))
        .unwrap();
        assert_eq!(assigned.normalized().unwrap().scope, super::ContractListScope::Assigned);
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
    }

    #[test]
    fn contract_list_params_accept_scope_version_and_org_filters() {
        use serde_json::json;

        use super::ContractListParams;

        let params: ContractListParams = serde_json::from_value(json!({
            "page": 2,
            "scope_version": "v1",
            "owner_user_ids": "a,b",
            "org_unit_ids": "org-1,org-2",
            "include_descendants": true
        }))
        .unwrap();
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        let query = params.normalized().unwrap();
        assert_eq!(query.org_unit_ids.unwrap().as_slice(), &["org-1".to_string(), "org-2".to_string()]);
        assert_eq!(query.include_descendants, Some(true));
        assert!(serde_json::from_value::<ContractListParams>(json!({"owner": "张三"})).is_err());
        assert!(serde_json::from_value::<ContractListParams>(json!({"owner_name": "张三"})).is_err());
    }

    #[test]
    fn customer_id_serializes_as_transparent_string() {
        assert_eq!(serde_json::to_string(&CustomerAccountId::new("cust-1")).unwrap(), "\"cust-1\"");
    }

    #[test]
    fn status_and_archive_source_keep_json_wire_contracts() {
        use crate::entity::contract::{ArchiveSource, ContractStatus};

        assert_eq!(serde_json::to_string(&ContractStatus::Effective).unwrap(), "\"EFFECTIVE\"");
        assert_eq!(serde_json::to_string(&ContractStatus::Terminated).unwrap(), "\"TERMINATED\"");
        assert_eq!(serde_json::to_string(&ContractStatus::Expired).unwrap(), "\"EXPIRED\"");
        assert_eq!(serde_json::to_string(&ArchiveSource::ContractCenter).unwrap(), "\"CONTRACT_CENTER\"");
        assert_eq!(
            serde_json::to_string(&ArchiveSource::SalesOrderCreate).unwrap(),
            "\"SALES_ORDER_CREATE\""
        );
    }

    #[test]
    fn create_and_upload_requests_keep_json_field_contracts() {
        use serde_json::json;

        use super::{CreateContractRequest, UploadContractRequest};

        let created: CreateContractRequest = serde_json::from_value(json!({
            "contract_no": "HT-1",
            "customer_id": "cust-1",
            "settlement_party_id": "party-1",
            "contract_pdf_file_id": "file-1",
            "archive_source": "SALES_ORDER_CREATE",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "signed_at": "2025-12-20"
        }))
        .unwrap();
        assert_eq!(created.contract_no, "HT-1");
        assert_eq!(created.archive_source, Some(crate::entity::contract::ArchiveSource::SalesOrderCreate));

        let unknown = serde_json::from_value::<UploadContractRequest>(json!({
            "contract_no": "HT-1",
            "customer_id": "cust-1",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "signed_at": "2025-12-20",
            "extra": true
        }));
        assert!(unknown.is_err());
    }

    #[test]
    fn contract_view_json_keeps_list_row_shape() {
        use serde_json::json;

        use super::ContractView;
        use crate::entity::contract::ContractStatus;

        let view = ContractView {
            id: "c-1".to_string(),
            contract_no: "HT-1".to_string(),
            customer_id: "cust-1".to_string(),
            settlement_party_id: "party-1".to_string(),
            status: ContractStatus::Effective,
            current_revision_id: Some("rev-1".to_string()),
            current_revision: None,
            customer_no: Some("C-1".to_string()),
            owner_user_id: Some("user-1".to_string()),
            owner_user_name: Some("张三".to_string()),
            created_at: 1_700_000_000,
            version: 2,
        };
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            json!({
                "id": "c-1",
                "contract_no": "HT-1",
                "customer_id": "cust-1",
                "settlement_party_id": "party-1",
                "status": "EFFECTIVE",
                "current_revision_id": "rev-1",
                "current_revision": null,
                "customer_no": "C-1",
                "owner_user_id": "user-1",
                "owner_user_name": "张三",
                "created_at": 1_700_000_000,
                "version": 2
            })
        );
    }
}

/// 合同快捷筛选，与列表指标采用相同规则。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractMetric {
    All,
    Effective,
    #[serde(rename = "expiring_30d")]
    Expiring30d,
    Expired,
    Terminated,
}

/// 合同范围指标；保留授权、归属范围、组织、客户、合同号及状态条件。
/// 不随关键词、快捷指标、负责人、结算主体、排序或页码变化。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractMetrics {
    pub all: i64,
    pub effective: i64,
    pub expiring_30d: i64,
    pub expired: i64,
    pub terminated: i64,
}

/// 合同分页列表；保留 Page 字段并追加授权基础范围内的指标。
#[derive(Debug, Clone, Serialize)]
pub struct ContractListView {
    /// 当前客户主负责人构成合同跟进责任。
    pub ownership_basis: &'static str,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`；有规则但对象为空时不设置。
    pub empty_reason: Option<&'static str>,
    /// 当前合同范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
    #[serde(flatten)]
    pub page: PageView<ContractView>,
    pub metrics: ContractMetrics,
}
