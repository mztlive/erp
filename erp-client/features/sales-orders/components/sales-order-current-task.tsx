"use client"

import { ClipboardCheckIcon } from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    displayActorName,
    displayReadableName,
} from "@/features/approval-workflow/display"
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { AcceptanceOrderProgress } from "../lib/acceptance-model"
import { qtyWithUnit } from "../lib/acceptance-model"
import {
    isSalesOrderApprovalInProgress,
    type NavSectionId,
} from "../lib/sales-order-detail-model"

/** 仅把当前账号可以办理的验收列为待我处理，审批等待保持查阅语义。 */
export function SalesOrderCurrentTask({
    order,
    canAccept,
    progress,
    onSelectSection,
}: {
    order: SalesOrderDetailView
    canAccept: boolean
    progress?: AcceptanceOrderProgress
    onSelectSection: (
        section: NavSectionId,
        extras?: { mode?: "register" },
    ) => void
}) {
    const pendingApproval = isSalesOrderApprovalInProgress(order)
    if (!pendingApproval && !canAccept) return null
    if (pendingApproval) {
        const instance = order.approval?.instance
        const blocked = instance?.status === "BLOCKED"
        const node = displayReadableName(
            instance?.currentNodeName,
            instance?.currentNode,
        )
        const actor =
            displayActorName(
                instance?.currentAssigneeName,
                instance?.currentAssignee,
            ) || displayActorName(order.primaryStatus.ownerUserName)
        return (
            <Alert
                variant={blocked ? "destructive" : "info"}
                id="sales-order-current-task"
                className="rounded-lg py-2"
            >
                <ClipboardCheckIcon aria-hidden="true" />
                <AlertDescription className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                    <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
                        <span className="font-medium text-foreground">
                            {blocked ? "审批受阻" : "审批中"}
                        </span>
                        {node ? <span>{node}</span> : null}
                        <span>当前处理人：{actor || "以审批记录为准"}</span>
                    </div>
                    <Button
                        id="sales-order-current-task-action"
                        size="sm"
                        variant="outline"
                        onClick={() => onSelectSection("approval")}
                    >
                        查看审批
                    </Button>
                </AlertDescription>
            </Alert>
        )
    }
    const quantity = progress?.unitCode
        ? qtyWithUnit(progress.pendingQuantity, progress.unitCode)
        : undefined
    return (
        <Alert variant="info" id="sales-order-current-task">
            <ClipboardCheckIcon aria-hidden="true" />
            <AlertDescription className="flex flex-wrap items-center justify-between gap-4">
                <div className="min-w-0">
                    <p className="font-semibold text-foreground">
                        待我处理
                        <span className="ml-3">
                            {quantity
                                ? `${quantity}待客户验收`
                                : `${progress?.pendingFactCount ?? ""} 批交付待客户验收`}
                        </span>
                    </p>
                    <p className="mt-1 text-sm">
                        请核对客户实际收货结果，并上传签收凭证。
                    </p>
                </div>
                <Button
                    id="sales-order-current-task-action"
                    size="sm"
                    variant="default"
                    onClick={() =>
                        onSelectSection("acceptance", { mode: "register" })
                    }
                >
                    登记验收
                </Button>
            </AlertDescription>
        </Alert>
    )
}
