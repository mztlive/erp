//! 京东四级地址及配送选择绑定；原资料恢复不执行地址转换。
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_supply::ports::connector::common::{
    ActionKey, ConnectorResult, Lookup, Snapshot, SourceRevision, SourceStamp,
};
use erp_supply::ports::connector::offer::{
    DeliveryContext, DeliveryGroup, DeliveryPlan, DeliveryQuery, DeliverySource,
};
use erp_supply::ports::connector::order::{
    CreateOrder, DeliveryChoice, DeliveryMethod, OrderLine, Recipient,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::order_payload::integer_quantity;
use super::transport::JdVopReadRequest;
use super::wire_order::Freight;
use super::{JdVopConnector, mapping, protocol_required};

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize, Deserialize)]
struct AddressProof {
    binding: String,
    action_id: String,
    payload_hash: String,
    recipient_hash: String,
    address_hash: String,
}

#[derive(Serialize, Deserialize)]
struct ChoiceProof {
    binding: String,
    context_hash: String,
    lines_hash: String,
    shipping_fee: Amount,
    expires_at: i64,
}

#[async_trait]
impl DeliverySource for JdVopConnector {
    async fn prepare(&self, key: &ActionKey, recipient: &Recipient) -> ConnectorResult<DeliveryContext> {
        self.local_context(key, recipient)
    }

    async fn context(&self, _: &ActionKey) -> ConnectorResult<Lookup<DeliveryContext>> {
        Err(mapping("请使用原加密执行快照恢复京东收货资料"))
    }

    async fn context_with_recipient(
        &self,
        key: &ActionKey,
        recipient: &Recipient,
    ) -> ConnectorResult<Lookup<DeliveryContext>> {
        self.local_context(key, recipient).map(Lookup::Found)
    }

    async fn options(&self, query: &DeliveryQuery) -> ConnectorResult<Snapshot<DeliveryPlan>> {
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        self.validate_context(&query.context)?;
        let shipping = self.freight(&query.context, &query.lines).await?;

        let expires_at = Instant::now()
            .unix_secs()
            .checked_add(
                i64::try_from(profile.preflight_validity_seconds)
                    .map_err(|_| mapping("京东预检有效期超出范围"))?,
            )
            .ok_or_else(|| mapping("京东预检有效期溢出"))?;
        let choice = ChoiceProof {
            binding: self.binding.clone(),
            context_hash: self.hash(&query.context)?,
            lines_hash: self.hash(&query.lines)?,
            shipping_fee: shipping,
            expires_at,
        };
        let reference = self.encode(&choice)?;
        Ok(observed(DeliveryPlan {
            groups: vec![DeliveryGroup {
                line_ids: query.lines.iter().map(|line| line.line_id.clone()).collect(),
                choices: vec![DeliveryChoice {
                    method: DeliveryMethod::Courier,
                    shipping_fee: shipping,
                    reference,
                }],
            }],
            valid_until: None,
        }))
    }
}

impl JdVopConnector {
    fn local_context(&self, key: &ActionKey, recipient: &Recipient) -> ConnectorResult<DeliveryContext> {
        if key.id.is_empty() || key.payload_hash.is_empty() {
            return Err(mapping("京东原地址准备身份缺失"));
        }
        let address =
            recipient.supplier_address.as_ref().ok_or_else(|| mapping("请先确认京东四级收货地址"))?;
        address
            .validate_binding(
                self.connection_id.as_ref(),
                &address.snapshot_reference,
                &address.address_fingerprint,
            )
            .map_err(|_| mapping("京东原地址绑定不完整"))?;

        let proof = AddressProof {
            binding: self.binding.clone(),
            action_id: key.id.clone(),
            payload_hash: key.payload_hash.clone(),
            recipient_hash: self.hash(recipient)?,
            address_hash: self.hash(address)?,
        };
        Ok(DeliveryContext { reference: self.encode(&proof)?, supplier_address: Some(address.clone()) })
    }

    pub(super) fn validate_context(&self, context: &DeliveryContext) -> ConnectorResult<()> {
        let proof: AddressProof = self.decode(&context.reference)?;
        let address =
            context.supplier_address.as_ref().ok_or_else(|| mapping("京东地址上下文缺少原四级资料"))?;
        if proof.binding != self.binding
            || address.connection_id != self.connection_id.as_ref()
            || proof.address_hash != self.hash(address)?
        {
            return Err(mapping("京东收货地址或账号已变化，请重新核验"));
        }
        Ok(())
    }

