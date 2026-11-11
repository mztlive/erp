//! 原回款登记人的固定提交资格和所有者事实。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::repository::ReceivableExt;
use erp_identity::entity::authorization::{AccessContext, FactKind, FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

struct ReceiptFacts(Database);

fn owner_fact(receipt: CustomerReceipt) -> ObjectFacts {
    ObjectFacts {
        resource: "customer_receipt".into(),
        id: receipt.base.id.to_string(),
        version: receipt.base.version.to_string(),
        owner_id: FactValue::Known(Some(receipt.created_by)),
        ..Default::default()
    }
}

#[async_trait]
impl AccessFactsPort for ReceiptFacts {
    async fn load(
        &self,
        _: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut facts = Vec::new();
        for id in targets.ids() {
            if let Some(receipt) = self.0.customer_receipts().find_by_id(id, executor).await? {
                facts.push(owner_fact(receipt));
            }
        }
        Ok(facts)
    }
}

/// 原单读取、编辑及重提均由 identity 核验原登记人和当前提交权限。
/// # 参数
/// db、identity 为原业务设施；actor、id 和 executor 为原命令上下文。
/// # 返回
/// 当前提交权限和原登记人事实同时满足时成功。
/// # 错误
/// 账号、权限、原单或所有者资格不满足时拒绝；读取错误透传。
pub(super) async fn require_receipt_owner(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse("customer_receipt:submit")?;
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner].into(),
                policy: AccessPolicy::Owned,
            },
            facts: Arc::new(ReceiptFacts(db.clone())),
        },
    }])?;
    Ok(AuthorizationService::new(db.clone(), identity.clone(), registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
            executor,
        )
        .await?)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerReceiptId, PartyId};
    use erp_finance::entity::receivable::CustomerReceiptData;

    use super::*;

    #[test]
    fn receipt_owner_is_the_original_registrant_and_empty_legacy_owner_is_known() {
        let receipt = CustomerReceipt::new(
            CustomerReceiptId::new("receipt"),
            CustomerReceiptData {
                receipt_no: "RC-1".into(),
                counterparty_party_id: PartyId::new("party"),
                customer_id: None,
                received_at: Instant::from_unix_secs(10),
                amount: "100.00".parse().unwrap(),
                bank_reference: None,
            },
            "creator",
        )
        .unwrap();
        let fact = owner_fact(receipt.clone());
        assert_eq!(fact.id, "receipt");
        assert_eq!(fact.owner_id, FactValue::Known(Some("creator".into())));
        let mut legacy = receipt;
        legacy.created_by.clear();
        assert_eq!(owner_fact(legacy).owner_id, FactValue::Known(Some(String::new())));
    }
}
