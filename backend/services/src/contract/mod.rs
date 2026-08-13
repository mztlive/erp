//! 域 D12 `contract` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 合同首次归档：跨集合（contract + contract_revision + 审计）→
//!   `database::Transactional::with_transaction`（仓储方法
//!   `create_contract_with_revision` 声明「必须收到事务执行器」）；
//! - 追加版本 / 终止：跨集合（版本 + 主表 CAS + 审计）→ 同一事务模板；
//! - 列表 / 详情：单集合无跨步骤原子性要求 → `&mut NoTransaction`。
//!
//! 跨域读取（客户存在性）走 D08 的 `customer_accounts` Repository；
//! 审计写入复用 `audit::AuditActor::resource_log` + `AccessControlExt::audit_logs`，
//! 与既有 `source_registry` 模板一致。

use database::{AccessControlExt, ContractExt, CustomerExt, NoTransaction, Transactional};
use entities::contract::{
    ArchiveSource, Contract, ContractData, ContractId, ContractRevision, ContractRevisionData,
    ContractRevisionId,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;
mod query;
mod scope;

pub use self::dto::{
    ArchiveContractRevisionRequest, ContractDetailView, ContractListParams, ContractListScope,
    ContractRevisionView, ContractView, CreateContractRequest, PageView, TerminateContractRequest,
};

/// 合同服务。
///
/// 提供合同归档、版本追加、终止与查询编排。
pub struct ContractService {
    db: Database,
}

impl ContractService {
    /// 创建合同服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 首次归档合同（合同身份 + 首个不可变版本 + PDF 关联原子形成，数据模型 §6.4）。
    ///
    /// 跨集合事务写入 `contract`、`contract_revision` 与审计日志；客户存在性
    /// 经 D08 的 `customer_accounts` Repository 校验；contract_no 唯一性由唯一索引
    /// 兜底（重复提交映射 409）。
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
        let customer = self
            .db
            .customer_accounts()
            .find_by_id(&req.customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError(
                "客户已停用，禁止归档新合同".to_string(),
            ));
        }

        let contract = Contract::new(
            ContractId::new(next_id()),
            ContractData {
                contract_no: req.contract_no,
                customer_id: req.customer_id,
                settlement_party_id: req.settlement_party_id,
            },
            actor.id(),
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
        let audit = actor
            .clone()
            .resource_log("contract.create", "contract", contract.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut contract_for_tx = contract.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(contract.into())
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
        Ok(ContractDetailView {
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
        if contract.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let existing = self
            .db
            .contract_revisions()
            .list_by_contract(&contract.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let next_no = existing
            .first()
            .map(|revision| revision.revision.revision_no + 1)
            .unwrap_or(1);
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
        let audit =
            actor
                .clone()
                .resource_log("contract.archive_revision", "contract", contract.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut contract_for_tx = contract.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.contract()
                        .archive_contract_revision(&mut contract_for_tx, &revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.contract_detail(id).await
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
        if contract.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        contract.terminate(actor.id())?;
        let audit = actor
            .clone()
            .resource_log("contract.terminate", "contract", contract.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.contracts().update(&mut contract, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.contract_detail(id).await
    }
}
