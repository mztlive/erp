"use client"

import Link from "next/link"

import { Button } from "@/components/ui/button"
import { w29Href } from "@/features/execution-projections/lib/url-state"
import type {
    ExecutionProjectionRow,
    ExecutionProjectionView,
} from "@/features/execution-projections/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type RowCommandRequest = {
    kind: "QUERY_RESULT" | "RETRY" | "ESCALATE"
    row: ExecutionProjectionRow
    objectVersion: string
}

/** 详情头部的次要动作（销售单协同 / 重试 / 升级到接口错误中心）。 */
export function ExecutionProjectionDetailSecondaryActions({
    detail,
    rows,
    commandPending,
    onRequestRowCommand,
}: {
    detail: ExecutionProjectionView
    rows: ExecutionProjectionRow[]
    commandPending: boolean
    onRequestRowCommand: (action: RowCommandRequest) => void
}) {
    const findRow = () =>
        rows.find((r) => r.projectionId === detail.identity.projectionId)

    const base = `execution-projections-detail-${toAutomationIdSegment(detail.identity.projectionId)}`
    return (
        <>
            <Button
                id={`${base}-open-collaboration`}
                type="button"
                size="sm"
                variant="outline"
                render={
                    <Link
                        id={`${base}-open-collaboration`}
                        href={`/sales/orders/${detail.identity.salesOrderId}?section=collaboration`}
                    />
                }
            >
                打开销售单协同
            </Button>
            {detail.allowedActions.includes("RETRY") ? (
                <Button
                    id={`${base}-retry`}
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={commandPending}
                    onClick={() => {
                        const row = findRow()
                        if (!row) return
                        onRequestRowCommand({
                            kind: "RETRY",
                            row,
                            objectVersion: detail.objectVersion,
                        })
                    }}
                >
                    重试发送
                </Button>
            ) : null}
            {detail.allowedActions.includes("ESCALATE") ||
            detail.reconciliationStatus === "VERSION_MISMATCH" ? (
                detail.allowedActions.includes("ESCALATE") &&
                !detail.deliveries[0]?.workItemId ? (
                    <Button
                        id={`${base}-escalate`}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={commandPending}
                        onClick={() => {
                            const row = findRow()
                            if (!row) return
                            onRequestRowCommand({
                                kind: "ESCALATE",
                                row,
                                objectVersion: detail.objectVersion,
                            })
                        }}
                    >
                        升级到接口错误中心
                    </Button>
                ) : (
                    <Button
                        id={`${base}-w29`}
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                id={`${base}-w29`}
                                href={w29Href(
                                    detail.deliveries[0]?.workItemId,
                                    detail.deliveries[0]?.errorTaskId,
                                )}
                            />
                        }
                    >
                        去接口错误中心处理
                    </Button>
                )
            ) : null}
        </>
    )
}
