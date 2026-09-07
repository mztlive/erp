//! 原实体 BSON 往返合同在持久化层执行，复用原 fixture 与完整断言。

mod inbox_message {
    use crate::entity::integration_ops::inbox_message::tests::message_data;
    use crate::entity::integration_ops::{InboxMessage, InboxMessageId};

    #[test]
    fn entity_roundtrip_through_bson() {
        let message = InboxMessage::new(InboxMessageId::new("msg-11"), message_data()).unwrap();
        let roundtrip: InboxMessage =
            bson::deserialize_from_document(bson::serialize_to_document(&message).unwrap()).unwrap();
        assert_eq!(roundtrip, message);
    }
}

mod integration_error_task {
    use crate::entity::integration_ops::integration_error_task::tests::task;
    use crate::entity::integration_ops::IntegrationErrorTask;

    #[test]
    fn entity_roundtrip_through_bson() {
        let created = task();
        let roundtrip: IntegrationErrorTask =
            bson::deserialize_from_document(bson::serialize_to_document(&created).unwrap()).unwrap();
        assert_eq!(roundtrip, created);
    }
}

mod reconciliation_difference {
    use crate::entity::integration_ops::reconciliation_difference::tests::difference_data;
    use crate::entity::integration_ops::{ReconciliationDifference, ReconciliationDifferenceId};

    #[test]
    fn entity_roundtrip_through_bson() {
        let difference =
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-9"), difference_data())
                .unwrap();
        let roundtrip: ReconciliationDifference =
            bson::deserialize_from_document(bson::serialize_to_document(&difference).unwrap()).unwrap();
        assert_eq!(roundtrip, difference);
    }
}

mod reconciliation_difference_resolution {
    use crate::entity::integration_ops::reconciliation_difference_resolution::tests::data;
    use crate::entity::integration_ops::{
        ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionId, ResolutionAction,
    };

    #[test]
    fn entity_roundtrips_through_bson() {
        let mut input = data(ResolutionAction::AddEvidence);
        input.evidence_reference = Some(" audit://evidence-1 ".to_string());
        let record = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-2"),
            input,
        )
        .unwrap();
        let decoded: ReconciliationDifferenceResolution =
            bson::deserialize_from_document(bson::serialize_to_document(&record).unwrap()).unwrap();

        assert_eq!(decoded, record);
        assert_eq!(record.evidence_reference.as_deref(), Some("audit://evidence-1"));
    }
}
