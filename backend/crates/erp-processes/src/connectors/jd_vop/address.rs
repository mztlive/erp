//! 官方原生地址候选与有效性复核；不把转换或自由填写的编码当作确认事实。
use std::collections::BTreeSet;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_supply::ports::connector::address::{
    AddressCandidateQuery, ConfirmedSupplierAddress, SupplierAddressCandidate, SupplierAddressDepth,
    SupplierAddressSource,
};
use erp_supply::ports::connector::common::{ConnectorResult, Snapshot, SourceRevision, SourceStamp};

use super::transport::JdVopReadRequest;
use super::wire::{Areas, WireText};
use super::{JdVopConnector, mapping, protocol_required};

#[async_trait]
impl SupplierAddressSource for JdVopConnector {
    async fn candidates(
        &self,
        query: &AddressCandidateQuery,
    ) -> ConnectorResult<Snapshot<Vec<SupplierAddressCandidate>>> {
        validate_query(query)?;
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let areas: Areas = reader
            .read(JdVopReadRequest::Areas {
                level: depth_code(query.depth),
                parent_code: query.parent_code.clone(),
            })
            .await?;
        if query.depth == SupplierAddressDepth::Town && areas.area_info_list.is_empty() {
            return Ok(snapshot(vec![SupplierAddressCandidate {
                depth: SupplierAddressDepth::Town,
                code: "0".into(),
                name: "无需第四级地址".into(),
                parent_code: query.parent_code.clone(),
            }]));
        }
        let mut seen = BTreeSet::new();
        let mut candidates = Vec::with_capacity(areas.area_info_list.len());
        for area in areas.area_info_list {
            if area.area_id.0 == "0"
                || area.area_name.trim().is_empty()
                || !seen.insert(area.area_id.0.clone())
            {
                return Err(mapping("京东地址候选重复或资料不完整"));
            }
            candidates.push(SupplierAddressCandidate {
                depth: query.depth,
                code: area.area_id.0,
                name: area.area_name.trim().into(),
                parent_code: query.parent_code.clone(),
            });
        }
        Ok(snapshot(candidates))
    }

    async fn validate(&self, address: &ConfirmedSupplierAddress) -> ConnectorResult<Snapshot<bool>> {
        if address.connection_id != self.connection_id.as_ref()
            || address.version == 0
            || address.snapshot_reference.is_empty()
            || address.address_fingerprint.is_empty()
        {
            return Err(mapping("京东地址与原连接或执行快照不符"));
        }
        let mut parent_code = None;
        for (depth, level) in [
            (SupplierAddressDepth::Province, &address.province),
            (SupplierAddressDepth::City, &address.city),
            (SupplierAddressDepth::County, &address.county),
            (SupplierAddressDepth::Town, &address.town),
        ] {
            let candidates = self.candidates(&AddressCandidateQuery { depth, parent_code }).await?;
            if !candidates
                .value
                .iter()
                .any(|candidate| candidate.code == level.code && candidate.name == level.name)
            {
                return Ok(snapshot(false));
            }
            parent_code = Some(level.code.clone());
        }
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let valid: WireText = reader
            .read(JdVopReadRequest::VerifyAddress {
                province: address.province.code.clone(),
                city: address.city.code.clone(),
                county: address.county.code.clone(),
                town: address.town.code.clone(),
            })
            .await?;
        match valid.0.as_str() {
            "true" => Ok(snapshot(true)),
            "false" => Ok(snapshot(false)),
            _ => Err(mapping("京东地址有效性结果未识别")),
        }
    }
}

fn validate_query(query: &AddressCandidateQuery) -> ConnectorResult<()> {
    let province = query.depth == SupplierAddressDepth::Province;
    let valid_parent = query.parent_code.as_ref().is_some_and(|code| {
        !code.is_empty() && code != "0" && code.len() <= 30 && code.bytes().all(|byte| byte.is_ascii_digit())
    });
    if (province && query.parent_code.is_some()) || (!province && !valid_parent) {
        return Err(mapping("请选择京东上一层地址候选后继续"));
    }
    Ok(())
}

