"use client"

import type { AccessColumnsInput } from "@/features/access-audit/hooks/access-columns-input"
import { useAuditColumns } from "@/features/access-audit/hooks/use-audit-columns"
import { useFieldColumns } from "@/features/access-audit/hooks/use-field-columns"
import { useRoleColumns } from "@/features/access-audit/hooks/use-role-columns"
import { useScopeColumns } from "@/features/access-audit/hooks/use-scope-columns"
import { useUserColumns } from "@/features/access-audit/hooks/use-user-columns"

export type {
    AccessColumnsInput,
    AccountFormState,
    DeletingAccountState,
    DeletingRoleState,
} from "@/features/access-audit/hooks/access-columns-input"

function useAccessColumns(input: AccessColumnsInput) {
    return {
        auditColumns: useAuditColumns(input),
        fieldColumns: useFieldColumns(input),
        roleColumns: useRoleColumns(input),
        scopeColumns: useScopeColumns(input),
        userColumns: useUserColumns(input),
    }
}

export { useAccessColumns }
