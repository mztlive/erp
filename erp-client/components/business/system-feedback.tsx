"use client"

import { useEffect } from "react"
import {
    feedbackStore,
    problemKey,
    reportProblem,
    reportOperationProblem,
} from "@/lib/system-feedback"

/** 业务返回的失败状态进入系统提示；读状态恢复或离开区域后清理，写结果保留待核对。 */
export function SystemFeedback({
    problemId,
    error,
    title,
    description,
    operation = false,
    outcomeUnknown,
}: {
    problemId: string
    error: unknown
    title: string
    description?: string
    operation?: boolean
    outcomeUnknown?: boolean
}) {
    useEffect(() => {
        if (operation) {
            if (error != null)
                reportOperationProblem(error, {
                    key: problemId,
                    title,
                    description,
                    outcomeUnknown,
                })
            return
        }
        const store = feedbackStore()
        const key = problemKey(problemId)
        if (error != null)
            reportProblem(problemId, error, { title, description })
        else store.resolve(key)
        return () => store.resolve(key)
    }, [problemId, error, title, description, operation, outcomeUnknown])
    return null
}
