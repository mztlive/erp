//! 在采购创建与提交中委派供应商资格及唯一付款条件规则。
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SupplierAccountId;
use erp_procurement::entity::facts::{PaymentTermFact, SupplierRoleFact};
use erp_procurement::entity::purchase_order::{CreationBasisFacts, PurchaseType};
use erp_procurement::ports::creation_basis::CreationBasisSupplierPort;
use erp_supplier::SupplierExt;
use erp_supplier::entity::supplier::eligibility::OfferingProductKind;
use mongodb::Database;
use persistence_core::Executor;
/// 绑定组合根数据库，不提前执行读取。
pub struct CreationBasisSupplierAdapter {
    db: Database,
    verified: Option<(SupplierAccountId, Option<SupplierRoleFact>)>,
}
impl CreationBasisSupplierAdapter {
    /// 绑定数据库，供应商事实仍在调用方执行器中读取。
    /// # 参数
    /// 组合根数据库。
    /// # 返回
    /// 不持有已读取事实的供应商端口。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db, verified: None }
    }

    /// 复用 guard 后原事务批量取得的指定供应商事实，冻结草稿时不重复查询。
    /// # 参数
    /// 数据库、指定供应商以及本阶段已验证的创建依据事实。
    /// # 返回
    /// 指定供应商的窄事实端口；其他供应商仍通过原执行器读取。
    /// # 错误
    /// 无；缺失供应商及商务指针仍由采购草稿构造报告原有错误。
    ///
    /// 只能用于此事实取得后的同一事务阶段；任何供应商事实写入后必须重新取证。
    pub fn from_facts(db: Database, supplier: &SupplierAccountId, facts: &CreationBasisFacts) -> Self {
        Self { db, verified: Some((supplier.clone(), facts.suppliers.get(supplier.as_ref()).cloned())) }
    }
}
#[async_trait]
impl CreationBasisSupplierPort for CreationBasisSupplierAdapter {
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        purchase_type: PurchaseType,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_procurement::Result<()> {
        let kind = match purchase_type {
            PurchaseType::Physical => OfferingProductKind::Physical,
            PurchaseType::Virtual => OfferingProductKind::Virtual,
            PurchaseType::Service => OfferingProductKind::OfflineService,
        };
        erp_supplier::service::supplier::eligibility::ensure_offering_capability_qualified(
            &self.db,
            supplier_id,
            kind,
            on_date,
            executor,
        )
        .await
        .map_err(map_supplier_error)
    }
    /// 指定供应商复用原事务批量事实，缺键和缺商务指针保持原样；其他身份仍查库。
    async fn supplier_role(
        &self,
        id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> erp_procurement::Result<Option<SupplierRoleFact>> {
        if let Some((cached_id, supplier)) = &self.verified
            && cached_id == id
        {
            return Ok(supplier.clone());
        }
        Ok(self.db.supplier_accounts().find_by_id(id, executor).await?.map(|supplier| SupplierRoleFact {
            current_commercial_profile_revision_id: supplier.current_commercial_profile_revision_id,
        }))
    }
    fn payment_term(&self, code: &str) -> erp_core::Result<PaymentTermFact> {
        super::super::adapters::payment_term::parse(code)
    }
    fn payment_snapshot(&self, code: &str) -> erp_core::Result<PaymentTermFact> {
        super::super::adapters::payment_term::parse_snapshot(code)
    }
}

/// 跨域错误保留业务拒绝、仓储错误与事务冲突类别。
fn map_supplier_error(error: erp_supplier::Error) -> erp_procurement::Error {
    use erp_procurement::Error as Target;
    use erp_supplier::Error as Source;
    match error {
        Source::Internal(message) => Target::Internal(message),
        Source::NotFound(message) => Target::NotFound(message),
        Source::ValidationError(message) => Target::ValidationError(message),
        Source::BusinessLogicError(message) => Target::BusinessLogicError(message),
        Source::ConflictError(message) => Target::ConflictError(message),
        Source::ReceiptDuplicate(error) => Target::ReceiptDuplicate(error),
        Source::TransientTransaction(error) => Target::TransientTransaction(error),
        Source::Forbidden(message) => Target::Forbidden(message),
        Source::Unauthenticated(message) => Target::Unauthenticated(message),
        Source::Logic(error) => Target::Logic(error),
        Source::OutcomeUnknown(error) => Target::OutcomeUnknown(error),
        Source::RepositoryError(error) => Target::RepositoryError(error),
    }
}
