//! 域 D12 `contract` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 合同首次归档：跨集合（contract + contract_revision + 审计）→
//!   `persistence_core::Transactional::with_transaction`（仓储方法
//!   `create_contract_with_revision` 声明「必须收到事务执行器」）；
//! - 追加版本 / 终止：跨集合（版本 + 主表 CAS + 审计）→ 同一事务模板；
//! - 列表 / 详情：单集合无跨步骤原子性要求 → `&mut NoTransaction`。
//!
//! 跨域读取（客户存在性、归属、账号名、附件存在）走消费方 Port；
//! 文件资产写入与跨域根事务由 `erp-processes` 持有，本模块提供事务内接口。

use std::sync::Arc;

use crate::dto::contract::{
    ArchiveContractRevisionRequest, ContractDetailView, ContractRevisionView, ContractView,
    CreateContractRequest, TerminateContractRequest, UploadContractRequest,
};
use crate::entity::contract::{
    ArchiveSource, Contract, ContractData, ContractRevision, ContractRevisionData,
};
use crate::error::{Error, Result};
use crate::ports::{
    AccountNamePort, ContractAuditPort, CustomerAccountFact, CustomerAssignmentFactsPort, CustomerFactsPort,
    FileAssetFact, FileAssetFactsPort,
};
use crate::repository::ContractExt;
use application_core::AuditActor;
use erp_core::ids::{ContractId, ContractRevisionId, CustomerAccountId, FileAssetId, PartyId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

mod query;
mod scope;

/// 已规划、尚未持久化的合同身份与首个/下一个不可变修订。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedContractArchive {
    /// 合同稳定身份。
    pub contract: Contract,
    /// 不可变合同修订。
    pub revision: ContractRevision,
}

/// 合同服务。
///
/// 提供合同归档、版本追加、终止与查询编排。
pub struct ContractService {
    db: Database,
    audit: Arc<dyn ContractAuditPort>,
    customers: Arc<dyn CustomerFactsPort>,
    assignments: Arc<dyn CustomerAssignmentFactsPort>,
    accounts: Arc<dyn AccountNamePort>,
    files: Arc<dyn FileAssetFactsPort>,
}

impl ContractService {
    /// 创建合同服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `customers` - 客户存在性与编号事实
    /// * `assignments` - 客户归属可见范围
    /// * `accounts` - 负责人显示名
    /// * `files` - 合同 PDF 附件存在性
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(
        db: Database,
        audit: Arc<dyn ContractAuditPort>,
        customers: Arc<dyn CustomerFactsPort>,
        assignments: Arc<dyn CustomerAssignmentFactsPort>,
        accounts: Arc<dyn AccountNamePort>,
        files: Arc<dyn FileAssetFactsPort>,
    ) -> Self {
        Self {
            db,
            audit,
            customers,
            assignments,
            accounts,
            files,
        }
    }

