"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"

/**
 * 页面动作结果与错误提示的共享状态（确认 / 补拉 / 重试等多条动作链共用）。
 */
export function useMallSyncActionFeedback() {
    const [result, setResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    return { result, setResult, actionError, setActionError }
}

export type MallSyncActionFeedback = ReturnType<
    typeof useMallSyncActionFeedback
>
