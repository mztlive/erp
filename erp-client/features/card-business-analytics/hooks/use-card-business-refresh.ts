"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"

/**
 * 手动刷新状态簇：refreshing / refreshFailed 与刷新动作。
 * 重新取数本身仍由 Query 的 refetch 完成（不自行 fetch）。
 */
export function useCardBusinessRefresh(refetch: () => Promise<unknown>) {
    const [refreshFailed, setRefreshFailed] = React.useState<string | null>(
        null,
    )
    const [refreshing, setRefreshing] = React.useState(false)

    async function handleRefresh() {
        setRefreshing(true)
        setRefreshFailed(null)
        try {
            await refetch()
        } catch (error) {
            setRefreshFailed(
                getErrorMessage(error, "刷新失败，已保留上次成功数据。"),
            )
        } finally {
            setRefreshing(false)
        }
    }

    return { refreshing, refreshFailed, handleRefresh }
}
