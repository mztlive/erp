"use client"
import {
    useInfiniteQuery,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
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
        enabled: profile?.account_kind === "admin",
        key: [
            profile?.userid,
            profile?.account_kind,
            profile?.policy_version,
            profile?.organization_version,
            profile?.permissions,
        ],
    }
}
export function useCopies(query: CopyFilters = {}, enabled = true) {
    const identity = useCopyIdentity()
    return useQuery({
        queryKey: [...copyKeys, "list", identity.key, query],
        queryFn: () => fetchCopies(query),
        enabled: identity.enabled && enabled,
        meta: { workspaceNavigation: true },
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
/** 抄送详情追加式历史分页：保留首屏冻结快照并按游标累积审批执行记录，彻底修复翻页丢失前页问题。 */
export function useCopyInfinite(id: string | null) {
    const identity = useCopyIdentity()
    return useInfiniteQuery({
        queryKey: [...copyKeys, "detail-infinite", identity.key, id],
        queryFn: ({ pageParam }) => fetchCopy(id!, pageParam),
        initialPageParam: undefined as string | undefined,
        getNextPageParam: (page) =>
            page.history.has_more && page.history.next_cursor
                ? page.history.next_cursor
                : undefined,
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
