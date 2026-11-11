//! 抄送以账号资格和本人收件政策接入 identity；查询实现由仓储提供。
use std::sync::Arc;

use erp_identity::entity::authorization::FactKind;
use erp_identity::ports::{AccessFactsPort, AccessMapping};
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry, AuthorizationService,
    DataAccess, Permission, Result, SharedRbacService,
};
use mongodb::Database;

use crate::repository::approval_copy::authorization::{CopyFacts, CopyMapping, RESOURCE};

/// 固定读写动作共享收件人政策，入口标识不要求账号具有同名功能权限。
/// # 参数
/// db、identity 为当前工作流身份设施。
/// # 返回
/// 返回统一授权服务及同一实例的查询 Mapping。
/// # 错误
/// 固定权限、资源模型或登记不符合 identity 合同时返回错误。
pub(super) fn authorization(
    db: &Database,
    identity: &SharedRbacService,
) -> Result<(AuthorizationService, Arc<dyn AccessMapping>)> {
    let mapping = CopyMapping::shared()?;
    let facts: Arc<dyn AccessFactsPort> = Arc::new(CopyFacts(db.clone()));
    let mut entries = Vec::new();
    for action in ["list", "detail", "read"] {
        let data = DataAccess {
            subject: AccessSubject::Direct,
            required_facts: [FactKind::Owner].into(),
            policy: AccessPolicy::Account { policy: Box::new(AccessPolicy::Owned) },
        };
        let access = if action == "list" {
            AccessMode::Query { data, mapping: mapping.clone() }
        } else {
            AccessMode::Object { data, facts: facts.clone() }
        };
        entries.push(AccessRegistration {
            permission: Permission::parse(format!("{RESOURCE}:{action}"))?,
            required_permissions: vec![],
            access,
        });
    }
    Ok((
        AuthorizationService::new(db.clone(), identity.clone(), AuthorizationRegistry::new(entries)?),
        mapping,
    ))
}
