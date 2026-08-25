"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import type {
    NavSectionId,
    WorkSectionId,
} from "@/features/sales-orders/lib/sales-order-detail-model"

/**
 * 详情页 URL 参数（returnTo / from / mode / workItemId / queueContextId）
 * 与切片导航（section/mode 的 replace 更新）的单一来源。
 */
export function useSalesOrderDetailUrlState({
    salesOrderId,
}: {
    salesOrderId: string
}) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const returnTo = searchParams.get("returnTo")
    const fromWorkspace = searchParams.get("from")
    const focusedWorkItemId = searchParams.get("workItemId")?.trim() ?? ""
    const queueContextId = searchParams.get("queueContextId")?.trim() ?? ""
    const workItemReturnTo =
        returnTo?.trim() ||
        (queueContextId && focusedWorkItemId
            ? `/workspace/tasks?queueContextId=${encodeURIComponent(queueContextId)}&currentWorkItemId=${encodeURIComponent(focusedWorkItemId)}`
            : "/workspace/tasks")

    const selectSection = React.useCallback(
        (next: NavSectionId | WorkSectionId | "versions") => {
            const params = new URLSearchParams()
            params.set("section", next)
            if (returnTo) params.set("returnTo", returnTo)
            if (fromWorkspace) params.set("from", fromWorkspace)
            const workItemId = searchParams.get("workItemId")
            if (
                workItemId &&
                (next === "approval" || next === "change-review")
            ) {
                params.set("workItemId", workItemId)
            }
            const queueContextId = searchParams.get("queueContextId")
            if (queueContextId && workItemId) {
                params.set("queueContextId", queueContextId)
            }
            const qs = params.toString()
            router.replace(
                qs
                    ? `/sales/orders/${salesOrderId}?${qs}`
                    : `/sales/orders/${salesOrderId}`,
                { scroll: false },
            )
        },
        [fromWorkspace, returnTo, router, salesOrderId, searchParams],
    )

    const fromQueue =
        Boolean(returnTo) &&
        (fromWorkspace === "W07" ||
            fromWorkspace === "W08" ||
            fromWorkspace === "W09")
    const backHref = fromQueue && returnTo ? returnTo : "/sales/orders"
    const backLabel =
        fromWorkspace === "W07"
            ? "返回工作台"
            : fromWorkspace === "W08"
              ? "返回采购单列表"
              : fromWorkspace === "W09"
                ? "返回履约处理"
                : "返回列表"

    return {
        returnTo,
        fromWorkspace,
        focusedWorkItemId,
        queueContextId,
        workItemReturnTo,
        fromQueue,
        backHref,
        backLabel,
        selectSection,
    }
}
