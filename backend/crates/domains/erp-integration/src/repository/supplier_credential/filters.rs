use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::bson::{Document, doc, serialize_to_document};

use crate::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialAuthorizationRequest, SupplierCredentialBinding,
    SupplierCredentialRefreshClaim, SupplierCredentialRefreshState, identity,
};
use crate::{Error, Result};

pub(super) fn id(value: &str) -> Document {
    doc! {"id":value,"deleted_at":NOT_DELETED_TIMESTAMP_BSON}
}

pub(super) fn binding(value: &str, binding: &SupplierCredentialBinding) -> Result<Document> {
    identity(value, 128)?;
    binding.validate()?;
    let mut filter = id(value);
    filter
        .extend(doc! {"provider":"jd_vop","account_scope":&binding.account_scope,"app_key":&binding.app_key});
    Ok(filter)
}

pub(super) fn version(current: &SupplierCredential, expected: u64) -> Result<Document> {
    let version = i64::try_from(expected)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::ValidationError("技术凭据版本超出存储边界".into()))?;
    let mut filter = binding(&current.base.id, &current.binding())?;
    filter.insert("version", version);
    Ok(filter)
}

pub(super) fn ordinary_import(current: &SupplierCredential, expected: u64, now: Instant) -> Result<Document> {
    let mut filter = version(current, expected)?;
    filter.insert(
        "$or",
        vec![
            doc! {"refresh.state":"READY"},
            doc! {"refresh.state":"CLAIMED","refresh.lease_until":{"$lte":now.unix_secs()}},
        ],
    );
    Ok(filter)
}

pub(super) fn claim(
    current: &SupplierCredential,
    expected: u64,
    command: &SupplierCredentialRefreshClaim,
) -> Result<Document> {
    let mut filter = ordinary_import(current, expected, command.now)?;
    filter.insert("last_completed_refresh_operation_id", doc! {"$ne":&command.operation_id});
    Ok(filter)
}

pub(super) fn claimed(current: &SupplierCredential, now: Instant) -> Result<Document> {
    let SupplierCredentialRefreshState::Claimed { owner_id, operation_id, .. } = &current.refresh else {
        return Err(Error::ConflictError("技术凭据没有原领取身份".into()));
    };
    let mut filter = version(current, current.base.version)?;
    filter.extend(doc! {"refresh.state":"CLAIMED","refresh.owner_id":owner_id,
    "refresh.operation_id":operation_id,"refresh.lease_until":{"$gt":now.unix_secs()}});
    Ok(filter)
}

pub(super) fn sent(current: &SupplierCredential) -> Result<Document> {
    let SupplierCredentialRefreshState::SentUnknown { owner_id, operation_id, sent_at, .. } =
        &current.refresh
    else {
        return Err(Error::ConflictError("技术凭据没有原发送身份".into()));
    };
    let mut filter = version(current, current.base.version)?;
    filter.extend(doc! {"refresh.state":"SENT_UNKNOWN","refresh.owner_id":owner_id,
    "refresh.operation_id":operation_id,"refresh.sent_at":sent_at.unix_secs()});
    Ok(filter)
}

pub(super) fn reauthorization(current: &SupplierCredential, expected: u64, now: Instant) -> Result<Document> {
    let mut filter = version(current, expected)?;
    filter.insert(
        "$or",
        vec![
            doc! {"refresh.state":"SENT_UNKNOWN","refresh.lease_until":{"$lte":now.unix_secs()}},
            doc! {"refresh.state":"REAUTHORIZATION_REQUIRED"},
            doc! {"refresh.state":{"$in":["READY","CLAIMED"]},"expires_at":{"$lte":now.unix_secs()}},
        ],
    );
    Ok(filter)
}

pub(super) fn authorization_claim(
    current: &SupplierCredential,
    request: &SupplierCredentialAuthorizationRequest,
) -> Result<Document> {
    let expected =
        request.expected_version.ok_or_else(|| Error::ConflictError("原授权缺少所见版本".into()))?;
    let mut filter = version(current, expected)?;
    filter.insert("authorization_history.operation_id", doc! {"$ne":&request.claim.operation_id});
    filter.insert("account_identity_fingerprint", current.account_identity_fingerprint.clone());
    filter.insert(
        "authorization_candidate_identity_fingerprint",
        current.authorization_candidate_identity_fingerprint.clone(),
    );

    filter.insert("$or", vec![
        doc! {"refresh.state":{"$in":["READY","REAUTHORIZATION_REQUIRED","AUTHORIZATION_REJECTED"]}},
        doc! {"refresh.state":{"$in":["CLAIMED","SENT_UNKNOWN","AUTHORIZATION_CLAIMED","AUTHORIZATION_SENT_UNKNOWN"]},
            "refresh.lease_until":{"$lte":request.claim.now.unix_secs()}},
    ]);
    Ok(filter)
}

pub(super) fn authorization_claimed(current: &SupplierCredential, now: Instant) -> Result<Document> {
    let SupplierCredentialRefreshState::AuthorizationClaimed { owner_id, operation_id, .. } =
        &current.refresh
    else {
        return Err(Error::ConflictError("原授权领取身份缺失".into()));
    };
    let mut filter = version(current, current.base.version)?;
    filter.extend(doc! {"refresh.state":"AUTHORIZATION_CLAIMED","refresh.owner_id":owner_id,
    "refresh.operation_id":operation_id,"refresh.lease_until":{"$gt":now.unix_secs()}});
    Ok(filter)
}

pub(super) fn authorization_sent(current: &SupplierCredential) -> Result<Document> {
    let SupplierCredentialRefreshState::AuthorizationSentUnknown {
        owner_id,
        operation_id,
        sent_at,
        exchange_sent_at,
        ..
    } = &current.refresh
    else {
        return Err(Error::ConflictError("原授权发送身份缺失".into()));
    };
    let mut filter = version(current, current.base.version)?;
    filter.extend(doc! {"refresh.state":"AUTHORIZATION_SENT_UNKNOWN","refresh.owner_id":owner_id,
    "refresh.operation_id":operation_id,"refresh.sent_at":sent_at.unix_secs()});
    filter.insert("refresh.exchange_sent_at", exchange_sent_at.map(Instant::unix_secs));
    Ok(filter)
}

pub(super) fn authorization_exchange(current: &SupplierCredential, now: Instant) -> Result<Document> {
    let mut filter = authorization_sent(current)?;
    filter.insert("refresh.lease_until", doc! {"$gt":now.unix_secs()});
    Ok(filter)
}

pub(super) fn replacement(next: &SupplierCredential) -> Result<Vec<Document>> {
    let record =
        serialize_to_document(next).map_err(|_| Error::Internal("技术凭据无法编码为存储格式".into()))?;

    let mut fields = Document::new();
    for (name, value) in record {
        // 所有服务端字段均按常量写入；密文或操作文本不能被解释为Mongo表达式。
        fields.insert(name, doc! {"$literal":value});
    }
    Ok(vec![doc! {"$set":fields}])
}

#[cfg(test)]
mod tests;
