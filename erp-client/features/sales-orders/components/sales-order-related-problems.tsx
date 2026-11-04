"use client"

import { useEffect, useRef } from "react"
import { reportProblem, resolveProblem } from "@/lib/system-feedback"
import type { SalesOrderDetailView } from "../api/contracts"

/** 关联资料失败汇入宿主面板，不占用单据页头。 */
export function SalesOrderRelatedProblems({
    order,
    onRetry,
}: {
    order: SalesOrderDetailView
    onRetry: () => Promise<unknown>
}) {
    const retry = useRef(onRetry)
    retry.current = onRetry
    const previous = useRef<string[]>([])
    useEffect(() => {
        const keys = (order.loadWarnings ?? []).map((warning) => {
            const key = `sales-related:${order.id}:${warning.source}`
            reportProblem(key, undefined, {
                presentation: warning.failure,
                title: `${warning.source}${warning.failure.kind === "permission" ? "无权查看" : "暂未加载"}`,
                retry: () => retry.current(),
            })
            return key
        })
        for (const key of previous.current)
            if (!keys.includes(key)) resolveProblem(key)
        previous.current = keys
    }, [order.id, order.loadWarnings])
    return null
}
