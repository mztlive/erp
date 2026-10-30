// 技术凭据只读核对。执行：
// ERP_JD_CREDENTIAL_ID=<稳定引用> mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261030-supplier-credentials-audit.mongosh.js
// 必须限定一个已批准的credential_id；禁止默认遍历全部技术账号。
// 本脚本不创建集合/索引，不导入、刷新、解密、覆盖、删除凭据，不打印密文、账号、应用或令牌摘要。
// 正式导入须先由受控命令创建本集合唯一索引，再以expected_version更新原身份。
// 核对前必须已具备本集合唯一索引；缺失时停止，禁止降级为无索引扫描。
// SENT_UNKNOWN不得按到期自动刷新；只允许在原发送期限后以明确新授权和原版本恢复同一引用。
// 新增账号身份及授权历史字段缺失时按未核实旧记录处理；不得由页面填写账号补成已确认身份。
// 首次占位仅AUTHORIZATION_*阶段允许空密文；候选身份只有兑换前明确拒绝才能解除。
// authorizeForVOP与令牌兑换均须先保存发送边界；任何未知及历史同操作不得自动再发。
// 产生授权状态或身份历史后，禁止旧binary继续读取和修改凭据；回滚必须保留兼容状态机的binary并停旧worker。
// manually_configured和configuration_history缺失时使用false/空历史；无需写入回填。
// 页面令牌配置后，回滚版本必须保留配置标识和历史回执；不得回退到会丢失这些字段的写入版本。
// 回退必须保留原引用、未知刷新、版本及重新授权历史；不得通过删除记录或更换引用规避未知结果。

const credentialAuditId = process.env.ERP_JD_CREDENTIAL_ID?.trim();
if (!credentialAuditId || credentialAuditId.length > 128) {
    throw new Error("必须提供有效的 ERP_JD_CREDENTIAL_ID，禁止全表核对");
}
if (process.env.ERP_MIGRATION_APPLY === "1") {
    throw new Error("本入口始终只读；凭据写入必须使用受控导入或重新授权命令");
}

function credentialAuditInteger(value) {
    if (typeof value === "number") {
        return Number.isSafeInteger(value) ? BigInt(value) : null;
    }
    const text = value?.toString();
    return text && /^\d+$/.test(text) ? BigInt(text) : null;
}

function credentialAuditNonempty(value, maximum) {
    return typeof value === "string" && value === value.trim()
        && value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f]/.test(value);
}

function credentialAuditFingerprint(value) {
    return typeof value === "string" && /^[a-fA-F0-9]{64}$/.test(value);
}

function credentialAuditAuthorizationState(value) {
    return typeof value === "string" && value.startsWith("AUTHORIZATION_");
}

