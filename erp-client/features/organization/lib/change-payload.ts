import type {
    OrganizationChangeRequest,
    OrganizationOperation,
    OrgUnitKind,
} from "@/features/organization/types"

export type OrganizationChangeDraft = {
    operation: OrganizationOperation["operation"]
    name: string
    parentId: string
    kind: OrgUnitKind
    orgUnitId: string
    userId: string
    roleId: string
    includeDescendants: "true" | "false"
    validTo: string
    assignmentId: string
    reason: string
}

export const EMPTY_CHANGE_DRAFT: OrganizationChangeDraft = {
    operation: "create_unit",
    name: "",
    parentId: "",
    kind: "department",
    orgUnitId: "",
    userId: "",
    roleId: "",
    includeDescendants: "false",
    validTo: "",
    assignmentId: "",
    reason: "",
}

export function newIdempotencyKey(): string {
    return crypto.randomUUID()
}

export function shanghaiDateTimeToUnix(value: string): number | null {
    const trimmed = value.trim()
    if (!trimmed) return null
    const normalized = trimmed.length === 16 ? `${trimmed}:00` : trimmed
    const millis = Date.parse(`${normalized}+08:00`)
    if (!Number.isFinite(millis)) return null
    return Math.floor(millis / 1000)
}

export function buildOrganizationOperation(
    draft: OrganizationChangeDraft,
): OrganizationOperation {
    switch (draft.operation) {
        case "create_unit":
            return {
                operation: "create_unit",
                name: draft.name.trim(),
                parent_id: draft.parentId.trim() || null,
                kind: draft.kind,
            }
        case "move_unit":
            return {
                operation: "move_unit",
                org_unit_id: draft.orgUnitId,
                parent_id: draft.parentId.trim() || null,
            }
        case "rename_unit":
            return {
                operation: "rename_unit",
                org_unit_id: draft.orgUnitId,
                name: draft.name.trim(),
            }
        case "disable_unit":
            return { operation: "disable_unit", org_unit_id: draft.orgUnitId }
        case "transfer_member":
            return {
                operation: "transfer_member",
                user_id: draft.userId,
                org_unit_id: draft.orgUnitId,
            }
        case "end_membership":
            return { operation: "end_membership", user_id: draft.userId }
        case "grant_management":
            return {
                operation: "grant_management",
                user_id: draft.userId,
                role_id: draft.roleId,
                org_unit_id: draft.orgUnitId,
                include_descendants: draft.includeDescendants === "true",
                valid_to: shanghaiDateTimeToUnix(draft.validTo),
            }
        case "revoke_management":
            return {
                operation: "revoke_management",
                assignment_id: draft.assignmentId,
            }
    }
}

export function buildOrganizationChangeRequest(
    expectedVersion: number,
    idempotencyKey: string,
    draft: OrganizationChangeDraft,
): OrganizationChangeRequest {
    return {
        expected_version: expectedVersion,
        idempotency_key: idempotencyKey,
        reason: draft.reason.trim(),
        change: buildOrganizationOperation(draft),
    }
}
