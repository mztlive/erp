//! 域 D12 `contract` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 合同首次归档：跨集合（contract + contract_revision + 审计）→
//!   `persistence_core::Transactional::with_transaction`（仓储方法
//!   `create_contract_with_revision` 声明「必须收到事务执行器」）；
//! - 追加版本 / 终止：跨集合（版本 + 主表 CAS + 审计）→ 同一事务模板；
//! - 列表 / 详情：授权与业务读取使用同一事务快照，交付前重新核验。
//!
//! 客户、归属与附件事实走消费方 Port；账号目录使用 identity 公开 API。
//! 文件资产写入与跨域根事务由 `erp-processes` 持有，本模块提供事务内接口。

use std::slice;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::{CustomerAccountId, FileAssetId};
use erp_identity::AccountDirectoryService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use crate::authorization::CompiledScope;
pub use crate::authorization::ContractAccess;
use crate::dto::contract::{
    ContractDetailView, ContractRevisionView, ContractView, CreateContractRequest, UploadContractRequest,
};
use crate::entity::contract::{Contract, ContractRevision};
use crate::error::{Error, Result};
use crate::ports::{
    ContractAuditPort, CustomerAccountFact, CustomerFactsPort, PreparedContractAudit, SigningCompanyPort,
};
use crate::repository::ContractExt;
use crate::repository::prelude::*;

mod archive;
mod commands;
mod company;
mod detail;
mod fingerprint;
mod query;
mod sales_access;
mod scope;

pub use archive::{PlannedContractArchive, plan_first_archive, plan_upload_archive};
pub(crate) use archive::{conflict_if_stale_version, customer_eligibility, plan_next_revision};
use company::{require_signing_company, required_settlement};

/// 合同服务。
///
/// 提供合同归档、版本追加、终止与查询编排。
#[derive(Clone)]
pub struct ContractService {
    db: Database,
    audit: Arc<dyn ContractAuditPort>,
    customers: Arc<dyn CustomerFactsPort>,
    accounts: AccountDirectoryService,
    companies: Arc<dyn SigningCompanyPort>,
    data_scope: ContractAccess,
}

impl ContractService {
    /// 创建合同服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `customers` - 客户存在性与编号事实
    /// * `companies` - 我方公司身份及启停事实
    /// * `authorization` - 组合层装配固定责任模型和真实客户来源的授权能力
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 授权使用组合根的固定责任模型；未接线时拒绝访问。
    pub fn new(
        db: Database,
        audit: Arc<dyn ContractAuditPort>,
        customers: Arc<dyn CustomerFactsPort>,
        companies: Arc<dyn SigningCompanyPort>,
        authorization: ContractAccess,
    ) -> Self {
        Self {
            accounts: AccountDirectoryService::new(db.clone()),
            db,
            audit,
            customers,
            companies,
            data_scope: authorization,
        }
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
        let access = self.data_scope.clone();
        let service = self.clone();
        let actor_for_tx = actor.clone();
        let mut contract_for_tx = planned.contract;
        let revision = planned.revision;
        execute_authorized_transaction(
            &db,
            &audit_port,
            &audit,
            customer_id,
            access,
            actor_for_tx,
            Box::new(move |_, executor| {
                Box::pin(async move {
                    service.apply_create(&mut contract_for_tx, &revision, executor).await?;
                    Ok(contract_for_tx.into())
                })
            }),
        )
        .await
    }

