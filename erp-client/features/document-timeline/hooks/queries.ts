"use client"
import { useInfiniteQuery } from "@tanstack/react-query"
import { fetchTimeline } from "../api"
import type { TimelineFilter, TimelineTarget } from "../types"

export function useTimelineQuery(
    target: TimelineTarget,
    filter: TimelineFilter,
    enabled = true,
) {
    return useInfiniteQuery({
        queryKey: ["document-timeline", target.kind, target.id, filter],
        queryFn: ({ pageParam }) => fetchTimeline(target, filter, pageParam),
        initialPageParam: undefined as string | undefined,
        getNextPageParam: (page) => page.next_cursor ?? undefined,
        enabled: enabled && Boolean(target.id),
        meta: { businessTimeline: true },
        staleTime: 0,
        refetchOnWindowFocus: true,
    })
}
