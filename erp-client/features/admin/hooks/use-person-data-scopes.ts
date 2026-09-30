"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    fetchPersonScopes,
    savePersonScope,
    type PersonScopeInput,
} from "../api/person-data-scopes"

export function usePersonScopes(userId: string, enabled = true) {
    return useQuery({
        queryKey: ["admin", "person-data-scopes", userId],
        queryFn: () => fetchPersonScopes(userId),
        enabled: enabled && Boolean(userId),
        staleTime: 0,
        refetchOnMount: "always",
    })
}
export function useSavePersonScope(userId: string) {
    const client = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: ({
            value,
            version,
        }: {
            value: PersonScopeInput
            version: number
        }) => savePersonScope(userId, value, version),
        onSuccess: async () => {
            await Promise.all([
                client.invalidateQueries({ queryKey: ["admin"] }),
                client.invalidateQueries({ queryKey: ["organization"] }),
                client.invalidateQueries({ queryKey: ["access-audit"] }),
            ])
        },
    })
}
