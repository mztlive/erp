//! 供应商能力修订合格校验的服务编排。
//!
//! 纯业务规则已下沉至 `crate::entity::supplier::eligibility`；本模块仅负责
//! 已持久化事实的加载与领域判定的适配，不持有可复用的校验实现。

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SupplierAccountId, SupplierCapabilityRevisionId};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::entity::supplier::eligibility::{
    CapabilityEligibilityViolation, OfferingProductKind,
    ensure_capability_qualified as ensure_qualified_domain, required_offering_capability,
};
use crate::entity::supplier::{
    CapabilityCode, SupplierAccount, SupplierCapability, SupplierCapabilityRevision,
};
use crate::error::{Error, Result};
use crate::repository::SupplierExt;

/// 校验指定供应商能力修订在业务日是否仍为当前启用版本。
///
/// 领域判定已由 `crate::entity::supplier::eligibility::ensure_capability_qualified`
/// 承载；本函数仅加载供应商、能力及修订事实并将领域违例映射为服务层
/// 业务错误，保持既有 API 文案不变。
///
/// 已关联合同必须通过有效期校验；无资质记录时保留当前阶段临时放行政策。
///
/// # 参数
/// * `db` - 数据库实例（调用方执行器，本函数不开启事务）
/// * `supplier_id` - 供应商角色 ID
/// * `capability_revision_id` - 待校验的能力修订 ID
/// * `on_date` - 业务自然日（由调用方显式注入，不读取全局时钟）
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// * `NotFound` - 供应商不存在
/// * `BusinessLogicError` - 供应商已停用、能力不存在、版本不存在或
///   领域判定认为不合格（停用、归属不符、非当前版本、未生效、已过期）
///
/// # 约束
/// * 仅执行事实加载与领域委派，不在 Service 重复实现校验规则
/// * 不开启或提交事务；不持有全局时钟、ID 生成器或密钥
pub async fn ensure_capability_qualified(
    db: &Database,
    supplier_id: &SupplierAccountId,
    capability_revision_id: &SupplierCapabilityRevisionId,
    on_date: BusinessDate,
) -> Result<()> {
    ensure_qualified_with_port(
        &MongoQualificationFacts { db },
        supplier_id,
        capability_revision_id,
        on_date,
        &mut NoTransaction,
    )
    .await
}

