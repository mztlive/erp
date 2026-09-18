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

use application_core::AuditActor;
use erp_core::ids::{CustomerAccountId, FileAssetId, PartyId};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use crate::dto::contract::{
    ArchiveContractRevisionRequest, ContractDetailView, ContractRevisionView, ContractView,
    CreateContractRequest, TerminateContractRequest, UploadContractRequest,
};
use crate::entity::contract::{Contract, ContractRevision};
use crate::error::{Error, Result};
use crate::ports::{
    AccountNamePort, ContractAuditPort, ContractDataScopePort, ContractParticipantPort, CustomerAccountFact,
    CustomerAssignmentFactsPort, CustomerFactsPort, FileAssetFact, FileAssetFactsPort, PreparedContractAudit,
};
use crate::repository::ContractExt;

pub mod access;
mod archive;
mod query;
mod scope;

pub use access::ContractAccess;
pub use archive::{PlannedContractArchive, plan_first_archive, plan_upload_archive};
pub(crate) use archive::{conflict_if_stale_version, customer_eligibility, plan_next_revision};

/// 合同范围解析及合法参与事实必须成组装配；缺少任一项均不能形成完整授权。
pub struct ContractScopePorts {
    /// 公共范围解析及单对象判定端口。
    pub data_scope: Arc<dyn ContractDataScopePort>,
    /// 本域合法单据参与事实端口。
    pub participants: Arc<dyn ContractParticipantPort>,
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
    data_scope: Arc<dyn ContractDataScopePort>,
    participants: Arc<dyn ContractParticipantPort>,
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
    /// * `scope_ports` - 成组装配的公共范围与合法参与端口
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 范围解析必须走注入的 Port；未接线端口失败关闭，不得补公司范围。
    pub fn new(
        db: Database,
        audit: Arc<dyn ContractAuditPort>,
        customers: Arc<dyn CustomerFactsPort>,
        assignments: Arc<dyn CustomerAssignmentFactsPort>,
        accounts: Arc<dyn AccountNamePort>,
        files: Arc<dyn FileAssetFactsPort>,
        scope_ports: ContractScopePorts,
    ) -> Self {
        Self {
            db,
            audit,
            customers,
            assignments,
            accounts,
            files,
            data_scope: scope_ports.data_scope,
            participants: scope_ports.participants,
        }
    }

    /// 构造复用本服务授权 Port 的合同访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库与范围 Port 的访问器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在此回退构造身份域 Service。
    fn access(&self) -> access::ContractAccess {
        access::ContractAccess::new(
            self.db.clone(),
            Arc::clone(&self.data_scope),
            Arc::clone(&self.assignments),
            Arc::clone(&self.participants),
        )
    }