    pub(super) fn validate_choice(&self, request: &CreateOrder) -> ConnectorResult<()> {
        self.validate_context(&request.delivery_context)?;
        let address: AddressProof = self.decode(&request.delivery_context.reference)?;
        let choice: ChoiceProof = self.decode(&request.delivery.reference)?;
        if choice.binding != self.binding
            || choice.expires_at <= Instant::now().unix_secs()
            || choice.context_hash != self.hash(&request.delivery_context)?
            || choice.lines_hash != self.hash(&request.lines)?
            || choice.shipping_fee != request.delivery.shipping_fee
            || address.recipient_hash != self.hash(&request.recipient)?
        {
            return Err(mapping("京东配送选项与原收件资料、数量或批准费用不符，请重新预检"));
        }
        Ok(())
    }

    pub(super) async fn freight(
        &self,
        context: &DeliveryContext,
        lines: &[OrderLine],
    ) -> ConnectorResult<Amount> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let lines = line_quantities(lines)?;
        let area = area(context)?;
        let freight: Freight =
            reader.read(JdVopReadRequest::Freight { lines, area, payment_type: 4 }).await?;
        let total: Amount = freight.total_freight.0.parse().map_err(|_| mapping("京东运费金额或精度无效"))?;
        if total < Amount::zero() {
            return Err(mapping("京东运费不能为负数"));
        }
        Ok(total)
    }

    pub(super) fn hash<T: Serialize + ?Sized>(&self, value: &T) -> ConnectorResult<String> {
        let body = serde_json::to_vec(value).map_err(|_| mapping("京东协议资料摘要失败"))?;
        let mut mac = self.mac();
        mac.update(&body);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    fn encode<T: Serialize>(&self, value: &T) -> ConnectorResult<String> {
        let body = serde_json::to_vec(value).map_err(|_| mapping("京东协议资料编码失败"))?;
        let mut mac = self.mac();
        mac.update(&body);
        Ok(format!("{}.{}", hex::encode(&body), hex::encode(mac.finalize().into_bytes())))
    }

    fn decode<T: for<'de> Deserialize<'de>>(&self, value: &str) -> ConnectorResult<T> {
        if value.len() > 32768 {
            return Err(mapping("京东协议上下文过大"));
        }
        let (body, signature) = value.split_once('.').ok_or_else(|| mapping("京东协议上下文无效"))?;
        let body = hex::decode(body).map_err(|_| mapping("京东协议上下文无效"))?;
        let signature = hex::decode(signature).map_err(|_| mapping("京东协议上下文无效"))?;

        let mut mac = self.mac();
        mac.update(&body);
        mac.verify_slice(&signature).map_err(|_| mapping("京东协议上下文已失效"))?;
        serde_json::from_slice(&body).map_err(|_| mapping("京东协议上下文格式无效"))
    }

    fn mac(&self) -> HmacSha256 {
        let mut mac =
            HmacSha256::new_from_slice(self.cursor_secret.as_bytes()).expect("HMAC接受任意长度密钥");
        mac.update(b"erp-jd-vop-execution-proof-v1\0");
        mac
    }
}

pub(super) fn line_quantities(lines: &[OrderLine]) -> ConnectorResult<Vec<(String, u32)>> {
    if lines.is_empty() || lines.len() > 100 {
        return Err(mapping("京东预检明细须为1至100行"));
    }
    lines.iter().map(|line| Ok((line.sku.spec_id.clone(), integer_quantity(line)?))).collect()
}

pub(super) fn area(context: &DeliveryContext) -> ConnectorResult<[String; 4]> {
    let address = context.supplier_address.as_ref().ok_or_else(|| mapping("京东四级地址未确认"))?;
    Ok([
        address.province.code.clone(),
        address.city.code.clone(),
        address.county.code.clone(),
        address.town.code.clone(),
    ])
}

pub(super) fn observed<T>(value: T) -> Snapshot<T> {
    Snapshot {
        stamp: SourceStamp {
            revision: SourceRevision::Unversioned,
            changed_at: None,
            observed_at: Instant::now(),
        },
        value,
    }
}
