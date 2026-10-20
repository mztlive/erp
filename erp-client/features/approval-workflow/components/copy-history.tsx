"use client"
import { useInfiniteQuery } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { apiGet, getErrorMessage } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import type { CopyTrigger } from "@/features/approval-processes/types"
type CopyHistoryPage = {
    items: {
        id: string
        recipient_name: string
        trigger: CopyTrigger
        copied_at: number
        read_at: number | null
        delivered: boolean
        undelivered_reason: string | null
        round_no: number
    }[]
    next_cursor: string | null
}
const fetchHistory = (instance: string, after?: string) =>
    apiGet<CopyHistoryPage>(
        `/admin/approval-instances/${encodeURIComponent(instance)}/copies`,
        { after },
    )
export function ApprovalCopyHistory({
    instanceId,
    idPrefix,
}: {
    instanceId: string
    idPrefix: string
}) {
    const profile = useAccountProfileQuery().data
    const query = useInfiniteQuery({
        queryKey: [
            "approval",
            "copy-history",
            instanceId,
            profile?.userid,
            profile?.permissions,
        ],
        queryFn: ({ pageParam }) => fetchHistory(instanceId, pageParam),
        initialPageParam: undefined as string | undefined,
        getNextPageParam: (page) => page.next_cursor ?? undefined,
        enabled: Boolean(instanceId && profile),
        staleTime: 0,
    })
    if (query.isError)
        return (
            <div role="alert" className="text-sm">
                {getErrorMessage(query.error)}
                <Button
                    id={`${idPrefix}-retry`}
                    variant="ghost"
                    onClick={() => void query.refetch()}
                >
                    重试抄送记录
                </Button>
            </div>
        )
    const items = query.data?.pages.flatMap((p) => p.items) ?? []
    if (!items.length) return null
    return (
        <section className="space-y-3">
            <h3 className="text-sm font-medium">抄送记录</h3>
            {items.map((row) => (
                <div key={row.id} className="rounded-md border p-3 text-sm">
                    <p>
                        {row.recipient_name} ·{" "}
                        {COPY_TRIGGER_LABELS[row.trigger]} · 第 {row.round_no}{" "}
                        轮
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {new Date(row.copied_at * 1000).toLocaleString("zh-CN")}{" "}
                        ·{" "}
                        {row.delivered
                            ? row.read_at
                                ? `已读 ${new Date(row.read_at * 1000).toLocaleString("zh-CN")}`
                                : "未读"
                            : row.undelivered_reason || "未送达"}
                    </p>
                </div>
            ))}
            {query.hasNextPage && (
                <Button
                    id={`${idPrefix}-more`}
                    variant="outline"
                    disabled={query.isFetchingNextPage}
                    onClick={() => void query.fetchNextPage()}
                >
                    更多抄送记录
                </Button>
            )}
        </section>
    )
}
