//! 开票命令在原操作资格下校验真实销售来源。
use std::sync::Arc;

use erp_permissions::{internal, receivable, sales_order};
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::Error;
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{AccessTarget, RelatedObjects, RelationKey};

#[tokio::test]
async fn invoice_request_commands_require_the_original_action_and_its_roles_source_scope() {
    for (entry, action, other_action) in [
        (
            internal::INVOICE_REQUEST_SUBMIT_SOURCE,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
        ),
        (
            internal::INVOICE_REQUEST_CANCEL_SOURCE,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
        ),
    ] {
        let identity = Arc::new(Identity::default());
        identity.role_scopes.lock().unwrap().extend([
            (("handler".into(), sales_order::DETAIL), ScopeClause { self_owned: true, ..Default::default() }),
            (("reader".into(), sales_order::DETAIL), ScopeClause { company: true, ..Default::default() }),
        ]);
        let request = |source_owner| {
            let mut request = object("sales_invoice_request", "request", "user");
            request.relations.insert(
                RelationKey::parse("sales_source").unwrap(),
                RelatedObjects {
                    expected_ids: ["order".into()].into(),
                    objects: vec![object("sales_order", "order", source_owner)],
                },
            );
            request
        };
        let port = Arc::new(Facts::new(vec![request("user")]));
        let mut builder = complete();
        builder.facilities = identity.clone();
        builder.facts.insert("sales_invoice_request".into(), port.clone());
        let service = builder.build().unwrap();
        let require = || async {
            service
                .require(&actor(), &entry, AccessTarget::record("request")?, &mut SnapshotExecutor::default())
                .await
        };

        identity.state.lock().unwrap().grants = vec![("handler".into(), vec![action])];
        require().await.unwrap();

        identity.state.lock().unwrap().grants.push(("reader".into(), vec![sales_order::DETAIL]));
        port.objects.lock().unwrap()[0] = request("other");
        assert!(matches!(require().await, Err(Error::Forbidden(_))));

        port.objects.lock().unwrap()[0] = request("user");
        identity.state.lock().unwrap().grants[0].1 = vec![other_action];
        assert!(matches!(require().await, Err(Error::Forbidden(_))));
    }
}
