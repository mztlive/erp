//! 销售命令在原业务事务内重验资源动作、责任和业务版本。

use std::sync::Arc;

use erp_permissions::{Permission as AccessPermission, contract, sales_order as sales_order_permissions};

use crate::services::SalesServices;
mod settlement;

use application_core::AuditActor;
use erp_contract::{Contract, ContractExt};
use erp_customer::{CustomerAccount, CustomerExt};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{customer, internal};
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::entity::sales_order::contract_terms::ensure_contract_customer;
use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;

use super::SalesOrderCommandProcess;
use crate::{Error, Result};

/// 命令上下文保存请求身份与业务意图；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct SalesCommandAccess {
    pub(super) services: Arc<SalesServices>,
    db: mongodb::Database,
    authorization: Arc<AuthorizationService>,
    access: SalesAccess,
    actor: AuditActor,
    permission: AccessPermission,
    submit: bool,
    customer_selection: bool,
}

impl SalesOrderCommandProcess {
    // 构造在原事务执行对象授权的写命令检查器。
    //
    // `permission` 为本次销售写动作。返回可在原事务重验销售单及关联合同／客户的检查器。
    //
    // 关联合同／客户必须另走各自资源动作，不能复用本销售动作码。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        permission: AccessPermission,
    ) -> SalesCommandAccess {
        let customer_selection =
            permission == sales_order_permissions::CREATE || permission == sales_order_permissions::SUBMIT;
        SalesCommandAccess {
            services: self.services.clone(),
            db: self.db.clone(),
            authorization: self.authorization.clone(),
            access: SalesAccess::new(self.db.clone(), self.authorization.clone()),
            actor: actor.clone(),
            permission,
            submit: false,
            customer_selection,
        }
    }
}

impl SalesCommandAccess {
    // 建单即提交时标记还需 `sales_order:submit`，后续校验要求同一角色同时授予创建和提交；只保存草稿不追加该权限。
    pub(super) fn require_submit(mut self, submit: bool) -> Self {
        self.submit = submit;
        self
    }

    // 将本次客户关系变更标记为开展新业务。
    //
    // 调用方须先比较当前单据与本次选择的客户或合同。后续客户检查只接受当前选择资格的命令检查器。
    pub(super) fn selecting_customer(mut self) -> Self {
        self.customer_selection = true;
        self
    }