fn depth_code(depth: SupplierAddressDepth) -> u8 {
    match depth {
        SupplierAddressDepth::Province => 1,
        SupplierAddressDepth::City => 2,
        SupplierAddressDepth::County => 3,
        SupplierAddressDepth::Town => 4,
    }
}

fn snapshot<T>(value: T) -> Snapshot<T> {
    Snapshot {
        stamp: SourceStamp {
            revision: SourceRevision::Unversioned,
            changed_at: None,
            observed_at: Instant::now(),
        },
        value,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use erp_core::ids::SupplierApiConnectionId;
    use erp_supply::ports::connector::address::SupplierAddressLevel;
    use serde_json::{Value, json};

    use super::super::JdVopOfferMapper;
    use super::super::transport::{JdVopReadTransport, ReadClient};
    use super::*;

    #[derive(Default)]
    struct Transport {
        expected: Mutex<VecDeque<(JdVopReadRequest, Value)>>,
        count: Mutex<usize>,
    }

    #[async_trait]
    impl JdVopReadTransport for Transport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            *self.count.lock().unwrap() += 1;
            let (expected, result) = self.expected.lock().unwrap().pop_front().expect("expected read");
            assert_eq!(&expected, request);
            let key = format!("{}_responce", request.method().replace('.', "_"));
            Ok(serde_json::to_vec(
                &json!({key:{"openRpcResult":{"success":true,"resultCode":"0000","result":result}}}),
            )
            .unwrap())
        }
    }

    fn connector(expected: Vec<(JdVopReadRequest, Value)>) -> (JdVopConnector, Arc<Transport>) {
        let transport = Arc::new(Transport { expected: Mutex::new(expected.into()), ..Transport::default() });
        (
            JdVopConnector {
                connection_id: SupplierApiConnectionId::new("connection"),
                account_scope: "account-test".into(),
                binding: "binding".into(),
                cursor_secret: "secret".into(),
                reader: Some(Arc::new(ReadClient::new(transport.clone(), 2, 100, 5))),
                mapper: JdVopOfferMapper::default(),
                writer: None,
                profile: None,
                evidence_codec: None,
            },
            transport,
        )
    }

    fn address() -> ConfirmedSupplierAddress {
        let level = |code: &str| SupplierAddressLevel { code: code.into(), name: code.into() };
        ConfirmedSupplierAddress {
            connection_id: "connection".into(),
            snapshot_reference: "snapshot".into(),
            address_fingerprint: "fingerprint".into(),
            version: 1,
            province: level("1"),
            city: level("2"),
            county: level("3"),
            town: level("4"),
        }
    }

    #[tokio::test]
    async fn invalid_parent_is_rejected_before_any_external_read() {
        let (connector, transport) = connector(vec![]);
        assert!(
            connector
                .candidates(&AddressCandidateQuery { depth: SupplierAddressDepth::City, parent_code: None })
                .await
                .is_err()
        );
        assert!(
            connector
                .candidates(&AddressCandidateQuery {
                    depth: SupplierAddressDepth::Province,
                    parent_code: Some("erp-region".into())
                })
                .await
                .is_err()
        );
        assert_eq!(*transport.count.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn candidate_lookup_preserves_supplier_codes_and_requested_parent() {
        let (connector, _) = connector(vec![(
            JdVopReadRequest::Areas { level: 2, parent_code: Some("1".into()) },
            json!({"areaInfoList":[{"areaId":9007199254740993u64,"areaName":" 北京 "}]}),
        )]);
        let values = connector
            .candidates(&AddressCandidateQuery {
                depth: SupplierAddressDepth::City,
                parent_code: Some("1".into()),
            })
            .await
            .unwrap()
            .value;
        assert_eq!(values[0].code, "9007199254740993");
        assert_eq!(values[0].name, "北京");
        assert_eq!(values[0].parent_code.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn confirmed_address_rechecks_entire_path_then_remote_validity() {
        let mut responses = Vec::new();
        for (level, id, parent) in
            [(1, "1", None), (2, "2", Some("1")), (3, "3", Some("2")), (4, "4", Some("3"))]
        {
            responses.push((
                JdVopReadRequest::Areas { level, parent_code: parent.map(str::to_owned) },
                json!({"areaInfoList":[{"areaId":id,"areaName":id}]}),
            ));
        }
        responses.push((
            JdVopReadRequest::VerifyAddress {
                province: "1".into(),
                city: "2".into(),
                county: "3".into(),
                town: "4".into(),
            },
            json!(true),
        ));
        let (connector, transport) = connector(responses);
        assert!(SupplierAddressSource::validate(&connector, &address()).await.unwrap().value);
        assert_eq!(*transport.count.lock().unwrap(), 5);
    }

    #[tokio::test]
    async fn changed_candidate_name_does_not_silently_replace_confirmed_address() {
        let (connector, transport) = connector(vec![(
            JdVopReadRequest::Areas { level: 1, parent_code: None },
            json!({"areaInfoList":[{"areaId":"1","areaName":"changed"}]}),
        )]);
        assert!(!SupplierAddressSource::validate(&connector, &address()).await.unwrap().value);
        assert_eq!(*transport.count.lock().unwrap(), 1);
    }

    fn path_responses(towns: Value) -> Vec<(JdVopReadRequest, Value)> {
        let mut responses = [(1, "1", None), (2, "2", Some("1")), (3, "3", Some("2"))]
            .into_iter()
            .map(|(level, id, parent)| {
                (
                    JdVopReadRequest::Areas { level, parent_code: parent.map(str::to_owned) },
                    json!({"areaInfoList":[{"areaId":id,"areaName":id}]}),
                )
            })
            .collect::<Vec<_>>();
        responses.push((JdVopReadRequest::Areas { level: 4, parent_code: Some("3".into()) }, towns));
        responses
    }

    #[tokio::test]
    async fn absent_town_is_a_confirmable_zero_only_after_source_path_and_validity_checks() {
        let mut responses = vec![(
            JdVopReadRequest::Areas { level: 4, parent_code: Some("3".into()) },
            json!({"areaInfoList":[]}),
        )];
        responses.extend(path_responses(json!({"areaInfoList":[]})));
        responses.push((
            JdVopReadRequest::VerifyAddress {
                province: "1".into(),
                city: "2".into(),
                county: "3".into(),
                town: "0".into(),
            },
            json!(true),
        ));
        let (connector, transport) = connector(responses);
        let candidate = connector
            .candidates(&AddressCandidateQuery {
                depth: SupplierAddressDepth::Town,
                parent_code: Some("3".into()),
            })
            .await
            .unwrap()
            .value
            .remove(0);
        assert_eq!(candidate.code, "0");
        assert_eq!(candidate.parent_code.as_deref(), Some("3"));
        let mut address = address();
        address.town = SupplierAddressLevel { code: candidate.code, name: candidate.name };
        assert!(SupplierAddressSource::validate(&connector, &address).await.unwrap().value);
        assert_eq!(*transport.count.lock().unwrap(), 6);
    }

    #[tokio::test]
    async fn zero_town_is_rejected_if_source_has_towns_or_remote_validation_rejects_it() {
        for (towns, valid, count) in [
            (json!({"areaInfoList":[{"areaId":"4","areaName":"4"}]}), None, 4),
            (json!({"areaInfoList":[]}), Some(false), 5),
        ] {
            let mut responses = path_responses(towns);
            if let Some(valid) = valid {
                responses.push((
                    JdVopReadRequest::VerifyAddress {
                        province: "1".into(),
                        city: "2".into(),
                        county: "3".into(),
                        town: "0".into(),
                    },
                    json!(valid),
                ));
            }
            let (connector, transport) = connector(responses);
            let mut address = address();
            address.town = SupplierAddressLevel { code: "0".into(), name: "无需第四级地址".into() };
            assert!(!SupplierAddressSource::validate(&connector, &address).await.unwrap().value);
            assert_eq!(*transport.count.lock().unwrap(), count);
        }
    }
}
