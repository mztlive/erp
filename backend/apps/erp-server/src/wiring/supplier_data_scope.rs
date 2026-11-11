//! 供应商身份设施与业务事实装配。
use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supplier::{SupplierAccess, SupplierService};
use mongodb::Database;

/// 构造绑定身份数据库的供应商访问器。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回未缓存授权的供应商访问器。
///
/// # 错误
/// 无。
pub fn supplier_access(db: Database, rbac: SharedRbacService) -> SupplierAccess {
    SupplierAccess::new(db, rbac)
}

/// 构造已接入身份域公共解析器的供应商服务。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析供应商范围的服务。
///
/// # 错误
/// 无。
pub fn scoped_supplier_service(db: Database, rbac: SharedRbacService) -> SupplierService {
    SupplierService::new(db.clone(), super::MongoSupplierPartyFacts::shared(db.clone()))
        .with_scope(SupplierAccess::new(db, rbac))
}

/// 构造可签发敏感字段短时揭示令牌且已接入范围的供应商服务。
///
/// # 参数
/// * `db` - 供应商与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
/// * `sensitive_data` - 敏感字段编解码
///
/// # 返回
/// 返回可解析供应商范围并签发揭示令牌的服务。
///
/// # 错误
/// 无。
pub fn scoped_supplier_service_with_sensitive(
    db: Database,
    rbac: SharedRbacService,
    sensitive_data: Arc<erp_party::SensitiveDataCodec>,
) -> SupplierService {
    SupplierService::with_sensitive_data(
        db.clone(),
        super::MongoSupplierPartyFacts::shared(db.clone()),
        super::MongoSupplierSensitiveTokens::shared(sensitive_data),
    )
    .with_scope(SupplierAccess::new(db, rbac))
}