function credentialAuditAuthorization(row, failures) {
    const confirmed = row.account_identity_fingerprint ?? null;
    const candidate = row.authorization_candidate_identity_fingerprint ?? null;
    const uid = row.authorized_uid_fingerprint ?? null;
    for (const value of [confirmed, candidate, uid]) {
        if (value !== null && !credentialAuditFingerprint(value)) failures.push("ACCOUNT_IDENTITY_HMAC_INVALID");
    }
    if (uid !== null && confirmed === null) failures.push("UID_WITHOUT_CONFIRMED_IDENTITY");
    const history = row.authorization_history ?? [];
    if (!Array.isArray(history) || history.length > 256) {
        failures.push("AUTHORIZATION_HISTORY_INVALID");
        return {confirmed, candidate, history: []};
    }
    const operations = new Set();
    for (const event of history) {
        if (!credentialAuditNonempty(event?.operation_id, 128) || operations.has(event.operation_id)
            || !credentialAuditNonempty(event.owner_id, 128)
            || !credentialAuditNonempty(event.actor_id, 128)
            || !credentialAuditNonempty(event.evidence_reference, 1024)
            || !credentialAuditFingerprint(event.account_identity_fingerprint)
            || typeof event.new_authorization !== "boolean") failures.push("AUTHORIZATION_EVENT_INVALID");
        operations.add(event?.operation_id);
        const previous = event?.previous_version == null ? null : credentialAuditInteger(event.previous_version);
        if (event?.previous_version != null && (previous === null || previous < 1n)) failures.push("AUTHORIZATION_PREVIOUS_VERSION_INVALID");
        const claimed = credentialAuditInteger(event?.claimed_at);
        const sent = event?.sent_at == null ? null : credentialAuditInteger(event.sent_at);
        const exchange = event?.exchange_sent_at == null ? null : credentialAuditInteger(event.exchange_sent_at);
        const completed = event?.completed_at == null ? null : credentialAuditInteger(event.completed_at);
        const rejected = event?.rejected_at == null ? null : credentialAuditInteger(event.rejected_at);
        if (claimed === null || (event?.sent_at != null && (sent === null || sent < claimed))
            || (event?.exchange_sent_at != null && (exchange === null || sent === null || exchange < sent))
            || (event?.completed_at != null && (completed === null || exchange === null || completed < exchange))
            || (event?.rejected_at != null && (rejected === null || sent === null || rejected < sent || exchange !== null))
            || (completed !== null && rejected !== null)) failures.push("AUTHORIZATION_BOUNDARY_INVALID");
        if (completed !== null && (!credentialAuditFingerprint(event.payload_fingerprint)
            || event.account_identity_fingerprint !== confirmed)) failures.push("AUTHORIZATION_COMPLETION_INVALID");
        if (event?.authorized_uid_fingerprint != null && !credentialAuditFingerprint(event.authorized_uid_fingerprint)) {
            failures.push("AUTHORIZATION_UID_HMAC_INVALID");
        }
    }
    const completedOperation = row.last_completed_authorization_operation_id;
    if (completedOperation != null && !history.some((event) => event?.operation_id === completedOperation && event.completed_at != null)) {
        failures.push("AUTHORIZATION_RECEIPT_MISSING");
    }
    const state = row.refresh?.state;
    if (credentialAuditAuthorizationState(state)) {
        const event = history.find((entry) => entry?.operation_id === row.refresh.operation_id);
        if (!event) failures.push("CURRENT_AUTHORIZATION_EVENT_MISSING");
        else if (state === "AUTHORIZATION_REJECTED") {
            if (event.rejected_at == null || credentialAuditInteger(event.rejected_at) !== credentialAuditInteger(row.refresh.rejected_at)) {
                failures.push("AUTHORIZATION_REJECTION_MISSING");
            }
        } else if (event.owner_id !== row.refresh.owner_id || event.completed_at != null || event.rejected_at != null
            || event.account_identity_fingerprint !== candidate
            || (state === "AUTHORIZATION_CLAIMED" && event.sent_at != null)
            || (state === "AUTHORIZATION_SENT_UNKNOWN" && (event.sent_at == null
                || credentialAuditInteger(event.sent_at) !== credentialAuditInteger(row.refresh.sent_at)
                || (event.exchange_sent_at == null) !== (row.refresh.exchange_sent_at == null)
                || (event.exchange_sent_at != null && credentialAuditInteger(event.exchange_sent_at) !== credentialAuditInteger(row.refresh.exchange_sent_at))))) {
            failures.push("CURRENT_AUTHORIZATION_BOUNDARY_INVALID");
        }
    }
    return {confirmed, candidate, history};
}

function credentialAuditConfiguration(row, failures) {
    if (row.manually_configured != null && typeof row.manually_configured !== "boolean") {
        failures.push("MANUAL_CONFIGURATION_FLAG_INVALID");
    }
    const history = row.configuration_history ?? [];
    if (!Array.isArray(history) || history.length > 256) {
        failures.push("CONFIGURATION_HISTORY_INVALID");
        return 0;
    }
    const operations = new Set();
    const version = credentialAuditInteger(row.version);
    for (const event of history) {
        const previous = event?.previous_version == null ? null : credentialAuditInteger(event.previous_version);
        const occurred = credentialAuditInteger(event?.occurred_at);
        if (!credentialAuditNonempty(event?.operation_id, 128) || operations.has(event.operation_id)
            || !credentialAuditNonempty(event?.actor_id, 128)
            || !credentialAuditFingerprint(event?.payload_fingerprint)
            || occurred === null || occurred < 0n
            || (event?.previous_version != null && (previous === null || previous < 1n || version === null || previous >= version))
            || (event?.previous_operation_id != null && !credentialAuditNonempty(event.previous_operation_id, 128))) {
            failures.push("CONFIGURATION_EVENT_INVALID");
        }
        operations.add(event?.operation_id);
    }
    if (row.manually_configured === true && history.length === 0) failures.push("CONFIGURATION_RECEIPT_MISSING");
    return history.length;
}

