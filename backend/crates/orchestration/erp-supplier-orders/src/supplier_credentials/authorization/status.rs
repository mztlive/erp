use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{SupplierCredential, SupplierCredentialRefreshState};

use super::super::crypto::CredentialCrypto;
use super::dto::JdVopAuthorizationStatus;

pub(super) fn projection(
    record: &SupplierCredential,
    crypto: &CredentialCrypto,
    now: Instant,
) -> JdVopAuthorizationStatus {
    let deadline = record.refresh.deadline().map(Instant::unix_secs);
    let Some((state, operation, can_authorize, message)) = phase(record, now) else {
        return ready(record, crypto, now);
    };

    let mut status = JdVopAuthorizationStatus::empty(state, can_authorize, message);
    status.version = Some(record.base.version);
    status.expires_at = (!record.encrypted_payload.is_empty()).then(|| record.expires_at.unix_secs());
    status.recovery_after = deadline;
    if let Some((id, phase)) = operation {
        status.operation_id = Some(id);
        status.operation_state = Some(phase);
    }
    status
}

type Phase = (&'static str, Option<(String, &'static str)>, bool, &'static str);

fn phase(record: &SupplierCredential, now: Instant) -> Option<Phase> {
    use SupplierCredentialRefreshState as S;
    let identity = record.account_identity_fingerprint.is_some()
        || record.authorization_candidate_identity_fingerprint.is_some();
    Some(match &record.refresh {
        S::AuthorizationClaimed { operation_id, lease_until, .. } => (
            "authorizing",
            Some((operation_id.clone(), "claimed")),
            *lease_until <= now,
            "授权正在处理，可以关闭窗口，稍后回来查询结果",
        ),
        S::AuthorizationSentUnknown { operation_id, lease_until, .. } if *lease_until > now => (
            "authorizing",
            Some((operation_id.clone(), "sent_unknown")),
            false,
            "授权正在处理，请稍后查询，不要重复提交",
        ),
        S::AuthorizationSentUnknown { operation_id, .. } => (
            "unknown",
            Some((operation_id.clone(), "sent_unknown")),
            identity,
            "原授权结果待核实；请使用同一京东账号重新授权，并提供本次授权依据",
        ),
        S::AuthorizationRejected { operation_id, .. } => (
            "rejected",
            Some((operation_id.clone(), "rejected")),
            identity || record.encrypted_payload.is_empty(),
            "京东未接受本次登录资料，请核对采购账号、密码、应用回调地址和私钥文件后重新授权",
        ),
        S::Claimed { lease_until, .. } => {
            ("refreshing", None, identity && *lease_until <= now, "京东授权正在续期，请稍后查询")
        },
        S::SentUnknown { lease_until, .. } => (
            "unknown",
            None,
            identity && *lease_until <= now,
            "原续期结果待核实；发送期限结束后，可使用同一账号重新授权",
        ),
        S::ReauthorizationRequired { .. } => {
            ("reauthorization_required", None, identity, "京东授权已失效，请使用原采购账号重新授权")
        },
        S::Ready => return None,
    })
}

pub(super) fn operation_projection(
    record: &SupplierCredential,
    crypto: &CredentialCrypto,
    now: Instant,
    operation_id: Option<&str>,
) -> JdVopAuthorizationStatus {
    let mut status = projection(record, crypto, now);
    let Some(id) = operation_id else {
        return status;
    };
    if record.configuration_history.iter().any(|event| event.operation_id == id) {
        status.operation_id = Some(id.to_string());
        status.operation_state = Some("completed");
        return status;
    }

    let event = record.authorization_history.iter().find(|event| event.operation_id == id);
    status.operation_id = event.map(|event| event.operation_id.clone());
    status.operation_state = event.map(|event| {
        if event.completed_at.is_some() {
            "completed"
        } else if event.rejected_at.is_some() {
            "rejected"
        } else if record.authorization_history.last().is_some_and(|latest| latest.operation_id != id)
            || record.configuration_history.last().is_some_and(|latest| {
                latest.previous_version.is_some_and(|version| version > event.previous_version.unwrap_or(0))
            })
        {
            "superseded"
        } else if event.sent_at.is_some() {
            "sent_unknown"
        } else {
            "claimed"
        }
    });
    status
}

fn ready(record: &SupplierCredential, crypto: &CredentialCrypto, now: Instant) -> JdVopAuthorizationStatus {
    let mut status = if crypto.open(record).is_err() {
        JdVopAuthorizationStatus::empty(
            "unreadable",
            false,
            "已保存授权暂时无法读取，请联系管理员核对凭据配置",
        )
    } else if record.manually_configured {
        let can_authorize = record.account_identity_fingerprint.is_some();
        if record.expires_at <= now {
            JdVopAuthorizationStatus::empty(
                "expired",
                can_authorize,
                "已配置令牌已到期，请配置新取得的有效令牌",
            )
        } else {
            JdVopAuthorizationStatus::empty(
                "configured",
                can_authorize,
                "令牌已配置，接口调用和自动续期使用此记录；请测试连接核验是否可用",
            )
        }
    } else if record.account_identity_fingerprint.is_none() {
        JdVopAuthorizationStatus::empty(
            "identity_unverified",
            false,
            "已有令牌尚未核实京东账号身份，请联系管理员处理，不能直接更换账号",
        )
    } else if record.expires_at <= now {
        JdVopAuthorizationStatus::empty("expired", true, "京东授权已过期，请使用原采购账号重新授权")
    } else {
        JdVopAuthorizationStatus::empty("authorized", true, "授权已保存；请执行连接检查核验京东接口是否可用")
    };

    status.version = Some(record.base.version);
    status.expires_at = Some(record.expires_at.unix_secs());
    status.operation_id = record.last_completed_authorization_operation_id.clone();
    status.operation_state = status.operation_id.as_ref().map(|_| "completed");
    status
}
