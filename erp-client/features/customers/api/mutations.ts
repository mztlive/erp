import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api"
import type {
    CreateCustomerInput,
    CustomerMutationResult,
    CustomerStatus,
    SaveCustomerDetailsInput,
} from "@/features/customers/types"
import { apiErrorMessage, isApiError } from "./errors"
import {
    mapAddressInput,
    mapBankInput,
    mapContactInput,
    mapMutationResult,
    todayBusinessDate,
    tsToIso,
} from "./mappers"
import type {
    BackendCustomerProfile,
    BackendProfileMutation,
} from "./wire-types"

type BackendProfileRequest = {
    idempotency_key: string
    expected_party_version?: number
    expected_customer_version?: number
    legal_name: string
    short_name?: string
    unified_credit_code?: string
    default_payment_term_id?: string
    status?: CustomerStatus
    owner_user_id?: string
    contacts?: ReturnType<typeof mapContactInput>[]
    addresses?: ReturnType<typeof mapAddressInput>[]
    bank_accounts?: ReturnType<typeof mapBankInput>[]
    effective_from: string
    change_reason: string
}

/** 判断提交错误是否属于结果未知。 */
function unknownFromError(
    error: ApiError,
    idempotencyKey: string,
): CustomerMutationResult | null {
    const message = apiErrorMessage(error)
    if (
        error.kind === "Network" ||
        (error.status != null && error.status >= 500)
    ) {
        return { outcome: "unknown", message, idempotencyKey }
    }
    return null
}

/** 从服务端当前资料构造乐观锁冲突结果。 */
async function conflictResult(
    error: ApiError,
    customerId: string,
    fallback: Pick<
        SaveCustomerDetailsInput,
        "expectedLockVersion" | "legalName" | "shortName" | "unifiedCreditCode"
    >,
): Promise<CustomerMutationResult> {
    try {
        const current = await apiGet<BackendCustomerProfile>(
            `/admin/customer-profiles/${customerId}`,
        )
        return {
            outcome: "conflict",
            message: apiErrorMessage(error),
            serverLockVersion: current.version,
            serverRevisionNo: current.current_revision.revision_no,
            serverLegalName: current.current_revision.legal_name,
            serverShortName: current.current_revision.short_name ?? undefined,
            serverUnifiedCreditCode: current.unified_credit_code ?? undefined,
            actor: "系统",
            changedAt: tsToIso(current.updated_at),
        }
    } catch {
        return {
            outcome: "conflict",
            message: apiErrorMessage(error),
            serverLockVersion: fallback.expectedLockVersion,
            serverRevisionNo: 0,
            serverLegalName: fallback.legalName,
            serverShortName: fallback.shortName,
            serverUnifiedCreditCode: fallback.unifiedCreditCode,
            actor: "系统",
            changedAt: new Date().toISOString(),
        }
    }
}

/** 原子创建完整客户资料。 */
export async function createCustomer(
    input: CreateCustomerInput,
): Promise<CustomerMutationResult> {
    const request: BackendProfileRequest = {
        idempotency_key: input.idempotencyKey,
        legal_name: input.legalName.trim(),
        short_name: input.shortName?.trim() || undefined,
        unified_credit_code: input.unifiedCreditCode.trim(),
        default_payment_term_id: input.defaultPaymentTerm?.trim() || undefined,
        status: input.status ?? "active",
        contacts: input.contacts?.map(mapContactInput),
        addresses: input.addresses?.map(mapAddressInput),
        bank_accounts: input.bankAccounts?.map(mapBankInput),
        effective_from: todayBusinessDate(),
        change_reason: "首版建档",
    }
    try {
        return mapMutationResult(
            await apiPost<BackendProfileMutation>(
                "/admin/customer-profiles",
                request,
            ),
        )
    } catch (error) {
        if (isApiError(error)) {
            const unknown = unknownFromError(error, input.idempotencyKey)
            if (unknown) return unknown
            if (error.status === 409) {
                return {
                    outcome: "conflict",
                    message: apiErrorMessage(error),
                    serverLockVersion: 0,
                    serverRevisionNo: 0,
                    serverLegalName: input.legalName,
                    actor: "系统",
                    changedAt: new Date().toISOString(),
                }
            }
        }
        throw error
    }
}

/** 原子保存客户身份、客户角色和显式提交的从属事实集合。 */
export async function saveCustomerDetails(
    input: SaveCustomerDetailsInput,
): Promise<CustomerMutationResult> {
    const request: BackendProfileRequest = {
        idempotency_key: input.idempotencyKey,
        expected_party_version: input.expectedPartyVersion,
        expected_customer_version: input.expectedLockVersion,
        legal_name: input.legalName.trim(),
        short_name: input.shortName,
        unified_credit_code: input.unifiedCreditCode,
        default_payment_term_id: input.defaultPaymentTerm,
        status: input.status,
        contacts: input.contacts?.map(mapContactInput),
        addresses: input.addresses?.map(mapAddressInput),
        bank_accounts: input.bankAccounts?.map(mapBankInput),
        effective_from: todayBusinessDate(),
        change_reason: input.changeReason.trim(),
    }
    try {
        return mapMutationResult(
            await apiPut<BackendProfileMutation>(
                `/admin/customer-profiles/${input.customerId}`,
                request,
            ),
        )
    } catch (error) {
        if (isApiError(error)) {
            const unknown = unknownFromError(error, input.idempotencyKey)
            if (unknown) return unknown
            if (error.status === 409) {
                return conflictResult(error, input.customerId, input)
            }
        }
        throw error
    }
}

/** 按原幂等键查询已经提交成功的最终结果。 */
export async function queryCustomerMutationByIdempotency(
    idempotencyKey: string,
): Promise<CustomerMutationResult | null> {
    const result = await apiGet<BackendProfileMutation | null>(
        `/admin/customer-profile-commands/${encodeURIComponent(idempotencyKey)}`,
    )
    return result ? mapMutationResult(result) : null
}

/** 使用短时字段令牌揭示单个敏感值。 */
export async function revealCustomerSensitiveField(
    revealToken: string,
): Promise<string> {
    const result = await apiPost<{ value: string }>(
        "/admin/customer-sensitive-fields/reveal",
        { reveal_token: revealToken },
    )
    return result.value
}
