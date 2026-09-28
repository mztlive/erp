"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    applyDemoMasterData,
    ensureDemoFoundation,
    fetchDemoMasterDataStatus,
    removeDemoMasterData,
} from "@/features/demo-master-data/api"

const demoMasterDataKeys = {
    all: ["demo-master-data"] as const,
    status: () => [...demoMasterDataKeys.all, "status"] as const,
}

export function useDemoMasterDataStatusQuery() {
    return useQuery({
        queryKey: demoMasterDataKeys.status(),
        queryFn: fetchDemoMasterDataStatus,
    })
}

async function runApply(
    onProgress: (current: number, total: number) => void,
): Promise<string[]> {
    const foundation = await ensureDemoFoundation()
    const notices = [...foundation.notices]
    let cursor = 0
    for (;;) {
        const report = await applyDemoMasterData(cursor)
        for (const notice of report.notices) {
            if (!notices.includes(notice)) notices.push(notice)
        }
        onProgress(report.next_cursor, report.total_steps)
        if (report.done) return notices
        if (report.next_cursor <= cursor) {
            throw new Error("演示主数据没有继续生成，请稍后重试")
        }
        cursor = report.next_cursor
    }
}

async function runRemove(
    onProgress: (removed: number, derivedRemoved: number) => void,
): Promise<number> {
    let removed = 0
    let derivedRemoved = 0
    let purge = true
    for (;;) {
        const report = await removeDemoMasterData(purge)
        purge = false
        removed += report.removed
        derivedRemoved += report.derived_removed
        onProgress(removed, derivedRemoved)
        if (report.done) return derivedRemoved
        if (report.removed === 0 && report.derived_removed === 0) {
            throw new Error("演示主数据没有继续删除，请稍后重试")
        }
    }
}

export function useApplyDemoMasterDataMutation(
    onProgress: (current: number, total: number) => void,
) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: () => runApply(onProgress),
        onSuccess: async () => {
            await queryClient.invalidateQueries()
        },
    })
}

export function useRemoveDemoMasterDataMutation(
    onProgress: (removed: number, derivedRemoved: number) => void,
) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: () => runRemove(onProgress),
        onSuccess: async () => {
            await queryClient.invalidateQueries()
        },
    })
}
