//! 供应商身份设施与业务事实装配。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use erp_supplier::{SupplierAccess, SupplierService};
use mongodb::Database;

use super::{MongoSupplierPartyFacts, MongoSupplierSensitiveTokens};

/// 构造绑定身份数据库的供应商访问器。
/// 返回未缓存授权的供应商访问器。
pub fn supplier_access(db: Database, authorization: Arc<AuthorizationService>) -> SupplierAccess {
    SupplierAccess::new(db, authorization)
}

/// 构造已接入身份域公共解析器的供应商服务。
/// 返回可解析供应商范围的服务。
pub fn scoped_supplier_service(db: Database, authorization: Arc<AuthorizationService>) -> SupplierService {
    SupplierService::new(
        db.clone(),
        MongoSupplierPartyFacts::shared(db.clone()),
        SupplierAccess::new(db, authorization),
    )
}

/// 构造可签发敏感字段短时揭示令牌且已接入范围的供应商服务。
/// 返回可解析供应商范围并签发揭示令牌的服务。
pub fn scoped_supplier_service_with_sensitive(
    db: Database,
    authorization: Arc<AuthorizationService>,
    sensitive_data: Arc<SensitiveDataCodec>,
) -> SupplierService {
    SupplierService::with_sensitive_data(
        db.clone(),
        MongoSupplierPartyFacts::shared(db.clone()),
        MongoSupplierSensitiveTokens::shared(sensitive_data),
        SupplierAccess::new(db, authorization),
    )
}
