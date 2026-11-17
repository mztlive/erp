//! 结算服务和流程的组合根装配。
use std::sync::Arc;

use erp_identity::{AuthorizationService, SharedRbacService};
use erp_supply::SettlementAccess;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply_settlement::supply_settlement::SupplierSettlementProcess;
use mongodb::Database;

use super::workflow::{WorkflowAuth, work_item_service};

/// 构造绑定身份数据库的结算访问器。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `authorization` - 进程共享统一授权服务
///
/// # 返回
/// 返回无请求状态的结算访问器。
///
/// # 错误
/// 不返回错误。
pub fn settlement_access(db: Database, authorization: Arc<AuthorizationService>) -> SettlementAccess {
    SettlementAccess::new(db, authorization)
}

/// 构造已接入公共解析器的结算服务。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `authorization` - 进程共享统一授权服务
///
/// # 返回
/// 返回可解析结算范围的服务。
///
/// # 错误
/// 不返回错误。
pub fn scoped_settlement_service(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> SupplierSettlementService {
    SupplierSettlementService::new(db.clone()).with_data_scope(SettlementAccess::new(db, authorization))
}

/// 构造已接入公共解析器的结算流程。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 工作项身份与人员资格设施
/// * `authorization` - 进程共享统一授权服务
///
/// # 返回
/// 返回可解析结算范围的跨域流程。
///
/// # 错误
/// 不返回错误。
pub fn scoped_settlement_process(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> SupplierSettlementProcess<WorkflowAuth> {
    SupplierSettlementProcess::new(
        db.clone(),
        SettlementAccess::new(db.clone(), authorization.clone()),
        rbac.clone(),
        authorization.clone(),
        work_item_service(db, rbac, authorization),
    )
}
