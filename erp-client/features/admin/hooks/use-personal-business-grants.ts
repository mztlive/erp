"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    createPersonalGrant,
    fetchPersonalGrants,
    revokePersonalGrant,
    type PersonalBusinessGrant,
    type PersonalGrantInput,
} from "../api/personal-business-grants"

export const personalGrantKeys = {
    all: ["admin", "personal-business-grants"] as const,
    person: (id: string) => ["admin", "personal-business-grants", id] as const,
}
export function usePersonalGrants(userId: string, enabled = true) {
    return useQuery({
        queryKey: personalGrantKeys.person(userId),
        queryFn: () => fetchPersonalGrants(userId),
        enabled,
        staleTime: 0,
        refetchOnMount: "always",
    })
}
export function usePersonalGrantMutations(userId: string) {
    const client = useQueryClient()
    const invalidate = async () => {
        await Promise.all([
            client.invalidateQueries({ queryKey: ["admin"] }),
            client.invalidateQueries({ queryKey: ["data-scopes"] }),
            client.invalidateQueries({ queryKey: ["organization"] }),
        ])
    }
    const create = useMutation({
        meta: { affectsDataScope: true },
        mutationFn: ({
            grant,
            version,
        }: {
            grant: PersonalGrantInput
            version: number
        }) => createPersonalGrant(userId, grant, version),
        onSuccess: invalidate,
    })
    const revoke = useMutation({
        meta: { affectsDataScope: true },
        mutationFn: ({
            grant,
            version,
        }: {
            grant: PersonalBusinessGrant
            version: number
        }) => revokePersonalGrant(userId, grant, version),
        onSuccess: invalidate,
    })
    return { create, revoke }
}
