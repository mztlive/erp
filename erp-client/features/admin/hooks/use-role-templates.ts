"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { fetchRoleTemplates, generateBuiltinRoles } from "../api/role-templates"

const templateKey = ["admin", "role-templates"] as const

export function useRoleTemplates(open: boolean) {
    return useQuery({
        queryKey: templateKey,
        queryFn: fetchRoleTemplates,
        enabled: open,
        staleTime: 0,
    })
}

export function useGenerateBuiltinRoles() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: generateBuiltinRoles,
        meta: { affectsDataScope: true },
        onSuccess: async () => {
            await Promise.all([
                client.invalidateQueries({ queryKey: ["admin"] }),
                client.invalidateQueries({ queryKey: ["access-audit"] }),
            ])
        },
    })
}
