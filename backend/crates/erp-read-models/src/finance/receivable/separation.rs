//! Read-only evidence supporting financial reviewer separation checks.

use erp_audit::AuditExt;
use erp_finance::entity::receivable::{
    CustomerReceiptStatus, InvoiceDirection, InvoiceStatus, ReceivableAccount,
};
use erp_finance::ports::receivable::CardFundsSnapshot;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;
use services::{Error, Result};
use std::collections::HashMap;

/// 重验责任资格，并对已登记票款事实执行可证明的经办/复核岗位分离。
///
/// 审计事实经 `list_separation_facts_by_resources` 单次批量装载（数量增长时
/// 查询保持常数），SoD 政策解释仍在本函数与 `check_fact_separation` 中。
pub async fn validate_card_funds_reviewer_separation(
    db: &Database,
    account: &ReceivableAccount,
    snapshot: &CardFundsSnapshot,
    work_item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (work_item, actor_id);

    for receipt in &snapshot.receipts {
        if !matches!(
            receipt.status,
            CustomerReceiptStatus::Posted | CustomerReceiptStatus::Reversed
        ) || receipt.counterparty_party_id != account.counterparty_party_id
        {
            return Err(Error::BusinessLogicError(
                "复核引用的回款事实未正式过账或往来主体不一致".to_string(),
            ));
        }
    }

    for invoice in &snapshot.invoices {
        if invoice.invoice_direction != InvoiceDirection::Sales
            || !matches!(
                invoice.stable.status(),
                InvoiceStatus::Registered | InvoiceStatus::RedInvoiced
            )
            || invoice.party_id != account.counterparty_party_id
        {
            return Err(Error::BusinessLogicError(
                "复核引用的销项发票未正式登记或往来主体不一致".to_string(),
            ));
        }
    }

    let mut pairs = Vec::with_capacity(snapshot.receipts.len() + snapshot.invoices.len());
    for receipt in &snapshot.receipts {
        pairs.push(("customer_receipt".to_string(), receipt.base.id.clone()));
    }
    for invoice in &snapshot.invoices {
        pairs.push(("invoice".to_string(), invoice.base.id.clone()));
    }
    let facts = db
        .audit_logs()
        .list_separation_facts_by_resources(&pairs, executor)
        .await?;
    let mut by_resource: HashMap<(String, String), Vec<&erp_audit::SeparationAuditFact>> = HashMap::new();
    for fact in &facts {
        if let Some(resource_id) = fact.resource_id.as_deref() {
            by_resource
                .entry((fact.resource_type.clone(), resource_id.to_string()))
                .or_default()
                .push(fact);
        }
    }

    for receipt in &snapshot.receipts {
        let key = ("customer_receipt".to_string(), receipt.base.id.clone());
        let empty = Vec::new();
        let resource_facts = by_resource.get(&key).unwrap_or(&empty);
        check_fact_separation(
            resource_facts,
            actor_id,
            &["customer_receipt.create", "customer_receipt.post:"],
            &["customer_receipt.post:"],
        )?;
    }

    for invoice in &snapshot.invoices {
        let key = ("invoice".to_string(), invoice.base.id.clone());
        let empty = Vec::new();
        let resource_facts = by_resource.get(&key).unwrap_or(&empty);
        check_fact_separation(
            resource_facts,
            actor_id,
            &["invoice.create", "invoice.post", "invoice.red_issue"],
            &["invoice.post", "invoice.red_issue"],
        )?;
    }
    Ok(())
}
/// 从批量装载的最小审计事实证明票款已正式登记且当前复核人不是其经办人。
///
/// 纯策略解释：缺正式证据 fail closed；同 actor 经办冲突拒绝；仅非正式或
/// 失败事件（调用方批量查询已限定成功）不算证据。SoD 规则、当前 actor、
/// 拒绝文案与授权决定保留 Service，不得下沉。
fn check_fact_separation(
    facts: &[&erp_audit::SeparationAuditFact],
    actor_id: &str,
    operator_actions: &[&str],
    formal_actions: &[&str],
) -> Result<()> {
    let matches_action =
        |action: &str, prefixes: &[&str]| prefixes.iter().any(|prefix| action.starts_with(prefix));
    if !facts
        .iter()
        .any(|fact| matches_action(&fact.action, formal_actions))
    {
        return Err(Error::Forbidden(
            "无法从审计事实证明票款已经正式登记，岗位分离校验失败关闭".to_string(),
        ));
    }
    if facts
        .iter()
        .any(|fact| fact.actor_id == actor_id && matches_action(&fact.action, operator_actions))
    {
        return Err(Error::Forbidden(
            "票款事实经办人与最终复核人必须岗位分离".to_string(),
        ));
    }
    Ok(())
}
#[cfg(test)]
mod fact_separation_tests {
    use super::check_fact_separation;
    use erp_audit::SeparationAuditFact;

    fn fact(actor: &str, action: &str) -> SeparationAuditFact {
        SeparationAuditFact {
            resource_type: "customer_receipt".to_string(),
            resource_id: Some("cr-1".to_string()),
            actor_id: actor.to_string(),
            action: action.to_string(),
        }
    }

    const OPERATOR: &[&str] = &["customer_receipt.create", "customer_receipt.post:"];
    const FORMAL: &[&str] = &["customer_receipt.post:"];

    /// 正式证据存在且经办人不同时通过。
    #[test]
    fn formal_evidence_by_other_actor_passes() {
        let facts = [
            fact("creator-1", "customer_receipt.create"),
            fact("poster-1", "customer_receipt.post:registered"),
        ];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_ok());
    }

    /// 缺正式证据 fail closed。
    #[test]
    fn missing_formal_evidence_fails_closed() {
        let facts = [fact("creator-1", "customer_receipt.create")];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_err());
        let empty: Vec<&SeparationAuditFact> = Vec::new();
        assert!(check_fact_separation(&empty, "reviewer-1", OPERATOR, FORMAL).is_err());
    }

    /// 同 actor 经办冲突拒绝。
    #[test]
    fn same_actor_operator_conflict_is_rejected() {
        let facts = [
            fact("reviewer-1", "customer_receipt.create"),
            fact("poster-1", "customer_receipt.post:registered"),
        ];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_err());
    }

    /// 仅非正式事件不算证据。
    #[test]
    fn informal_only_events_do_not_count_as_evidence() {
        let facts = [
            fact("creator-1", "customer_receipt.draft_saved"),
            fact("creator-1", "customer_receipt.preview"),
        ];
        let refs = facts.iter().collect::<Vec<_>>();
        assert!(check_fact_separation(&refs, "reviewer-1", OPERATOR, FORMAL).is_err());
    }
}
