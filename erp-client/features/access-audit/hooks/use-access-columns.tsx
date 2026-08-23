"use client"

import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import { useAuditColumns } from "@/features/access-audit/hooks/use-audit-columns"
import { useRoleColumns } from "@/features/access-audit/hooks/use-role-columns"
import { useUserColumns } from "@/features/access-audit/hooks/use-user-columns"

export type {
    AccessColumnsInput,
    DeletingRoleState,
} from "@/features/access-audit/hooks/access-columns-input"

function useAccessColumns(input: AccessColumnsInput) {
    return {
        auditColumns: useAuditColumns(input),
        roleColumns: useRoleColumns(input),
        userColumns: useUserColumns(input),
    }
}

export { useAccessColumns }