    /// 按已登记文件资产规划合同首次归档，不读取数据库、不写附件。
    ///
    /// 对象存储写入与文件资产登记由所属编排入口组织。
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
    /// 字段非法时返回错误；客户及公司资格在原写事务由 apply_create 核验。
    pub async fn plan_upload(
        &self,
        req: UploadContractRequest,
        file_asset_id: FileAssetId,
        actor_id: &str,
    ) -> Result<PlannedContractArchive> {
        req.validate()?;
        let settlement_party_id = required_settlement(req.settlement_party_id.as_ref())?;
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
    /// # 返回
    /// 无返回值。合同身份与不可变修订已写入调用方执行器。
    ///
    /// # 错误
    /// 唯一索引冲突、乐观锁冲突或底层写入失败。
    pub async fn apply_create(
        &self,
        contract: &mut Contract,
        revision: &ContractRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_active_customer(&contract.customer_id, executor).await?;
        require_signing_company(self.companies.as_ref(), &contract.settlement_party_id, executor).await?;
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
    /// 详情经固定授权条件读取，完整补齐后检查原凭证；历史参与只补充读取。
    pub async fn contract_detail(&self, id: &str, actor: &AuditActor) -> Result<ContractDetailView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { detail::read(&service, &id, &actor, executor).await })
            })
            .await
    }

    /// 装载详情展示字段，不解释数据范围。
    ///
    /// # 参数
    /// * `id` - 合同 ID
    /// * `scope` - 当前读取阶段的完整合同详情查询凭证
    /// * `executor` - 原授权读取事务
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// 合同不存在时返回 NotFound。
    ///
    /// # 关键业务约束
    /// 本方法不解释权限；HTTP 详情入口必须先调用 `contract_detail`。
    async fn load_contract_detail(
        &self,
        id: &str,
        scope: &CompiledScope,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let contract = self
            .db
            .contracts()
            .find_authorized(id, scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在或无权查看".to_string()))?;
        self.detail_view(contract, executor).await
    }

    /// 从已授权的原对象或命令结果补齐修订与姓名，不增加详情菜单资格。
    async fn detail_view(
        &self,
        contract: Contract,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let revisions =
            self.db.contract_revisions().list_by_contract(&contract.base.id.clone().into(), executor).await?;
        let shared_sales_user_ids = contract.shared_sales_user_ids.clone();
        let view: ContractView = contract.into();
        let names = match &view.owner_user_id {
            Some(id) => self.accounts.names(std::slice::from_ref(id), executor).await?,
            None => Default::default(),
        };
        Ok(ContractDetailView {
            shared_sales_user_ids,
            owner_user_name: view.owner_user_id.as_ref().and_then(|id| names.get(id).cloned()),
            owner_user_id: view.owner_user_id,
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
        self.data_scope.clone().require_create(actor, customer_id, executor).await?;
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
        let detail = self.contract_detail(contract_id, actor).await?;
        if !detail.revisions.iter().any(|revision| revision.contract_pdf_file_id == file_id) {
            return Err(Error::NotFound("合同附件不存在或无权查看".into()));
        }
        Ok(())
    }

    /// 校验客户存在且未停用。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `executor` - 原授权写事务的执行器。
    ///
    /// # 返回
    /// 返回客户最小事实。
    ///
    /// # 错误
    /// 客户不存在或已停用。
    pub async fn ensure_active_customer(
        &self,
        customer_id: &CustomerAccountId,
        executor: &mut dyn Executor,
    ) -> Result<CustomerAccountFact> {
        let facts = self.customers.find_by_ids(slice::from_ref(customer_id), executor).await?;
        customer_eligibility(facts.into_iter().find(|fact| fact.id == customer_id.as_ref()))
    }
}

/// 本命令的仓储写入闭包：事务内执行，失败整事务回滚。
type TxAction<T> = Box<
    dyn for<'a> FnOnce(
            &'a Database,
            &'a mut dyn Executor,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
        + Send,
>;

/// 首次归档的授权事务和审计编排。
///
/// 原事务先核验真实客户候选，再执行合同写入和审计；任一步失败整事务回滚。
///
/// # 参数
/// * `db` - 合同数据库
/// * `audit_port` - 审计写入端口
/// * `audit` - 已规划的审计记录
/// * `customer_id` - 真实客户来源身份
/// * `access` - 合同范围访问器
/// * `actor` - 已通过鉴权的审计操作人
/// * `action` - 本命令的仓储写入闭包
///
/// # 返回
/// 事务提交成功时返回本命令在原事务生成的响应。
///
/// # 错误
/// 授权拒绝、仓储写入或审计落盘失败时返回原错误。
async fn execute_authorized_transaction<T: Send + 'static>(
    db: &Database,
    audit_port: &Arc<dyn ContractAuditPort>,
    audit: &PreparedContractAudit,
    customer_id: String,
    access: ContractAccess,
    actor: AuditActor,
    action: TxAction<T>,
) -> Result<T> {
    let client = db.client().clone();
    let db = db.clone();
    client
        .with_transaction(move |executor| {
            let audit_port = audit_port.clone();
            let audit = audit.clone();
            Box::pin(async move {
                access.require_create(&actor, &customer_id, executor).await?;
                let result = action(&db, executor).await?;
                audit_port.persist(&audit, executor).await?;
                Ok::<T, crate::error::Error>(result)
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
