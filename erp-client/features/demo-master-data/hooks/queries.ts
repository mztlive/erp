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

export function useRemoveDemoMasterDataMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: removeDemoMasterData,
        retry: false,
        onSuccess: async () => {
            await queryClient.cancelQueries()
            queryClient.removeQueries({ type: "inactive" })
            await queryClient.resetQueries({ type: "active" })
        },
    })
}
