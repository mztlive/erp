import { apiGet, apiPost } from "@/lib/api"
import type {
    AuthorizeJdVopAccount,
    JdVopAuthorizationStatus,
} from "../types/jd-vop-authorization"

function authorizationPath(connectionId: string) {
    return `/admin/supplier-api-connections/${encodeURIComponent(connectionId)}/jd-vop-authorization`
}

export function fetchJdVopAuthorization(
    connectionId: string,
    operationId?: string,
) {
    return apiGet<JdVopAuthorizationStatus>(
        authorizationPath(connectionId),
        operationId ? { operation_id: operationId } : undefined,
        { cache: "no-store" },
    )
}

export function authorizeJdVopAccount(
    connectionId: string,
    input: AuthorizeJdVopAccount,
) {
    return apiPost<JdVopAuthorizationStatus>(
        authorizationPath(connectionId),
        input,
        {
            cache: "no-store",
        },
    )
}
