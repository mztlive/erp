import type { ApiError } from "@/lib/api/errors"
import type { OrganizationEmptyReason } from "@/features/organization/types"

export function organizationEmptyReason(input: {
    error?: unknown
    noScope: boolean
    filtered: boolean
    empty: boolean
}): OrganizationEmptyReason | null {
    const status =
        input.error && typeof input.error === "object"
            ? (input.error as ApiError).status
            : undefined
    if (status === 403 || status === 401) return "NO_MODULE_PERMISSION"
    if (input.noScope) return "NO_DATA_SCOPE"
    if (!input.empty) return null
    if (input.filtered) return "FILTER_NO_RESULT"
    return "NO_RECORDS_IN_SCOPE"
}