    /// 首次归档合同（合同身份 + 首个不可变版本 + PDF 关联原子形成，数据模型 §6.4）。
    ///
    /// 跨集合事务写入 `contract`、`contract_revision` 与审计日志；客户存在性
    /// 经客户事实 Port 校验；contract_no 唯一性由唯一索引兜底（重复提交映射 409）。
    ///
    /// 编排经 [`execute_authorized_transaction`] 统一接入授权事务执行、
    /// 审计落盘与详情回读；动作闭包只声明本命令的仓储写入。
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
        let customer_id = req.customer_id.to_string();
        let planned = plan_first_archive(req, actor.id())?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract.create",
            "contract",
            planned.contract.base.id.clone(),
        )?;

        let db = self.db.clone();
        let audit_port = self.audit.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        let mut contract_for_tx = planned.contract.clone();
        let revision = planned.revision.clone();
        execute_authorized_transaction(
            &db,
            &audit_port,
            &audit,
            TxAuthorization::Create(customer_id),
            access,
            actor_for_tx,
            Box::new(move |db, session| {
                Box::pin(async move {
                    db.contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision, session)
                        .await?;
                    Ok::<(), crate::error::Error>(())
                })
            }),
        )
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
        let settlement_party_id =
            req.settlement_party_id.clone().unwrap_or_else(|| customer.party_id.clone());
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
        self.db.contract().create_contract_with_revision(contract, revision, executor).await?;
        Ok(())
    }

    /// 查询合同详情（合同 + 全部不可变版本时间线）。
    ///
    /// # 参数
    /// * `id` - 合同 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回详情视图（版本按序号倒序）。
    ///
    /// # 错误
    /// * `NotFound` - 合同不存在或不在读取范围内
    /// * `ConflictError` - 查询过程中范围变化
    ///
    /// # 关键业务约束
    /// 详情必须独立重验当前权限；历史参与只补充读取。
    pub async fn contract_detail(&self, id: &str, actor: &AuditActor) -> Result<ContractDetailView> {
        let expected = self.access().require(actor, "detail", id).await?;
        let view = self.load_contract_detail(id).await?;
        let current = self.access().require(actor, "detail", id).await?;
        scope::ensure_stable_snapshot(&expected.scope_version, &current.scope_version)?;
        Ok(view)
    }

    /// 装载详情展示字段，不解释数据范围。
    ///
    /// # 参数
    /// * `id` - 合同 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// 合同不存在时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 本方法不解释权限；HTTP 详情入口必须先调用 `contract_detail`。
    pub async fn load_contract_detail(&self, id: &str) -> Result<ContractDetailView> {
        let contract = self
            .db
            .contracts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在或无权查看".to_string()))?;
        let revisions = self
            .db
            .contract_revisions()
            .list_by_contract(&contract.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let view: ContractView = contract.into();
        let owner =
            self.list_customer_facts(std::slice::from_ref(&view.customer_id)).await?.into_iter().next();
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
    /// 编排经 [`execute_authorized_transaction`]；范围重验仍在事务内先执行。
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
        let audit_port = self.audit.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        let contract_id = id.to_string();
        let mut contract_for_tx = planned.contract.clone();
        let revision = planned.revision;
        execute_authorized_transaction(
            &db,
            &audit_port,
            &audit,
            TxAuthorization::Use(contract_id),
            access,
            actor_for_tx,
            Box::new(move |db, session| {
                Box::pin(async move {
                    db.contract().archive_contract_revision(&mut contract_for_tx, &revision, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            }),
        )
        .await?;

        self.contract_detail(id, actor).await
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
        self.db.contract().archive_contract_revision(contract, revision, executor).await?;
        Ok(())
    }

    /// 终止合同（乐观锁语义；历史销售引用保持不变，W04 授权终止）。
    ///
    /// 编排经 [`execute_authorized_transaction`]；终止状态机仍在事务外先执行。
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
        let audit_port = self.audit.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        let contract_id = id.to_string();
        execute_authorized_transaction(
            &db,
            &audit_port,
            &audit,
            TxAuthorization::Use(contract_id),
            access,
            actor_for_tx,
            Box::new(move |db, session| {
                Box::pin(async move {
                    db.contracts().update(&mut contract, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            }),
        )
        .await?;

        self.contract_detail(id, actor).await
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

    /// 在调用方事务内证明按指定客户创建合同的资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `customer_id` - 拟归档合同的客户
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建范围覆盖该客户时成功。
    ///
    /// # 错误
    /// 无创建动作或客户不在授权内时拒绝。
    ///
    /// # 关键业务约束
    /// 组合层持有根事务时必须调用本方法，不得只依赖入口事前检查。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.access().require_create(actor, customer_id, executor).await?;
        Ok(())
    }

    /// 独立重验合同读取资格，并确认附件属于该合同。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `contract_id` - 合同 ID
    /// * `file_id` - 合同 PDF 文件资产 ID
    ///
    /// # 返回
    /// 附件属于该合同且当前可读时成功。
    ///
    /// # 错误
    /// 合同不可读或附件不属于该合同时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 列表或详情已授权不能作为附件请求的长期凭证。
    pub async fn require_attachment(
        &self,
        actor: &AuditActor,
        contract_id: &str,
        file_id: &str,
    ) -> Result<()> {
        self.access().require(actor, "detail", contract_id).await?;
        let detail = self.load_contract_detail(contract_id).await?;
        if !detail.revisions.iter().any(|revision| revision.contract_pdf_file_id == file_id) {
            return Err(Error::NotFound("合同附件不存在或无权查看".into()));
        }
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

/// 写命令事务内的授权目标：新建客户或既有合同。
enum TxAuthorization {
    /// 新建归档：证明按指定客户创建的资格。
    Create(String),
    /// 既有合同：重验 `update` 对象资格。
    Use(String),
}

/// 本命令的仓储写入闭包：事务内执行，失败整事务回滚。
type TxAction = Box<
    dyn for<'a> FnOnce(
            &'a Database,
            &'a mut mongodb::ClientSession,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send,
>;

/// 授权事务执行 + 审计落盘的统一编排（三写命令共用）。
///
/// 范围重验始终在事务内先执行（`Create` 走 `require_create`，
/// `Use` 走 `require_with(..., "update", ...)`），再执行动作闭包的仓储写入，
/// 最后落盘审计；任一步失败整事务回滚。
///
/// # 参数
/// * `db` - 合同数据库
/// * `audit_port` - 审计写入端口
/// * `audit` - 已规划的审计记录
/// * `authorization` - 事务内授权目标
/// * `access` - 合同范围访问器
/// * `actor` - 已通过鉴权的审计操作人
/// * `action` - 本命令的仓储写入闭包
///
/// # 返回
/// 事务提交成功时返回 `Ok(())`。
///
/// # 错误
/// 授权拒绝、仓储写入或审计落盘失败时返回原错误。
async fn execute_authorized_transaction(
    db: &Database,
    audit_port: &Arc<dyn ContractAuditPort>,
    audit: &PreparedContractAudit,
    authorization: TxAuthorization,
    access: access::ContractAccess,
    actor: AuditActor,
    action: TxAction,
) -> Result<()> {
    let client = db.client().clone();
    let db = db.clone();
    client
        .with_transaction(move |session| {
            let db = db.clone();
            let audit_port = audit_port.clone();
            let audit = audit.clone();
            let access = access.clone();
            let actor = actor.clone();
            let action = action;
            Box::pin(async move {
                match authorization {
                    TxAuthorization::Create(customer_id) => {
                        access.require_create(&actor, &customer_id, session).await?;
                    },
                    TxAuthorization::Use(contract_id) => {
                        access.require_with(actor, "update", &contract_id, session).await?;
                    },
                }
                action(&db, session).await?;
                audit_port.persist(&audit, session).await?;
                Ok::<(), crate::error::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod version_lock_tests {
    use erp_core::ids::{CustomerAccountId, PartyId};

    use super::conflict_if_stale_version;
    use crate::entity::contract::{Contract, ContractData, ContractId};
    use crate::error::Error;

    /// 归档与终止必须使用实体 matches_version，禁止字段级直接比较。
    #[test]
    fn archive_and_terminate_use_entity_matches_version() {
        let production = include_str!("mod.rs").split("#[cfg(test)]").next().expect("生产代码");
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
            },
            other => panic!("必须映射为 ConflictError，得到 {other:?}"),
        }
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
            },
            other => panic!("期望 BusinessLogicError，得到 {other:?}"),
        }
    }
}