    /// 首次归档合同（合同身份 + 首个不可变版本 + PDF 关联原子形成，数据模型 §6.4）。
    ///
    /// 跨集合事务写入 `contract`、`contract_revision` 与审计日志；客户存在性
    /// 经客户事实 Port 校验；contract_no 唯一性由唯一索引兜底（重复提交映射 409）。
    ///
    /// # 参数
    /// * `req` - 创建请求（含 PDF 文件资产 ID 与版本快照）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建合同的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 客户不存在
    /// * `BusinessLogicError` - 客户已停用
    /// * `ConflictError` - contract_no 与既有合同重复
    pub async fn create_contract(
        &self,
        req: CreateContractRequest,
        actor: &AuditActor,
    ) -> Result<ContractView> {
        req.validate()?;
        self.ensure_active_customer(&req.customer_id).await?;
        let planned = plan_first_archive(req, actor.id())?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract.create",
            "contract",
            planned.contract.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let mut contract_for_tx = planned.contract.clone();
        let revision = planned.revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision, session)
                        .await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        Ok(planned.contract.into())
    }

    /// 按已登记文件资产规划合同首次归档，不开启事务、不写附件。
    ///
    /// 对象存储写入与文件资产登记由 `erp-processes` 在同一 Executor 上完成。
    ///
    /// # 参数
    /// * `req` - 合同业务字段
    /// * `file_asset_id` - 已分配的文件资产 ID
    /// * `actor_id` - 创建人
    ///
    /// # 返回
    /// 返回待写入的合同与首修订。
    ///
    /// # 错误
    /// 客户不存在或停用、字段非法时返回错误。
    pub async fn plan_upload(
        &self,
        req: UploadContractRequest,
        file_asset_id: FileAssetId,
        actor_id: &str,
    ) -> Result<PlannedContractArchive> {
        req.validate()?;
        let customer = self.ensure_active_customer(&req.customer_id).await?;
        let settlement_party_id = req
            .settlement_party_id
            .clone()
            .unwrap_or_else(|| customer.party_id.clone());
        plan_upload_archive(req, file_asset_id, settlement_party_id, actor_id)
    }

    /// 在调用方 Executor 上写入合同身份与不可变修订。
    ///
    /// 组合层持有根事务时必须调用本方法，不得再开事务。
    ///
    /// # 参数
    /// * `contract` - 待写入合同（成功后内存中绑定版本指针）
    /// * `revision` - 不可变修订
    /// * `executor` - 调用方执行器
    ///
    /// # 错误
    /// 唯一索引冲突、乐观锁冲突或底层写入失败。
    pub async fn apply_create_in_transaction(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .contract()
            .create_contract_with_revision(contract, revision, executor)
            .await?;
        Ok(())
    }

    /// 查询合同详情（合同 + 全部不可变版本时间线）。
    ///
    /// # 参数
    /// * `id` - 合同 ID
    ///
    /// # 返回
    /// 返回详情视图（版本按序号倒序）。
    ///
    /// # 错误
    /// * `NotFound` - 合同不存在
    pub async fn contract_detail(&self, id: &str) -> Result<ContractDetailView> {
        let contract = self
            .db
            .contracts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
        let revisions = self
            .db
            .contract_revisions()
            .list_by_contract(&contract.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let view: ContractView = contract.into();
        let owner = self
            .list_customer_facts(std::slice::from_ref(&view.customer_id))
            .await?
            .into_iter()
            .next();
        Ok(ContractDetailView {
            owner_user_id: owner.as_ref().and_then(|c| c.owner_id.clone()),
            owner_user_name: owner.filter(|c| c.owner_id.is_some()).map(|c| c.owner),
            id: view.id,
            contract_no: view.contract_no,
            customer_id: view.customer_id,
            settlement_party_id: view.settlement_party_id,
            status: view.status,
            current_revision_id: view.current_revision_id,
            created_at: view.created_at,
            version: view.version,
            revisions: revisions.into_iter().map(ContractRevisionView::from).collect(),
        })
    }

    /// 归档合同新版本（追加不可变版本并切换当前版本指针，乐观锁语义）。
    ///
    /// 期望版本 `req.version` 与当前版本不一致时直接返回冲突（409）；仓储层
    /// `update` 同时以 `id + version` CAS 兜底并发竞争。
    ///
    /// # 参数
    /// * `id` - 合同 ID
    /// * `req` - 追加版本请求（含期望版本与 PDF 关联）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回追加后的合同详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 合同不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn archive_contract_revision(
        &self,
        id: &str,
        req: ArchiveContractRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        req.validate()?;
        let contract = self
            .db
            .contracts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
        conflict_if_stale_version(contract.matches_version(req.version))?;
        let current_revision_no = self
            .db
            .contract_revisions()
            .latest_revision_no(&contract.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let planned = plan_next_revision(&contract, req, current_revision_no.unwrap_or(0))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract.archive_revision",
            "contract",
            planned.contract.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let mut contract_for_tx = planned.contract.clone();
        let revision = planned.revision;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.contract()
                        .archive_contract_revision(&mut contract_for_tx, &revision, session)
                        .await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        self.contract_detail(id).await
    }

    /// 在调用方 Executor 上追加不可变修订并切换当前版本指针。
    ///
    /// # 参数
    /// * `contract` - 待绑定新版本的合同
    /// * `revision` - 新不可变修订
    /// * `executor` - 调用方执行器
    ///
    /// # 错误
    /// 唯一索引冲突、乐观锁冲突或底层写入失败。
    pub async fn apply_archive_in_transaction(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .contract()
            .archive_contract_revision(contract, revision, executor)
            .await?;
        Ok(())
    }

    /// 终止合同（乐观锁语义；历史销售引用保持不变，W04 授权终止）。
    ///
    /// # 参数
    /// * `id` - 合同 ID
    /// * `req` - 终止请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回终止后的合同详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 合同不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `Logic` - 非 `Effective` 状态不允许终止（实体状态机拒绝）
    pub async fn terminate_contract(
        &self,
        id: &str,
        req: TerminateContractRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        req.validate()?;
        let mut contract = self
            .db
            .contracts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
        conflict_if_stale_version(contract.matches_version(req.version))?;
        contract.terminate(actor.id())?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract.terminate",
            "contract",
            contract.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.contracts().update(&mut contract, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        self.contract_detail(id).await
    }

    /// 在调用方 Executor 上持久化已终止合同。
    ///
    /// # 参数
    /// * `contract` - 已调用 `terminate` 的合同
    /// * `executor` - 调用方执行器
    ///
    /// # 错误
    /// 乐观锁冲突或底层写入失败。
    pub async fn apply_terminate_in_transaction(
        &self,
        contract: &mut Contract,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.contracts().update(contract, executor).await?;
        Ok(())
    }

    /// 确认合同 PDF 附件存在。组合层在登记新文件前用 Port 读取既有资产。
    ///
    /// # 参数
    /// * `file_id` - 文件资产 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回附件事实。
    ///
    /// # 错误
    /// 附件不存在时返回 `NotFound`。
    pub async fn confirm_contract_pdf(
        &self,
        file_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> Result<FileAssetFact> {
        self.files
            .find_by_id(file_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同 PDF 不存在".to_string()))
    }

    /// 校验客户存在且未停用。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    ///
    /// # 返回
    /// 返回客户最小事实。
    ///
    /// # 错误
    /// 客户不存在或已停用。
    pub async fn ensure_active_customer(
        &self,
        customer_id: &CustomerAccountId,
    ) -> Result<CustomerAccountFact> {
        customer_eligibility(self.customers.find_by_id(customer_id).await?)
    }
}

/// 将客户事实映射为归档资格；缺失或停用保持原错误文案。
fn customer_eligibility(customer: Option<CustomerAccountFact>) -> Result<CustomerAccountFact> {
    let customer = customer.ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
    if !customer.is_active {
        return Err(Error::BusinessLogicError(
            "客户已停用，禁止归档新合同".to_string(),
        ));
    }
    Ok(customer)
}

/// 由首次归档请求构造合同身份与首个不可变修订。
///
/// # 参数
/// * `req` - 已通过 `Validate` 的创建请求
/// * `actor_id` - 创建人
///
/// # 返回
/// 返回尚未持久化的合同与修订。
///
/// # 错误
/// 编号/快照为空、超长或有效期倒挂。
pub fn plan_first_archive(
    req: CreateContractRequest,
    actor_id: impl Into<String>,
) -> Result<PlannedContractArchive> {
    let actor_id = actor_id.into();
    let contract = Contract::new(
        ContractId::new(next_id()),
        ContractData {
            contract_no: req.contract_no,
            customer_id: req.customer_id,
            settlement_party_id: req.settlement_party_id,
        },
        actor_id,
    )?;
    let revision = ContractRevision::new(
        ContractRevisionId::new(next_id()),
        contract.base.id.clone().into(),
        1,
        ContractRevisionData {
            contract_no: contract.contract_no.clone(),
            customer_name: req.customer_name,
            contract_pdf_file_id: req.contract_pdf_file_id,
            archive_source: req.archive_source.unwrap_or(ArchiveSource::ContractCenter),
            settlement_party_id: contract.settlement_party_id.clone(),
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        },
    )?;
    Ok(PlannedContractArchive { contract, revision })
}

/// 由一次上传命令构造合同身份与首个不可变修订。
///
/// # 参数
/// * `req` - 已通过 `Validate` 的上传命令
/// * `file_asset_id` - 已分配的文件资产 ID
/// * `settlement_party_id` - 结算主体；调用方已按客户缺省补齐
/// * `actor_id` - 创建人
///
/// # 返回
/// 返回尚未持久化的合同与修订。
///
/// # 错误
/// 编号/快照为空、超长或有效期倒挂。
pub fn plan_upload_archive(
    req: UploadContractRequest,
    file_asset_id: FileAssetId,
    settlement_party_id: PartyId,
    actor_id: impl Into<String>,
) -> Result<PlannedContractArchive> {
    let actor_id = actor_id.into();
    let contract = Contract::new(
        ContractId::new(next_id()),
        ContractData {
            contract_no: req.contract_no,
            customer_id: req.customer_id,
            settlement_party_id: settlement_party_id.clone(),
        },
        actor_id,
    )?;
    let revision = ContractRevision::new(
        ContractRevisionId::new(next_id()),
        contract.base.id.clone().into(),
        1,
        ContractRevisionData {
            contract_no: contract.contract_no.clone(),
            customer_name: req.customer_name,
            contract_pdf_file_id: file_asset_id,
            archive_source: ArchiveSource::ContractCenter,
            settlement_party_id,
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        },
    )?;
    Ok(PlannedContractArchive { contract, revision })
}

/// 由追加版本请求构造下一不可变修订。
fn plan_next_revision(
    contract: &Contract,
    req: ArchiveContractRevisionRequest,
    current_revision_no: u32,
) -> Result<PlannedContractArchive> {
    let next_no = ContractRevision::next_revision_no(current_revision_no)?;
    let revision = ContractRevision::new(
        ContractRevisionId::new(next_id()),
        contract.base.id.clone().into(),
        next_no,
        ContractRevisionData {
            contract_no: contract.contract_no.clone(),
            customer_name: req.customer_name,
            contract_pdf_file_id: req.contract_pdf_file_id,
            archive_source: req.archive_source.unwrap_or(ArchiveSource::ContractCenter),
            settlement_party_id: contract.settlement_party_id.clone(),
            settlement_party_name: req.settlement_party_name,
            payment_term_code: req.payment_term_code,
            payment_term_name: req.payment_term_name,
            invoice_type: req.invoice_type,
            tax_point: req.tax_point,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            signed_at: req.signed_at,
        },
    )?;
    Ok(PlannedContractArchive {
        contract: contract.clone(),
        revision,
    })
}

/// 将实体版本匹配结果映射为稳定 409 语义。
///
/// # 参数
/// * `matched` - `Contract::matches_version(expected)` 的结果
///
/// # 返回
/// 匹配时返回 `Ok(())`。
///
/// # 错误
/// 不匹配时返回 `ConflictError`（HTTP 409）。
///
/// # 约束
/// 不比较版本号；调用方必须先调用实体 `matches_version`。
fn conflict_if_stale_version(matched: bool) -> Result<()> {
    if matched {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

#[cfg(test)]
mod version_lock_tests {
    use super::{conflict_if_stale_version, plan_first_archive, plan_upload_archive};
    use crate::dto::contract::{CreateContractRequest, UploadContractRequest};
    use crate::entity::contract::{ArchiveSource, Contract, ContractData, ContractId};
    use crate::error::Error;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{CustomerAccountId, FileAssetId, PartyId};
    use serde_json::json;

    /// 归档与终止必须使用实体 matches_version，禁止字段级直接比较。
    #[test]
    fn archive_and_terminate_use_entity_matches_version() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("conflict_if_stale_version(contract.matches_version(req.version))"));
        assert!(!production.contains("contract.base.version != req.version"));
        assert!(!production.contains("contract.base.version == req.version"));
    }

    #[test]
    fn stale_version_returns_conflict_with_stable_retry_message() {
        let contract = Contract::new(
            ContractId::new("c-1"),
            ContractData {
                contract_no: "HT-1".into(),
                customer_id: CustomerAccountId::new("cust-1"),
                settlement_party_id: PartyId::new("party-1"),
            },
            "admin-1",
        )
        .expect("合同必须可构造");
        assert!(conflict_if_stale_version(contract.matches_version(contract.base.version)).is_ok());
        match conflict_if_stale_version(contract.matches_version(0)) {
            Err(Error::ConflictError(message)) => {
                assert_eq!(message, "数据已被其他请求修改，请刷新后重试");
            }
            other => panic!("必须映射为 ConflictError，得到 {other:?}"),
        }
    }

    fn archive_json() -> serde_json::Value {
        json!({
            "contract_no": "HT-2026-0088",
            "customer_id": "cust-1",
            "settlement_party_id": "party-1",
            "contract_pdf_file_id": "file-1",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "valid_to": "2026-12-31",
            "signed_at": "2025-12-20",
        })
    }

    #[test]
    fn plan_first_archive_keeps_request_snapshots_and_pdf_id() {
        let req: CreateContractRequest = serde_json::from_value(archive_json()).unwrap();
        let planned = plan_first_archive(req, "admin-1").unwrap();
        assert_eq!(planned.contract.contract_no, "HT-2026-0088");
        assert_eq!(planned.revision.revision.revision_no, 1);
        assert_eq!(planned.revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(
            planned.revision.settlement_party_snapshot.settlement_party_name,
            "集团结算中心"
        );
        assert_eq!(planned.revision.contract_pdf_file_id, FileAssetId::new("file-1"));
        assert_eq!(planned.revision.archive_source, ArchiveSource::ContractCenter);
        assert_eq!(
            planned.revision.valid_from,
            BusinessDate::from_ymd(2026, 1, 1).unwrap()
        );
    }

    #[test]
    fn inactive_or_missing_customer_keeps_original_archive_errors() {
        use super::customer_eligibility;
        use crate::ports::CustomerAccountFact;

        match customer_eligibility(None) {
            Err(Error::NotFound(message)) => assert_eq!(message, "客户不存在"),
            other => panic!("期望 NotFound，得到 {other:?}"),
        }
        let disabled = CustomerAccountFact {
            id: "cust-1".to_string(),
            customer_no: "C-1".to_string(),
            party_id: PartyId::new("party-1"),
            is_active: false,
        };
        match customer_eligibility(Some(disabled)) {
            Err(Error::BusinessLogicError(message)) => {
                assert_eq!(message, "客户已停用，禁止归档新合同");
            }
            other => panic!("期望 BusinessLogicError，得到 {other:?}"),
        }
    }

    #[test]
    fn plan_upload_archive_binds_assigned_file_id_and_default_source() {
        let req: UploadContractRequest = serde_json::from_value(json!({
            "contract_no": "HT-2026-0099",
            "customer_id": "cust-1",
            "customer_name": "东方企业",
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "valid_from": "2026-01-01",
            "signed_at": "2025-12-20",
        }))
        .unwrap();
        let planned = plan_upload_archive(
            req,
            FileAssetId::new("asset-9"),
            PartyId::new("party-from-customer"),
            "admin-1",
        )
        .unwrap();
        assert_eq!(
            planned.contract.settlement_party_id,
            PartyId::new("party-from-customer")
        );
        assert_eq!(planned.revision.contract_pdf_file_id, FileAssetId::new("asset-9"));
        assert_eq!(planned.revision.archive_source, ArchiveSource::ContractCenter);
        assert_eq!(planned.revision.revision.revision_no, 1);
    }
}