/// 校验指定供应商能力修订；所有原读取使用调用方Executor，不另开事务。
///
/// executor 版为唯一实现（erp-supplier-006）：便捷版直接构造
/// `MongoQualificationFacts` 并以内联 `NoTransaction` 委托至此，
/// 不再经第二层薄转发；签名与错误语义保持不变。
pub async fn ensure_capability_qualified_with_executor(
    db: &Database,
    supplier_id: &SupplierAccountId,
    capability_revision_id: &SupplierCapabilityRevisionId,
    on_date: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_qualified_with_port(
        &MongoQualificationFacts { db },
        supplier_id,
        capability_revision_id,
        on_date,
        executor,
    )
    .await
}
/// 读取供给所需当前能力指针，再委派同一权威资格规则。
/// 保留首个能力读取与后续按修订代码的第二次读取，不缓存资格事实。
pub async fn ensure_offering_capability_qualified(
    db: &Database,
    supplier_id: &SupplierAccountId,
    kind: OfferingProductKind,
    on_date: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_offering_with_port(&MongoQualificationFacts { db }, supplier_id, kind, on_date, executor).await
}
#[async_trait]
trait QualificationFactsPort: Sync {
    async fn linked_contracts(
        &self,
        supplier_id: &SupplierAccountId,
        capability_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<crate::SupplierQualification>>;
    async fn supplier(
        &self,
        id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>>;
    async fn revision(
        &self,
        id: &SupplierCapabilityRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCapabilityRevision>;
    async fn capability(
        &self,
        id: &SupplierAccountId,
        code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCapability>>;
}
struct MongoQualificationFacts<'a> {
    db: &'a Database,
}
#[async_trait]
impl QualificationFactsPort for MongoQualificationFacts<'_> {
    async fn linked_contracts(
        &self,
        supplier_id: &SupplierAccountId,
        capability_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<crate::SupplierQualification>> {
        Ok(self.db.supplier_qualifications().linked_contracts(supplier_id, capability_id, executor).await?)
    }
    async fn supplier(
        &self,
        id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        self.db.supplier_accounts().find_by_id(id, executor).await.map_err(Into::into)
    }
    async fn revision(
        &self,
        id: &SupplierCapabilityRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCapabilityRevision> {
        load_capability_revision(self.db, id, executor).await
    }
    async fn capability(
        &self,
        id: &SupplierAccountId,
        code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCapability>> {
        self.db
            .supplier_capabilities()
            .find_by_supplier_and_code(id, code, executor)
            .await
            .map_err(Into::into)
    }
}
/// 首次能力读取先于供应商/修订读取，保留原供给入口首错。
async fn ensure_offering_with_port<P: QualificationFactsPort>(
    port: &P,
    supplier_id: &SupplierAccountId,
    kind: OfferingProductKind,
    on_date: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<()> {
    let capability = port
        .capability(supplier_id, required_offering_capability(kind), executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商未启用该商品类型所需能力".to_string()))?;
    let revision_id = capability
        .stable
        .current_revision_id
        .as_deref()
        .map(SupplierCapabilityRevisionId::new)
        .ok_or_else(|| Error::BusinessLogicError("供应商能力缺少当前版本".to_string()))?;
    ensure_qualified_with_port(port, supplier_id, &revision_id, on_date, executor).await
}
/// 原资格加载与纯规则委派；供应商状态仍在三次读取之后判定。
async fn ensure_qualified_with_port<P: QualificationFactsPort>(
    port: &P,
    supplier_id: &SupplierAccountId,
    capability_revision_id: &SupplierCapabilityRevisionId,
    on_date: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<()> {
    let supplier = port
        .supplier(supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let revision = port.revision(capability_revision_id, executor).await?;
    let capability = port
        .capability(supplier_id, revision.capability_code, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商能力不存在".to_string()))?;
    ensure_qualified_domain(&supplier, &capability, &revision, on_date).map_err(
        |violation| match violation {
            CapabilityEligibilityViolation::SupplierDisabled => {
                Error::BusinessLogicError("供应商已停用，不能用于供给或采购".to_string())
            },
            _ => Error::BusinessLogicError("供应商能力已停用、过期或版本已变化".to_string()),
        },
    )?;

    let contracts = port.linked_contracts(supplier_id, &capability.base.id, executor).await?;
    crate::entity::supplier::eligibility::ensure_linked_contracts_qualified(&contracts, on_date)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))
}
/// 加载能力修订。
///
/// # 参数
/// * `db` - 数据库实例
/// * `revision_id` - 修订 ID
///
/// # 返回
/// 返回修订实体；不存在时返回业务错误。
///
/// # 错误
/// 修订不存在时返回 `BusinessLogicError("供应商能力版本不存在")`。
async fn load_capability_revision(
    db: &Database,
    revision_id: &SupplierCapabilityRevisionId,
    executor: &mut dyn Executor,
) -> Result<SupplierCapabilityRevision> {
    db.supplier_capability_revisions()
        .find_by_id(revision_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商能力版本不存在".to_string()))
}

#[cfg(test)]
mod provider_tests {
    use std::sync::Mutex;

    use erp_core::ids::{PartyId, SupplierCapabilityId};

    use super::*;
    use crate::entity::supplier::{
        CapabilityStatus, SupplierAccountData, SupplierAccountStatus, SupplierCapabilityData,
        SupplierCapabilityRevisionData,
    };
    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Mutex<Vec<&'static str>>,
        fail: Option<usize>,
        disabled: bool,
        missing_pointer: bool,
        second_disabled: bool,
        contracts: Vec<crate::SupplierQualification>,
    }
    impl Recorder {
        fn record(&self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            let mut calls = self.calls.lock().unwrap();
            let i = calls.len();
            calls.push(step);
            if self.fail == Some(i) {
                return Err(Error::ConflictError(format!("qualification {i}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl QualificationFactsPort for Recorder {
        async fn linked_contracts(
            &self,
            supplier: &SupplierAccountId,
            capability: &str,
            executor: &mut dyn Executor,
        ) -> Result<Vec<crate::SupplierQualification>> {
            assert_eq!(supplier.as_ref(), "supplier-1");
            assert_eq!(capability, "cap-1");
            self.record("contracts", executor)?;
            Ok(self.contracts.clone())
        }
        async fn supplier(
            &self,
            id: &SupplierAccountId,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierAccount>> {
            assert_eq!(id.as_ref(), "supplier-1");
            self.record("supplier", executor)?;
            let status =
                if self.disabled { SupplierAccountStatus::Disabled } else { SupplierAccountStatus::Active };
            Ok(Some(test_supplier(status)))
        }
        async fn revision(
            &self,
            id: &SupplierCapabilityRevisionId,
            executor: &mut dyn Executor,
        ) -> Result<SupplierCapabilityRevision> {
            assert_eq!(id.as_ref(), "cap-rev-1");
            self.record("revision", executor)?;
            Ok(test_revision(
                "supplier-1",
                CapabilityCode::Physical,
                CapabilityStatus::Active,
                BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                None,
            ))
        }
        async fn capability(
            &self,
            id: &SupplierAccountId,
            code: CapabilityCode,
            executor: &mut dyn Executor,
        ) -> Result<Option<SupplierCapability>> {
            assert_eq!(id.as_ref(), "supplier-1");
            assert_eq!(code, CapabilityCode::Physical);
            self.record("capability", executor)?;
            let second = self.calls.lock().unwrap().len() > 1;
            let status = if second && self.second_disabled {
                CapabilityStatus::Disabled
            } else {
                CapabilityStatus::Active
            };
            let pointer = if self.missing_pointer { None } else { Some("cap-rev-1") };
            Ok(Some(test_capability("supplier-1", status, pointer)))
        }
    }
    async fn invoke(
        fail: Option<usize>,
        disabled: bool,
        missing_pointer: bool,
        second_disabled: bool,
    ) -> (Result<()>, Vec<&'static str>) {
        let mut executor = Marker(161);
        let port = Recorder {
            pointer: &mut executor as *mut Marker as usize,
            calls: Mutex::new(vec![]),
            fail,
            disabled,
            missing_pointer,
            second_disabled,
            contracts: vec![],
        };
        let result = ensure_offering_with_port(
            &port,
            &SupplierAccountId::new("supplier-1"),
            OfferingProductKind::Physical,
            BusinessDate::from_ymd(2026, 2, 1).unwrap(),
            &mut executor,
        )
        .await;
        assert_eq!(executor.0, 161);
        (result, port.calls.into_inner().unwrap())
    }
    #[tokio::test]
    async fn offering_qualification_checks_contracts_after_original_reads() {
        let (result, calls) = invoke(None, false, false, false).await;
        result.unwrap();
        assert_eq!(calls, ["capability", "supplier", "revision", "capability", "contracts"]);
    }
    #[tokio::test]
    async fn offering_qualification_stops_on_every_provider_error() {
        for i in 0..5 {
            let (result, calls) = invoke(Some(i), false, false, false).await;
            assert!(matches!(result,Err(Error::ConflictError(ref e)) if e==&format!("qualification {i}")));
            assert_eq!(calls, ["capability", "supplier", "revision", "capability", "contracts"][..=i]);
        }
    }
    #[tokio::test]
    async fn disabled_supplier_is_checked_after_all_original_reads() {
        let (result, calls) = invoke(None, true, false, false).await;
        assert!(
            matches!(result,Err(Error::BusinessLogicError(ref e)) if e=="供应商已停用，不能用于供给或采购")
        );
        assert_eq!(calls.len(), 4);
    }
    #[tokio::test]
    async fn missing_pointer_stops_and_second_capability_is_not_cached() {
        let (result, calls) = invoke(None, false, true, false).await;
        assert!(matches!(result,Err(Error::BusinessLogicError(ref e)) if e=="供应商能力缺少当前版本"));
        assert_eq!(calls, ["capability"]);
        let (result, calls) = invoke(None, false, false, true).await;
        assert!(
            matches!(result,Err(Error::BusinessLogicError(ref e)) if e=="供应商能力已停用、过期或版本已变化")
        );
        assert_eq!(calls.len(), 4);
    }
    #[tokio::test]
    async fn unverified_linked_contract_blocks_offering_through_real_orchestration() {
        let mut executor = Marker(161);
        let contract = crate::SupplierQualification::new(
            erp_core::ids::SupplierQualificationId::new("contract"),
            crate::SupplierQualificationData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                qualification_type: crate::QualificationType::Contract,
                certificate_no: "CON-1".into(),
                issuer: None,
                valid_from: None,
                valid_to: Some(BusinessDate::from_ymd(2027, 1, 1).unwrap()),
                attachment_id: None,
                status: crate::QualificationStatus::Active,
            },
            "actor",
        )
        .unwrap();
        let port = Recorder {
            pointer: &mut executor as *mut Marker as usize,
            calls: Mutex::new(vec![]),
            fail: None,
            disabled: false,
            missing_pointer: false,
            second_disabled: false,
            contracts: vec![contract],
        };
        let result = ensure_offering_with_port(
            &port,
            &SupplierAccountId::new("supplier-1"),
            OfferingProductKind::Physical,
            BusinessDate::from_ymd(2026, 6, 1).unwrap(),
            &mut executor,
        )
        .await;
        assert!(
            matches!(result, Err(Error::BusinessLogicError(message)) if message.contains("有效期未核实"))
        );
        assert_eq!(port.calls.into_inner().unwrap().last(), Some(&"contracts"));
    }

    fn test_supplier(status: SupplierAccountStatus) -> SupplierAccount {
        SupplierAccount::new(
            SupplierAccountId::new("supplier-1"),
            SupplierAccountData {
                party_id: PartyId::new("party-1"),
                supplier_no: "S-001".to_string(),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: None,
                maintainer_user_id: "buyer-1".to_string(),
                business_org_unit_id: "org-a".to_string(),
                status,
            },
            "admin-1",
        )
        .unwrap()
    }
    fn test_capability(
        supplier_id: &str,
        status: CapabilityStatus,
        current_revision_id: Option<&str>,
    ) -> SupplierCapability {
        let mut cap = SupplierCapability::new(
            SupplierCapabilityId::new("cap-1"),
            SupplierCapabilityData {
                supplier_id: SupplierAccountId::new(supplier_id),
                capability_code: CapabilityCode::Physical,
                service_region: None,
                owner_user_id: "buyer-1".to_string(),
                fulfillment_note: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                status,
            },
            "admin-1",
        )
        .unwrap();
        cap.stable.current_revision_id = current_revision_id.map(|s| s.to_string());
        cap
    }
    fn test_revision(
        supplier_id: &str,
        code: CapabilityCode,
        status: CapabilityStatus,
        valid_from: BusinessDate,
        valid_to: Option<BusinessDate>,
    ) -> SupplierCapabilityRevision {
        SupplierCapabilityRevision::new(
            SupplierCapabilityRevisionId::new("cap-rev-1"),
            SupplierCapabilityRevisionData {
                supplier_id: SupplierAccountId::new(supplier_id),
                capability_code: code,
                service_region: None,
                owner_user_id: "buyer-1".to_string(),
                fulfillment_note: None,
                valid_from,
                valid_to,
                status,
                revision_no: 1,
            },
        )
        .unwrap()
    }
}