    // 在调用方事务内读取当前可操作单据，并复用公共单对象判定。返回当前可操作的销售单。单据不可见或缺失时返回错误；已要求提交时，缺少创建权限同样拒绝。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesOrder> {
        let order = if self.permission == sales_order_permissions::CREATE {
            let permission = if self.submit {
                &internal::SALES_CREATE_AND_SUBMIT
            } else {
                &internal::SALES_CREATED_RESULT_READ
            };
            self.authorization.require(&self.actor, permission, AccessTarget::record(id)?, executor).await?;
            self.db
                .sales_orders()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("销售单不存在或无权操作".into()))?
        } else {
            self.access.require_object(&self.actor, &self.permission, id, executor).await?
        };

        if self.submit {
            self.access.require_object(&self.actor, &sales_order_permissions::SUBMIT, id, executor).await?;
        }
        Ok(order)
    }

    // 写入前重新读取当前责任与版本，防止预读取后交接或状态变化。
    //
    // `expected` 为预读时的单据版本；沿用原事务的执行器。当前版本仍为 `expected` 时返回。单据不可操作时返回对应错误；版本已变化时返回冲突。
    pub(super) async fn revalidate(
        &self,
        id: &str,
        expected: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.current(id, executor).await?;
        if current.base.version != expected {
            return Err(Error::ConflictError("销售单责任或版本已变化，请刷新后重试".into()));
        }
        Ok(())
    }

    // 新单使用即将持久化的显式责任解释创建范围，不能由创建人审计字段兜底。
    //
    // `order` 为即将持久化、带显式责任的销售单；沿用原事务的执行器。创建范围允许该责任时返回；已要求提交时，提交范围也必须允许。
    //
    // 责任不在动作范围内，或权限码无法解析时返回错误。
    pub(super) async fn creation(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        self.services
            .customer_access
            .require_current(&self.actor, order.customer_id.as_ref(), executor)
            .await?;

        let permission = if self.submit { &internal::SALES_CREATE_AND_SUBMIT } else { &self.permission };
        self.access.require_loaded(&self.actor, permission, order, executor).await?;
        if self.submit {
            self.access
                .require_loaded(&self.actor, &sales_order_permissions::SUBMIT, order, executor)
                .await?;
        }
        Ok(())
    }

    // 在调用方事务内按合同 detail 动作证明所选合同可见。
    //
    // `contract_id` 为本次命令选择的合同；`executor` 为调用方执行器，必须是原写入事务或同一授权读取。合同在范围内时成功。
    //
    // 不可见或缺失时返回 NotFound；缺动作时拒绝。
    //
    // handler 事前检查不能代替；不得用无范围 `find_by_id` 作为授权读取。
    async fn require_contract(&self, contract_id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.services
            .contracts
            .require_with(self.actor.clone(), &contract::DETAIL, contract_id, executor)
            .await?;
        Ok(())
    }

    // 在调用方事务内检查本次命令需要的客户资格。
    //
    // `customer_id` 为合同所属客户；`executor` 为调用方执行器，必须是原写入事务或同一授权读取。
    //
    // 新业务使用当前选择资格；关系未变的已有单据继续使用详情资格。客户缺失、不满足本次范围或缺少功能资格时拒绝。
    //
    // 不得用销售范围或合同范围代替客户对象判定。
    async fn require_customer(&self, customer_id: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.customer_selection {
            self.services.customer_access.require_current(&self.actor, customer_id, executor).await?;
        } else {
            self.services
                .customer_access
                .require_with(self.actor.clone(), &customer::DETAIL, customer_id, executor)
                .await?;
        }
        Ok(())
    }

    // 首次补录合同时分别重验合同可读资格与客户当前选择资格。
    //
    // `contract_id` 为本次命令选择的合同；`customer_id` 为合同所属客户；`executor` 为原写入事务执行器。
    //
    // 合同可读且客户具备开展新业务的当前资格时成功。合同不可读，或客户不具备当前选择资格时拒绝。
    //
    // 创建／保存／提交必须在原写入事务调用；独立 HTTP 事前检查不是凭证。
    pub(super) async fn related(
        &self,
        contract_id: &str,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_contract(contract_id, executor).await?;
        self.services.customer_access.require_current(&self.actor, customer_id, executor).await?;
        Ok(())
    }

    // 按本次销售命令的合同与客户重验对应业务资格。
    //
    // `order` 为即将写入的销售单，带明确客户及可选合同；`executor` 为原写入事务执行器。合同可读且客户满足当前命令需要的资格时成功。
    //
    // 客户不满足本次范围，或已选合同不可读时拒绝。
    //
    // 客户始终独立重验；只有已选择合同时才重验合同对象。
    pub(super) async fn related_order(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        if let Some(contract_id) = order.contract_id.as_ref() {
            let contract = self.load_contract(contract_id.as_ref(), executor).await?;
            ensure_contract_customer(&contract.customer_id, Some(&order.customer_id))?;
            self.require_customer(order.customer_id.as_ref(), executor).await?;
            self.settlement_name(&order.settlement_party_id, executor).await?;
            Ok(())
        } else {
            self.load_customer(order.customer_id.as_ref(), executor).await?;
            self.settlement_name(&order.settlement_party_id, executor).await?;
            Ok(())
        }
    }

    // 在调用方执行器内按合同 detail 动作装载合同实体。
    //
    // `contract_id` 为所选合同稳定身份。合同在范围内时返回实体。不可见或缺失时返回 NotFound。
    //
    // `find_by_id` 只在 `require_with` 之后装载业务字段，不得单独作为授权。
    pub(super) async fn load_contract(
        &self,
        contract_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Contract> {
        self.require_contract(contract_id, executor).await?;
        self.db
            .contracts()
            .find_by_id(contract_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在或无权查看".into()))
    }

    // 在调用方执行器内检查本次客户资格后装载实体。
    //
    // `customer_id` 为合同所属客户。客户满足当前命令需要的资格时返回实体。客户资格不足或缺失时返回错误。
    //
    // 新建、提交及关系变更须先检查当前资格；已有关系的读取保留详情资格。
    pub(super) async fn load_customer(
        &self,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerAccount> {
        self.require_customer(customer_id, executor).await?;
        self.db
            .customer_accounts()
            .find_by_id(customer_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在或无权查看".into()))
    }
}
