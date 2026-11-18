//! 京东5853/5854只读资金证据；方向、币种、账号及当地时间口径必须先由账号协议核验。

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::FixedOffset;
use erp_supply::ports::connector::common::{ConnectorResult, Page, Snapshot};
use erp_supply::ports::connector::funds::{
    SupplierFundsBalance, SupplierFundsCurrency, SupplierFundsDirection, SupplierFundsEvidence,
    SupplierFundsObservation, SupplierFundsQuery, SupplierFundsSource,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::Sha256;

use super::delivery::observed;
use super::transport::JdVopReadRequest;
use super::wire_funds::{BalanceChanges, balance_amount, observation};
use super::{JdVopConnector, mapping, protocol_required};

/// 每个业务码的真实收支含义及金额符号均须有本账号证据，不能按文档样例自行启用。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct JdVopFundsTradeRule {
    pub direction: SupplierFundsDirection,
    pub sign: JdVopFundsSign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum JdVopFundsSign {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum JdVopFundsTimeFormat {
    LocalDateTimeSeconds,
}

/// 由受信G0账号核验装配，敏感账号资料不派生Debug。
#[derive(Clone, Serialize)]
pub struct JdVopFundsProfile {
    pub evidence_reference: String,
    pub verified_account_reference: String,
    pub expected_pin: String,
    pub account_type: String,
    pub currency: SupplierFundsCurrency,
    pub timezone_offset_seconds: i32,
    pub time_format: JdVopFundsTimeFormat,
    pub maximum_page_size: NonZeroU32,
    pub trade_types: BTreeMap<String, JdVopFundsTradeRule>,
    pub message_effects: BTreeMap<String, SupplierFundsDirection>,
}

impl JdVopFundsProfile {
    /// 校验外部账号、交易号稳定性、币种、符号及时间口径的已核验资料引用。
    /// # 参数
    /// 无；本资料只能由受信服务端装配，不接收业务前端输入。
    /// # 返回
    /// 所有资金来源解释依据完整时成功。
    /// # 错误
    /// 缺账号或证据、无业务码映射、无效时区及本实现分页上限超过1000时拒绝。
    pub fn validate(&self) -> ConnectorResult<()> {
        if [
            &self.evidence_reference,
            &self.verified_account_reference,
            &self.expected_pin,
            &self.account_type,
        ]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 256)
            || self.trade_types.is_empty()
            || self.trade_types.len() > 100
            || self.trade_types.keys().any(|code| {
                code.is_empty() || code.len() > 32 || !code.bytes().all(|byte| byte.is_ascii_digit())
            })
            || self.maximum_page_size.get() > 1000
            || self.timezone_offset_seconds.unsigned_abs() > 14 * 3600
            || self.timezone_offset_seconds % 60 != 0
            || FixedOffset::east_opt(self.timezone_offset_seconds).is_none()
        {
            return Err(mapping("京东资金账号、业务码、币种或时间口径尚未核实"));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct FundsCursor {
    binding: String,
    since: i64,
    through: i64,
    limit: u32,
    page: u32,
}

impl JdVopConnector {
    pub(super) fn funds_profile(&self) -> ConnectorResult<&JdVopFundsProfile> {
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        let funds = profile.verified_funds_profile.as_ref().ok_or_else(protocol_required)?;
        funds.validate()?;
        if funds.verified_account_reference != profile.verified_external_funds_account_ref
            || self.evidence_codec.is_none()
            || self.reader.is_none()
        {
            return Err(mapping("京东资金读取账号或加密证据配置不一致"));
        }
        Ok(funds)
    }

    fn funds_evidence(&self, raw: &RawValue) -> ConnectorResult<SupplierFundsEvidence> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.cursor_secret.as_bytes())
            .map_err(|_| mapping("资金证据指纹初始化失败"))?;
        mac.update(self.binding.as_bytes());
        mac.update(raw.get().as_bytes());
        let fingerprint = hex::encode(mac.finalize().into_bytes());
        let encrypted = self
            .evidence_codec
            .as_ref()
            .ok_or_else(protocol_required)?
            .encrypt(raw.get())
            .map_err(|_| mapping("京东资金来源证据加密失败"))?;
        Ok(SupplierFundsEvidence {
            reference: format!("jd-funds-{fingerprint}"),
            body_encrypted: encrypted,
            content_fingerprint: fingerprint,
        })
    }

    fn funds_page(&self, query: &SupplierFundsQuery) -> ConnectorResult<u32> {
        let profile = self.funds_profile()?;
        if query.since > query.through || query.limit > profile.maximum_page_size {
            return Err(mapping("京东资金查询窗口或分页上限无效"));
        }
        let Some(cursor) = &query.cursor else {
            return Ok(1);
        };
        if cursor.len() > 4096 {
            return Err(mapping("资金查询游标超过允许大小"));
        }
        let (encoded, signature) = cursor.split_once('.').ok_or_else(|| mapping("资金查询游标无效"))?;
        let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| mapping("资金查询游标无效"))?;
        let mut mac = Hmac::<Sha256>::new_from_slice(self.cursor_secret.as_bytes())
            .map_err(|_| mapping("资金查询游标无效"))?;
        mac.update(&bytes);
        mac.verify_slice(&hex::decode(signature).map_err(|_| mapping("资金查询游标无效"))?)
            .map_err(|_| mapping("资金查询游标无效"))?;
        let cursor: FundsCursor = serde_json::from_slice(&bytes).map_err(|_| mapping("资金查询游标无效"))?;
        if cursor.binding != self.binding
            || cursor.since != query.since.unix_secs()
            || cursor.through != query.through.unix_secs()
            || cursor.limit != query.limit.get()
            || cursor.page < 1
        {
            return Err(mapping("资金查询游标与账号、窗口或页大小不一致"));
        }
        Ok(cursor.page)
    }

    fn next_funds_page(&self, query: &SupplierFundsQuery, page: u32) -> ConnectorResult<String> {
        let cursor = FundsCursor {
            binding: self.binding.clone(),
            since: query.since.unix_secs(),
            through: query.through.unix_secs(),
            limit: query.limit.get(),
            page,
        };
        let bytes = serde_json::to_vec(&cursor).map_err(|_| mapping("资金查询检查点编码失败"))?;
        let mut mac = Hmac::<Sha256>::new_from_slice(self.cursor_secret.as_bytes())
            .map_err(|_| mapping("资金查询检查点编码失败"))?;
        mac.update(&bytes);
        Ok(format!("{}.{}", URL_SAFE_NO_PAD.encode(&bytes), hex::encode(mac.finalize().into_bytes())))
    }
}

#[async_trait]
impl SupplierFundsSource for JdVopConnector {
    fn account_reference(&self) -> &str {
        self.profile
            .as_ref()
            .map(|profile| profile.verified_external_funds_account_ref.as_str())
            .unwrap_or_default()
    }

    fn message_effect(&self, message_type: &str) -> Option<SupplierFundsDirection> {
        if matches!(message_type, "10" | "14" | "15" | "18") {
            return None;
        }
        self.funds_profile().ok()?.message_effects.get(message_type).copied()
    }

    async fn balance(&self) -> ConnectorResult<Snapshot<SupplierFundsBalance>> {
        let profile = self.funds_profile()?;
        let raw: Box<RawValue> = self
            .reader
            .as_ref()
            .ok_or_else(protocol_required)?
            .read(JdVopReadRequest::FundsBalance {
                source_type: self.profile.as_ref().ok_or_else(protocol_required)?.source_type,
            })
            .await?;
        let evidence = self.funds_evidence(&raw)?;
        Ok(observed(SupplierFundsBalance {
            account_reference: profile.verified_account_reference.clone(),
            amount: balance_amount(&raw)?,
            currency: profile.currency,
            evidence,
        }))
    }

    async fn transactions(
        &self,
        query: &SupplierFundsQuery,
    ) -> ConnectorResult<Page<Snapshot<SupplierFundsObservation>>> {
        let profile = self.funds_profile()?;
        let page = self.funds_page(query)?;
        let zone = FixedOffset::east_opt(profile.timezone_offset_seconds)
            .ok_or_else(|| mapping("资金账户时间口径无效"))?;
        let response: BalanceChanges = self
            .reader
            .as_ref()
            .ok_or_else(protocol_required)?
            .read(JdVopReadRequest::BalanceChanges {
                source_type: self.profile.as_ref().ok_or_else(protocol_required)?.source_type,
                start_date: query.since.as_utc().with_timezone(&zone).format("%Y-%m-%d %H:%M:%S").to_string(),
                end_date: query.through.as_utc().with_timezone(&zone).format("%Y-%m-%d %H:%M:%S").to_string(),
                page_size: query.limit,
                page_index: page,
            })
            .await?;
        let more = response.validate(page, query.limit)?;
        let mut items = Vec::with_capacity(response.items.len());
        for raw in response.items {
            let evidence = self.funds_evidence(&raw)?;
            items.push(observed(observation(&raw, profile, evidence, query)));
        }
        let next = if more {
            Some(
                self.next_funds_page(query, page.checked_add(1).ok_or_else(|| mapping("资金页码超出范围"))?)?,
            )
        } else {
            None
        };
        Ok(Page { items, next })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, VecDeque};
    use std::sync::{Arc, Mutex};

    use erp_core::common::time::Instant;
    use erp_core::ids::SupplierApiConnectionId;
    use erp_party::SensitiveDataCodec;
    use erp_supply::ports::connector::funds::SupplierFundsObservationResult;
    use serde_json::json;

    use super::super::transport::{JdVopReadTransport, ReadClient};
    use super::super::wire_funds::tests::{profile, query};
    use super::super::{
        JdVopConfirmationFundsPolicy, JdVopExecutionProfile, JdVopInvoiceProfile, JdVopOfferMapper,
    };
    use super::*;

    type Response = (JdVopReadRequest, ConnectorResult<Vec<u8>>);
    struct Transport {
        responses: Mutex<VecDeque<Response>>,
        requests: Mutex<Vec<JdVopReadRequest>>,
    }

    #[async_trait]
    impl JdVopReadTransport for Transport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            self.requests.lock().unwrap().push(request.clone());
            let (expected, result) = self.responses.lock().unwrap().pop_front().expect("固定资金查询替身");
            assert_eq!(&expected, request);
            result
        }
    }

    fn response(request: JdVopReadRequest, result: serde_json::Value) -> Response {
        let key = format!("{}_responce", request.method().replace('.', "_"));
        let body = serde_json::to_vec(
            &json!({key:{"openRpcResult":{"success":true,"resultCode":"0000","result":result}}}),
        )
        .unwrap();
        (request, Ok(body))
    }

    fn request(page: u32) -> JdVopReadRequest {
        JdVopReadRequest::BalanceChanges {
            source_type: 3,
            start_date: "1970-01-01 08:00:00".into(),
            end_date: "1970-01-01 09:00:00".into(),
            page_size: NonZeroU32::new(1).unwrap(),
            page_index: page,
        }
    }

    fn execution_profile() -> JdVopExecutionProfile {
        JdVopExecutionProfile {
            source_type: 3,
            evidence_reference: "verified-account".into(),
            invoice: JdVopInvoiceProfile {
                invoice_type: 23,
                invoice_put_type: 2,
                invoice_selected_title: 4,
                invoice_content_type: 1,
                invoice_company_name: "测试公司".into(),
                invoice_reg_code: None,
                invoice_reg_company_name: None,
                invoice_reg_address: None,
                invoice_reg_phone: None,
                invoice_reg_bank: None,
                invoice_reg_bank_account: None,
                recipient: None,
            },
            verified_external_funds_account_ref: "account-ref".into(),
            confirmation_funds_policy: Some(JdVopConfirmationFundsPolicy::OriginalOrderReservation {
                evidence_reference: "verified-original-order-funding".into(),
            }),
            preflight_validity_seconds: 60,
            ordinary_skus: BTreeSet::from(["123".into()]),
            verified_message_types: None,
            verified_funds_profile: Some(profile()),
        }
    }

    fn connector(responses: Vec<Response>) -> (JdVopConnector, Arc<Transport>, Arc<SensitiveDataCodec>) {
        let transport =
            Arc::new(Transport { responses: Mutex::new(responses.into()), requests: Mutex::new(vec![]) });
        let codec = Arc::new(SensitiveDataCodec::from_secret(b"funds-fixture-key"));
        (
            JdVopConnector {
                connection_id: SupplierApiConnectionId::new("connection"),
                account_scope: "scope".into(),
                binding: "binding".into(),
                cursor_secret: "cursor-secret".into(),
                reader: Some(Arc::new(ReadClient::new(transport.clone(), 2, 1000, 5))),
                mapper: JdVopOfferMapper::default(),
                writer: None,
                profile: Some(Arc::new(execution_profile())),
                evidence_codec: Some(codec.clone()),
                message_runtime: None,
            },
            transport,
            codec,
        )
    }

    fn page(index: u32, code: &str) -> serde_json::Value {
        json!({"pageSize":1,"pageIndex":index,"pageItemTotal":2,"pageTotal":2,"items":[{
            "tradeNo":format!("900719925474099{index}"),"jdOrderId":"9007199254740995","amount":"-21.9300",
            "tradeType":code,"accountType":"1","pin":"verified-pin","createdDate":"1970-01-01 08:30:00"}]})
    }

    #[tokio::test]
    async fn actual_read_payload_keeps_window_timezone_pages_and_encrypted_evidence() {
        let (connector, transport, codec) = connector(vec![
            response(
                JdVopReadRequest::FundsBalance { source_type: 3 },
                json!({"balanceInfo":{"remainLimit":"100.00"}}),
            ),
            response(request(1), page(1, "1209")),
            response(request(2), page(2, "9999")),
        ]);
        let balance = SupplierFundsSource::balance(&connector).await.unwrap();
        assert_eq!(balance.value.account_reference, "account-ref");
        assert_eq!(balance.value.amount, "100.00".parse().unwrap());
        assert!(codec.decrypt(&balance.value.evidence.body_encrypted).unwrap().contains("remainLimit"));
        assert_eq!(serde_json::to_value(&balance.stamp.revision).unwrap(), "Unversioned");
        assert!(balance.stamp.changed_at.is_none());
        let mut query = query();
        let first = connector.transactions(&query).await.unwrap();
        assert!(matches!(first.items[0].value.result, SupplierFundsObservationResult::Transaction(_)));
        let cursor = first.next.unwrap();
        query.cursor = Some(cursor.clone());
        let second = connector.transactions(&query).await.unwrap();
        assert!(second.next.is_none());
        assert!(matches!(second.items[0].value.result, SupplierFundsObservationResult::Unresolved(_)));
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
        query.through = Instant::from_unix_secs(3601);
        assert!(connector.transactions(&query).await.is_err());
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn failed_page_and_unverified_profile_send_no_extra_requests_or_fake_balance() {
        let (mut connector, transport, _) = connector(vec![(request(1), Err(mapping("upstream failed")))]);
        assert!(connector.transactions(&query()).await.is_err());
        assert_eq!(transport.requests.lock().unwrap().len(), 1);
        connector.profile = None;
        assert!(SupplierFundsSource::balance(&connector).await.is_err());
        assert!(connector.transactions(&query()).await.is_err());
        assert_eq!(transport.requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn cursor_is_bound_to_account_window_size_and_signature_and_failure_messages_do_not_prove_money() {
        let (mut connector, _, _) = connector(vec![]);
        let query = query();
        let cursor = connector.next_funds_page(&query, 2).unwrap();
        let mut resumed = query.clone();
        resumed.cursor = Some(cursor.clone());
        assert_eq!(connector.funds_page(&resumed).unwrap(), 2);
        resumed.cursor = Some(format!("{cursor}00"));
        assert!(connector.funds_page(&resumed).is_err());
        resumed.cursor = Some(cursor);
        connector.binding = "another-account".into();
        assert!(connector.funds_page(&resumed).is_err());
        for kind in ["10", "14", "15", "18"] {
            assert_eq!(connector.message_effect(kind), None);
        }
        assert_eq!(connector.message_effect("unknown"), None);
    }
}
