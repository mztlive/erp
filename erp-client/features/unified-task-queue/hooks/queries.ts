"use client"

import { useQuery } from "@tanstack/react-query"

import { workItemKeys } from "@/features/work-items"

import {
    fetchUnifiedTaskQueue,
    fetchUnifiedTaskQueueCounts,
} from "../api/work-items"
import type { UnifiedQueueFilters } from "../types"

export const unifiedQueueKeys = {
    all: workItemKeys.all,
    view: (filters: UnifiedQueueFilters) =>
        [...workItemKeys.all, "unified-queue", filters] as const,
    counts: () => [...workItemKeys.all, "mine-count"] as const,
}

export function useUnifiedTaskQueueQuery(filters: UnifiedQueueFilters) {
    return useQuery({
        queryKey: unifiedQueueKeys.view(filters),
        queryFn: () => fetchUnifiedTaskQueue(filters),
    })
}

export function useUnifiedTaskCountQuery() {
    return useQuery({
        queryKey: unifiedQueueKeys.counts(),
        queryFn: fetchUnifiedTaskQueueCounts,
    })
}
