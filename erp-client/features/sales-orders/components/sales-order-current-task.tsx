"use client"

import { ClipboardCheckIcon } from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
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
    const quantity = progress?.unitCode
        ? qtyWithUnit(progress.pendingQuantity, progress.unitCode)
        : undefined
    return (
        <Alert variant="info" id="sales-order-current-task">
            <ClipboardCheckIcon aria-hidden="true" />
            <AlertDescription className="flex flex-wrap items-center justify-between gap-4">
                <div className="min-w-0">
                    <p className="font-semibold text-foreground">
                        {pendingApproval ? "审批跟进" : "待我处理"}
                        <span className="ml-3">
                            {pendingApproval
                                ? "销售单正在审批"
                                : quantity
                                  ? `${quantity}待客户验收`
                                  : `${progress?.pendingFactCount ?? ""} 批交付待客户验收`}
                        </span>
                    </p>
                    <p className="mt-1 text-sm">
                        {pendingApproval
                            ? `当前处理人：${order.primaryStatus.ownerUserName || "以审批记录为准"}`
                            : "请核对客户实际收货结果，并上传签收凭证。"}
                    </p>
                </div>
                <Button
                    id="sales-order-current-task-action"
                    size="sm"
                    variant={pendingApproval ? "outline" : "default"}
                    onClick={() =>
                        onSelectSection(
                            pendingApproval ? "approval" : "acceptance",
                            pendingApproval ? undefined : { mode: "register" },
                        )
                    }
                >
                    {pendingApproval ? "查看审批" : "登记验收"}
                </Button>
            </AlertDescription>
        </Alert>
    )
}
