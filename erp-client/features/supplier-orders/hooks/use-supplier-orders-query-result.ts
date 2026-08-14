"use client"

import * as React from "react"

import { useQueryResultMutation } from "@/features/supplier-orders/hooks/queries"
import type { SupplierOrdersUrlUpdater } from "@/features/supplier-orders/lib/url-state"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"

export type QueryActionResult = {
    status: "succeeded" | "failed" | "unknown" | "blocked"
    title: string
    description: string
    reference?: string
}

export type QueryFromPreviewInput = {
    orderId: string
    lockVersion: number
    placeActionId: string
}

/**
 * 「查询原结果」动作：
 * - 列表行入口：先校验 allowedActions，再打开预览、取详情锁版本后提交对象查询命令；
 * - 预览抽屉入口：直接按当前详情提交；
 * - 结果状态渲染为 FormalActionResult（status/title/description/reference）。
 */
export function useSupplierOrdersQueryResult({
    updateUrl,
}: {
    updateUrl: SupplierOrdersUrlUpdater
}) {
    const [actionResult, setActionResult] =
        React.useState<QueryActionResult | null>(null)
    const commandIdentities = React.useRef(
        new Map<string, { operationId: string; idempotencyKey: string }>(),
    )
    const queryResultMutation = useQueryResultMutation()

    function investigationIdentity(source: string, orderId: string) {
        const key = `${source}:${orderId}`
        const existing = commandIdentities.current.get(key)
        if (existing) return { key, ...existing }
        const identity = {
            operationId: `w26:${source}:${crypto.randomUUID()}`,
            idempotencyKey: `w26:${source}:${crypto.randomUUID()}`,
        }
        commandIdentities.current.set(key, identity)
        return { key, ...identity }
    }

    const openPreview = React.useCallback(
        (orderId: string) => updateUrl({ preview: orderId }, "push"),
        [updateUrl],
    )

    const handleQueryFromList = React.useCallback(
        async (row: SupplierOrderListRow) => {
            if (!row.allowedActions.includes("QUERY_RESULT")) {
                setActionResult({
                    status: "blocked",
                    title: "无法查询原结果",
                    description:
                        row.actionBlockers.find(
                            (b) => b.action === "QUERY_RESULT",
                        )?.message ?? "当前订单不可查询",
                })
                return
            }
            // 打开预览并在取得详情锁版本后查询
            openPreview(row.orderId)
            const { fetchSupplierOrderDetail } =
                await import("@/features/supplier-orders/api/index")
            const detail = await fetchSupplierOrderDetail({
                orderId: row.orderId,
            })
            if (detail.workItem) {
                setActionResult({
                    status: "blocked",
                    title: "请进入正式任务处理",
                    description:
                        "当前订单已关联正式任务；列表直接入口不得降级提交对象查询命令。",
                })
                return
            }
            const identity = investigationIdentity("list-query", row.orderId)
            const res = await queryResultMutation.mutateAsync({
                commandKind: "OBJECT",
                orderId: row.orderId,
                expectedLockVersion: detail.order.lockVersion,
                action: "QUERY_RESULT",
                targetSupplierActionId: detail.placeActionId,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
            })
            if (res.status !== "unknown") {
                commandIdentities.current.delete(identity.key)
            }
            setActionResult({
                status:
                    res.status === "unknown"
                        ? "unknown"
                        : res.status === "blocked"
                          ? "blocked"
                          : res.status === "succeeded"
                            ? "succeeded"
                            : "failed",
                title:
                    res.status === "succeeded"
                        ? "查询原结果已完成"
                        : res.status === "unknown"
                          ? "查询结果仍未知"
                          : "查询未成功",
                description: res.message,
                reference: res.reference,
            })
        },
        [openPreview, queryResultMutation],
    )

    const queryFromPreview = React.useCallback(
        async (input: QueryFromPreviewInput) => {
            const identity = investigationIdentity(
                "preview-query",
                input.orderId,
            )
            const res = await queryResultMutation.mutateAsync({
                commandKind: "OBJECT",
                orderId: input.orderId,
                expectedLockVersion: input.lockVersion,
                action: "QUERY_RESULT",
                targetSupplierActionId: input.placeActionId,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
            })
            if (res.status !== "unknown") {
                commandIdentities.current.delete(identity.key)
            }
            setActionResult({
                status:
                    res.status === "failed"
                        ? "failed"
                        : res.status === "blocked"
                          ? "blocked"
                          : res.status === "unknown"
                            ? "unknown"
                            : "succeeded",
                title:
                    res.status === "succeeded"
                        ? "查询原结果已完成"
                        : "查询未形成终局成功",
                description: res.message,
                reference: res.reference,
            })
        },
        [queryResultMutation],
    )

    const dismissActionResult = React.useCallback(
        () => setActionResult(null),
        [],
    )

    return {
        actionResult,
        dismissActionResult,
        queryPending: queryResultMutation.isPending,
        handleQueryFromList,
        queryFromPreview,
    }
}
