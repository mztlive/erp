"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    fetchCopies,
    fetchCopy,
    fetchCopyFile,
    markCopiesRead,
    type CopyFilters,
} from "./api"
export const copyKeys = ["approval-copies"] as const
function useCopyIdentity() {
    const profile = useAccountProfileQuery().data
    return {
        enabled: Boolean(
            profile && hasPermission(profile.permissions, "approval_copy:read"),
        ),
        key: [profile?.userid, profile?.permissions, profile?.role_ids],
    }
}
export function useCopies(query: CopyFilters = {}) {
    const identity = useCopyIdentity()
    return useQuery({
        queryKey: [...copyKeys, "list", identity.key, query],
        queryFn: () => fetchCopies(query),
        enabled: identity.enabled,
        refetchInterval: 60_000,
    })
}
export function useCopy(id: string | null, after?: string) {
    const identity = useCopyIdentity()
    return useQuery({
        queryKey: [...copyKeys, "detail", identity.key, id, after],
        queryFn: () => fetchCopy(id!, after),
        enabled: identity.enabled && Boolean(id),
        staleTime: 0,
        gcTime: 0,
        retry: false,
    })
}
export function useReadCopies() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: markCopiesRead,
        onSuccess: () => client.invalidateQueries({ queryKey: copyKeys }),
    })
}
export function useCopyFile() {
    return useMutation({
        mutationFn: ({
            id,
            file,
            preview,
        }: {
            id: string
            file: string
            preview: boolean
        }) => fetchCopyFile(id, file, preview),
    })
}
