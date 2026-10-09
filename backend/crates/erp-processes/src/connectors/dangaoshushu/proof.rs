//! 封装连接、地址和配送选项绑定；令牌不含密钥或待执行 HTTP 请求。
use config::DangaoshushuConfig;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::order::{CoordinateSystem, DeliveryMethod, OrderLine, Recipient};
use erp_supply::ports::supplier_reference_registry::SupplierReferenceTarget;
use hmac::{Hmac, KeyInit, Mac};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, json, to_vec};
use sha2::Sha256;

use super::parsing::mapping;

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize, Deserialize)]
pub(super) struct AddressProof {
    pub connection_id: String,
    pub configuration_hash: String,
    pub action_id: String,
    pub payload_hash: String,
    pub user_id: String,
    pub addr_id: String,
    pub city_id: String,
    pub recipient_hash: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ChoiceProof {
    pub connection_id: String,
    pub configuration_hash: String,
    pub context_hash: String,
    pub lines_hash: String,
    pub rule_id: String,
    pub method: String,
    pub shipping_fee: String,
    pub ship_time_text: String,
    pub shop_name: Option<String>,
    pub shop_detail: Option<String>,
    pub expires_at: i64,
}

pub(super) fn configuration_hash(settings: &DangaoshushuConfig) -> ConnectorResult<String> {
    let bytes = to_vec(&json!({
        "base_url":settings.base_url,
        "channel":settings.channel_no,"user":settings.user_id,
        "timestamp_unit":format!("{:?}",settings.timestamp_unit),
        "timeout":settings.timeout_seconds,"rate":settings.requests_per_second,
        "callback_skew":settings.callback_max_skew_seconds,
        "price_basis":settings.clearing_price_is_tax_inclusive_cny,
        "units":settings.spec_units,"regions":settings.city_regions
    }))
    .map_err(|_| mapping())?;
    Ok(hash(&settings.private_key, &bytes))
}

pub(super) fn binding_hash(
    settings: &DangaoshushuConfig,
    target: &SupplierReferenceTarget,
) -> ConnectorResult<String> {
    let bytes = to_vec(&json!({
        "configuration":configuration_hash(settings)?,
        "connection":target.connection_id,"supplier":target.supplier_id,
        "environment":target.environment.as_str()
    }))
    .map_err(|_| mapping())?;
    Ok(hash(&settings.private_key, &bytes))
}

pub(super) fn hash(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC 接受任意长度密钥");
    mac.update(b"erp-dangaoshushu-proof-v1\0");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}
pub(super) fn encode<T: Serialize>(secret: &str, value: &T) -> ConnectorResult<String> {
    let body = to_vec(value).map_err(|_| mapping())?;
    Ok(format!("{}.{}", hex::encode(&body), hash(secret, &body)))
}
pub(super) fn decode<T: DeserializeOwned>(secret: &str, value: &str) -> ConnectorResult<T> {
    if value.len() > 32768 {
        return Err(mapping());
    }
    let (body, signature) = value.split_once('.').ok_or_else(mapping)?;
    let body = hex::decode(body).map_err(|_| mapping())?;
    let signature = hex::decode(signature).map_err(|_| mapping())?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC 接受任意长度密钥");
    mac.update(b"erp-dangaoshushu-proof-v1\0");
    mac.update(&body);
    mac.verify_slice(&signature).map_err(|_| mapping())?;
    from_slice(&body).map_err(|_| mapping())
}
pub(super) fn recipient_hash(secret: &str, recipient: &Recipient) -> ConnectorResult<String> {
    let geo = recipient.geocoded_address.as_ref().ok_or_else(mapping)?;
    if geo.coordinate_system != CoordinateSystem::Bd09 {
        return Err(mapping());
    }
    let bytes = to_vec(&[
        &recipient.name,
        &recipient.phone,
        &recipient.region,
        &recipient.address,
        &geo.city_name,
        &geo.district,
        &geo.latitude,
        &geo.longitude,
    ])
    .map_err(|_| mapping())?;
    Ok(hash(secret, &bytes))
}
pub(super) fn lines_hash(secret: &str, lines: &[OrderLine]) -> ConnectorResult<String> {
    let values: Vec<_> = lines.iter().map(|line| json!({"line_id":line.line_id,"product_id":line.sku.product_id,"spec_id":line.sku.spec_id,"quantity":line.quantity.to_string(),"price":line.approved_unit_price.to_string(),"attributes":line.attributes.iter().map(|attr| (&attr.name,&attr.value)).collect::<Vec<_>>() })).collect();
    Ok(hash(secret, &to_vec(&values).map_err(|_| mapping())?))
}
pub(super) fn method_key(method: &DeliveryMethod) -> String {
    match method {
        DeliveryMethod::Courier => "same".into(),
        DeliveryMethod::LocalDelivery { date, slot } => format!("delivery:{date}:{slot}"),
        DeliveryMethod::Pickup { store_id, date, slot } => format!("shop:{store_id}:{date}:{slot}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proof_rejects_tampering_and_other_connection_secrets() {
        let token = encode("key", &json!({"id":"address-1"})).unwrap();
        assert_eq!(decode::<serde_json::Value>("key", &token).unwrap()["id"], "address-1");
        assert!(decode::<serde_json::Value>("other", &token).is_err());
        assert!(decode::<serde_json::Value>("key", &("ff".to_string() + token.as_str())).is_err());
    }
}