function runSupplierCredentialAudit() {
    const collection = db.getCollection("supplier_credentials");
    const indexes = collection.getIndexes();
    const unique = (name, fields) => indexes.some((index) => index.name === name
        && index.unique === true && Object.keys(index.key).join(",") === fields.join(",")
        && fields.every((field) => index.key[field] === 1));
    if (!unique("uk_supplier_credentials_id", ["id"])
        || !unique("uk_supplier_credentials_binding", ["provider", "account_scope", "app_key"])) {
        throw new Error("技术凭据必要唯一索引缺失；禁止执行无索引核对");
    }
    const rows = collection.find({id: credentialAuditId}, {
        _id: 0, id: 1, provider: 1, account_scope: 1, app_key: 1, version: 1,
        deleted_at: 1, encrypted_payload: 1, payload_fingerprint: 1, expires_at: 1,
        refresh: 1, reauthorization_history: 1, account_identity_fingerprint: 1,
        authorization_candidate_identity_fingerprint: 1, authorized_uid_fingerprint: 1,
        last_completed_authorization_operation_id: 1, authorization_history: 1,
        manually_configured: 1, configuration_history: 1,
    }).hint("uk_supplier_credentials_id").limit(2).toArray();
    if (rows.length > 1) {
        throw new Error("稳定凭据引用存在重复记录，禁止导入或刷新");
    }
    if (rows.length === 0) {
        print(JSON.stringify({credential_id: credentialAuditId, exists: false, read_only: true}));
        return;
    }
    const row = rows[0];
    const failures = [];
    if (row.provider !== "jd_vop" || !credentialAuditNonempty(row.account_scope, 256)
        || !credentialAuditNonempty(row.app_key, 256)) failures.push("BINDING_INVALID");
    const duplicateBindings = collection.find({provider: row.provider,
        account_scope: row.account_scope, app_key: row.app_key}, {_id: 1})
        .hint("uk_supplier_credentials_binding").limit(2).toArray();
    if (duplicateBindings.length > 1) failures.push("BINDING_DUPLICATE");
    const version = credentialAuditInteger(row.version);
    const expiresAt = credentialAuditInteger(row.expires_at);
    if (version === null || version < 1n || version > 9223372036854775807n) failures.push("VERSION_INVALID");
    if (expiresAt === null || expiresAt < 0n) failures.push("EXPIRY_INVALID");
    if (credentialAuditInteger(row.deleted_at) !== 0n) failures.push("DELETED_OR_INVALID");
    const state = row.refresh?.state;
    const knownStates = ["READY", "CLAIMED", "SENT_UNKNOWN", "REAUTHORIZATION_REQUIRED",
        "AUTHORIZATION_CLAIMED", "AUTHORIZATION_SENT_UNKNOWN", "AUTHORIZATION_REJECTED"];
    if (!knownStates.includes(state)) failures.push("REFRESH_STATE_INVALID");
    const authorization = credentialAuditAuthorization(row, failures);
    const configurationCount = credentialAuditConfiguration(row, failures);
    const placeholder = row.encrypted_payload === "" && row.payload_fingerprint === ""
        && authorization.confirmed === null && credentialAuditAuthorizationState(state)
        && authorization.history.length > 0;
    if (!placeholder && (typeof row.encrypted_payload !== "string" || !row.encrypted_payload.trim()
        || row.encrypted_payload.length > 1048576)) failures.push("ENCRYPTED_PAYLOAD_INVALID");
    if (!placeholder && !credentialAuditFingerprint(row.payload_fingerprint)) failures.push("PAYLOAD_HMAC_INVALID");
    if (placeholder && (state !== "AUTHORIZATION_REJECTED" && authorization.candidate === null)) {
        failures.push("PLACEHOLDER_CANDIDATE_MISSING");
    }
    if (authorization.confirmed !== null && authorization.candidate !== null && authorization.confirmed !== authorization.candidate) {
        failures.push("CONFIRMED_ACCOUNT_CHANGED");
    }
    const deadline = credentialAuditInteger(row.refresh?.lease_until);
    if (state !== "READY" && state !== "AUTHORIZATION_REJECTED" && (!credentialAuditNonempty(row.refresh?.owner_id, 128)
        || !credentialAuditNonempty(row.refresh?.operation_id, 128) || deadline === null)) {
        failures.push("REFRESH_IDENTITY_INVALID");
    }
    if ((state === "SENT_UNKNOWN" || state === "REAUTHORIZATION_REQUIRED" || state === "AUTHORIZATION_SENT_UNKNOWN")
        && credentialAuditInteger(row.refresh?.sent_at) === null) failures.push("SEND_BOUNDARY_MISSING");
    if (!Array.isArray(row.reauthorization_history ?? []) || (row.reauthorization_history ?? []).length > 256) {
        failures.push("REAUTHORIZATION_HISTORY_INVALID");
    }
    if (indexes.some((index) => index.expireAfterSeconds !== undefined)) failures.push("TTL_FORBIDDEN");
    const expired = expiresAt !== null && expiresAt <= BigInt(Math.floor(Date.now() / 1000));
    print(JSON.stringify({credential_id: credentialAuditId, exists: true, read_only: true,
        version: version?.toString(), refresh_state: knownStates.includes(state) ? state : "INVALID",
        expires_at: expiresAt?.toString(), refresh_deadline: deadline?.toString(),
        account_identity_verified: authorization.confirmed !== null,
        authorization_candidate_locked: authorization.candidate !== null,
        authorization_history_count: authorization.history.length,
        manually_configured: row.manually_configured === true,
        configuration_history_count: configurationCount,
        requires_configuration_aware_binary: configurationCount > 0,
        requires_authorization_aware_binary: authorization.history.length > 0 || authorization.confirmed !== null || authorization.candidate !== null,
        authorization_exchange_started: state === "AUTHORIZATION_SENT_UNKNOWN" && row.refresh?.exchange_sent_at != null,
        manual_reauthorization_only: expired || state === "SENT_UNKNOWN" || state === "REAUTHORIZATION_REQUIRED" || credentialAuditAuthorizationState(state),
        violations: failures}));
    if (failures.length) throw new Error("技术凭据核对未通过；按分类处理后重新执行只读核对");
}

runSupplierCredentialAudit();
