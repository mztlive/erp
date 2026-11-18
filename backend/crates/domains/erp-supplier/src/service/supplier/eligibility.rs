//! 供应商能力修订合格校验的服务编排。
//!
//! 纯业务规则已下沉至 `crate::entity::supplier::eligibility`；本模块仅负责
//! 已持久化事实的加载与领域判定的适配，不持有可复用的校验实现。

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SupplierAccountId, SupplierCapabilityRevisionId};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier::eligibility::{
    OfferingProductKind, ensure_capability_qualified as ensure_qualified_domain, required_offering_capability,
};
use crate::entity::supplier::{
    CapabilityCode, SupplierAccount, SupplierCapability, SupplierCapabilityRevision,
};
use crate::error::{Error, Result};
use crate::repository::SupplierExt;
use crate::repository::prelude::*;

/// 检查供应商能否在指定业务日提供该类商品。用调用方执行器先找所需能力及当前修订，再重读账号、修订、能力
/// 并核对关联合同；不自行开启事务或缓存资格。
/// 未配置能力或没有当前修订先失败；账号缺失、停用、修订非当前、越过有效期或关联合同不合格也拒绝，仓储错
/// 误向上传播。
pub async fn ensure_offering_capability_qualified(
    db: &Database,
    supplier_id: &SupplierAccountId,
    kind: OfferingProductKind,
    on_date: BusinessDate,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_offering_with_port(db, supplier_id, kind, on_date, executor).await
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
#[async_trait]
impl QualificationFactsPort for Database {
    async fn linked_contracts(
        &self,
        supplier_id: &SupplierAccountId,
        capability_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<crate::SupplierQualification>> {
        Ok(self.supplier_qualifications().linked_contracts(supplier_id, capability_id, executor).await?)
    }
    async fn supplier(
        &self,
        id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        self.supplier_accounts().find_by_id(id, executor).await.map_err(Into::into)
    }
    async fn revision(
        &self,
        id: &SupplierCapabilityRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCapabilityRevision> {
        self.supplier_capability_revisions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("找不到该供给能力的版本，请核对供应商档案".to_string()))
    }
    async fn capability(
        &self,
        id: &SupplierAccountId,
        code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCapability>> {
        self.supplier_capabilities().find_by_supplier_and_code(id, code, executor).await.map_err(Into::into)
    }
}
// 根据商品类型加载供给所需能力及其当前修订指针。缺能力或缺当前指针立即返回，避免被后续供应商或修订读取错
// 误掩盖。
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
        .ok_or_else(|| {
            Error::BusinessLogicError(
                "该供应商未启用此类商品所需的供给能力，请调整供应商能力或另选供应商".to_string(),
            )
        })?;
    let revision_id =
        capability.stable.current_revision_id.as_deref().map(SupplierCapabilityRevisionId::new).ok_or_else(
            || Error::BusinessLogicError("该供给能力缺少当前资料，请先维护供应商档案".to_string()),
        )?;
    ensure_qualified_with_port(port, supplier_id, &revision_id, on_date, executor).await
}
// 复用同一执行器依次读取供应商、指定修订及对应能力，交给领域规则判定是否仍合格；通过后才加载其关联的合同
// 核验业务日资格。
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

    ensure_qualified_domain(&supplier, &capability, &revision, on_date)
        .map_err(|violation| Error::BusinessLogicError(violation.to_string()))?;

    let contracts = port.linked_contracts(supplier_id, &capability.base.id, executor).await?;
    crate::entity::supplier::eligibility::ensure_linked_contracts_qualified(&contracts, on_date)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

#[cfg(test)]
mod provider_tests {
    use std::sync::Mutex;

    use erp_core::ids::{PartyId, SupplierCapabilityId};
    use persistence_core::NoTransaction;

    use super::*;
    use crate::entity::supplier::{
        CapabilityStatus, SupplierAccountData, SupplierAccountStatus, SupplierCapabilityData,
        SupplierCapabilityRevisionData,
    };
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
        let mut executor = NoTransaction;
        let port = Recorder {
            pointer: &mut executor as *mut NoTransaction as usize,
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
        assert!(
            matches!(result,Err(Error::BusinessLogicError(ref e)) if e=="该供给能力缺少当前资料，请先维护供应商档案")
        );
        assert_eq!(calls, ["capability"]);

        let (result, calls) = invoke(None, false, false, true).await;
        assert!(
            matches!(result,Err(Error::BusinessLogicError(ref e)) if e=="供应商能力已停用、过期或版本已变化")
        );
        assert_eq!(calls.len(), 4);
    }
    #[tokio::test]
    async fn unverified_linked_contract_blocks_offering_through_real_orchestration() {
        let mut executor = NoTransaction;
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
            pointer: &mut executor as *mut NoTransaction as usize,
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
