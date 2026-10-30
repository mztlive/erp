use std::sync::Mutex;

use async_trait::async_trait;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::address::{ConfirmedSupplierAddress, SupplierAddressCandidate};
use erp_supply::ports::connector::common::{
    ConnectorError, ConnectorResult, Snapshot, SourceRevision, SourceStamp,
};

use super::*;

struct LiveAddresses {
    calls: Mutex<Vec<AddressCandidateQuery>>,
    fail: bool,
    observed_at: Instant,
    candidates: Vec<SupplierAddressCandidate>,
}

#[async_trait]
impl SupplierAddressSource for LiveAddresses {
    async fn candidates(
        &self,
        query: &AddressCandidateQuery,
    ) -> ConnectorResult<Snapshot<Vec<SupplierAddressCandidate>>> {
        self.calls.lock().unwrap().push(query.clone());
        if self.fail {
            return Err(ConnectorError {
                class: SupplierFailureClass::TransientFailure,
                code: "ADDRESS_READ_FAILED".into(),
                summary: "读取失败".into(),
                retry_after: None,
            });
        }
        Ok(Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: self.observed_at,
            },
            value: self.candidates.clone(),
        })
    }

    async fn validate(&self, _: &ConfirmedSupplierAddress) -> ConnectorResult<Snapshot<bool>> {
        panic!("地址更新提示不得修改或确认任何原订单收货地址")
    }
}

fn source() -> LiveAddresses {
    LiveAddresses {
        calls: Mutex::new(vec![]),
        fail: false,
        observed_at: Instant::from_unix_secs(20),
        candidates: vec![SupplierAddressCandidate {
            depth: SupplierAddressDepth::Province,
            code: "1".into(),
            name: "来源省级名称".into(),
            parent_code: None,
        }],
    }
}

fn hint() -> SupplierRefreshHint {
    SupplierRefreshHint {
        topic: "address".into(),
        target_kind: "supplier_address".into(),
        object_id: "99999".into(),
        product_id: None,
        funds_transaction_id: None,
    }
}

#[tokio::test]
async fn address_notice_refreshes_live_entry_without_guessing_changed_area_depth() {
    let source = source();
    refresh_address_notice(&source, &hint(), Instant::from_unix_secs(10)).await.unwrap();
    assert_eq!(
        *source.calls.lock().unwrap(),
        [AddressCandidateQuery { depth: SupplierAddressDepth::Province, parent_code: None }]
    );
}

#[tokio::test]
async fn unavailable_stale_incomplete_or_wrong_target_notices_cannot_finish() {
    for failure in ["read", "stale", "empty", "duplicate", "wrong-depth", "parent"] {
        let mut source = source();
        match failure {
            "read" => source.fail = true,
            "stale" => source.observed_at = Instant::from_unix_secs(1),
            "empty" => source.candidates.clear(),
            "duplicate" => source.candidates.push(source.candidates[0].clone()),
            "wrong-depth" => source.candidates[0].depth = SupplierAddressDepth::City,
            _ => source.candidates[0].parent_code = Some("99999".into()),
        }
        assert!(refresh_address_notice(&source, &hint(), Instant::from_unix_secs(10)).await.is_err());
    }
    let source = source();
    let mut wrong = hint();
    wrong.target_kind = "sku".into();
    assert!(refresh_address_notice(&source, &wrong, Instant::from_unix_secs(10)).await.is_err());
    assert!(source.calls.lock().unwrap().is_empty());
}
