//! 域 D12 `contract` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；业务日期 `YYYY-MM-DD`；
//! 金额按 P0 约定字符串序列化（本域无金额字段）。
//!
//! 契约来源：erp-client `features/contracts`（W04）；本域接口按后端实体字段
//! 形状提供，与前端 mock 的 `ContractCenterView` 差异见批次报告「契约变更」。

use entities::common::time::BusinessDate;
use entities::contract::{ArchiveSource, ContractStatus};
use entities::ids::{CustomerAccountId, FileAssetId, PartyId};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 合同列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const CONTRACT_SORT_FIELDS: &[&str] = &["created_at", "contract_no"];

/// 排序方向。
pub use crate::query::SortDir;

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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 contract_no 需要按「空白视为空」拒绝，落入 HTTP 400）。
use crate::query::non_blank;

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
    /// 不按客户归属收窄（合同中心默认，兼容既有全量列表）。
    #[default]
    All,
    /// 仅当前用户有效归属（OWNER 或 COLLABORATOR）客户下的合同。
    Assigned,
}

/// 合同列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ContractListParams {
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
        Ok(ContractListQuery {
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

impl From<entities::contract::Contract> for ContractView {
    /// 从合同实体构造列表行视图。
    ///
    /// # 参数
    /// * `contract` - 合同实体
    ///
    /// # 返回
    /// 返回列表行视图。
    fn from(contract: entities::contract::Contract) -> Self {
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

impl From<entities::contract::ContractRevision> for ContractRevisionView {
    /// 从合同版本实体构造版本视图。
    ///
    /// # 参数
    /// * `revision` - 合同版本实体
    ///
    /// # 返回
    /// 返回版本视图。
    fn from(revision: entities::contract::ContractRevision) -> Self {
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
    use super::{normalize_sort, SortDir};
    use entities::ids::CustomerAccountId;

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
        use super::ContractListParams;
        use entities::contract::ContractStatus;
        use serde_json::json;

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
        assert_eq!(
            assigned.normalized().unwrap().scope,
            super::ContractListScope::Assigned
        );
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
    }

    #[test]
    fn customer_id_serializes_as_transparent_string() {
        assert_eq!(
            serde_json::to_string(&CustomerAccountId::new("cust-1")).unwrap(),
            "\"cust-1\""
        );
    }
}
