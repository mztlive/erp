"use client"

import * as React from "react"
import { InfoIcon, ShieldAlertIcon } from "lucide-react"

import {
    Alert,
    AlertAction,
    AlertDescription,
    AlertTitle,
} from "@/components/ui/alert"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { stageDueDisplay } from "@/features/sales-orders/lib/labels"
import {
    isOpenProcurementRejection,
    nextStepOwner,
    type FocusTask,
} from "@/features/sales-orders/lib/sales-order-detail-model"

export function FocusTaskBanner({
    order,
    focusTask,
    action,
    canActOnRejection = false,
}: {
    order: SalesOrderDetailView
    focusTask: FocusTask
    action?: React.ReactNode
    canActOnRejection?: boolean
}) {
    const due = stageDueDisplay(order)
    const Icon = focusTask.tone === "warning" ? ShieldAlertIcon : InfoIcon
    const detail = [
        focusTask.description,
        focusTask.id === "procurement-rejection"
            ? rejectionBannerDetail(order, canActOnRejection)
            : null,
        `责任人 ${nextStepOwner(order)}`,
        due ? `时限 ${due.label}` : null,
    ]
        .filter(Boolean)
        .join(" · ")

    return (
        <Alert
            variant={focusTask.tone === "warning" ? "warning" : "info"}
            className="rounded-lg px-3 py-2"
        >
            <Icon aria-hidden="true" />
            <AlertTitle className="text-sm">
                现在要处理 · {focusTask.title}
            </AlertTitle>
            {action ? <AlertAction>{action}</AlertAction> : null}
            <AlertDescription className="text-xs [&_p]:mb-0">
                {detail}
            </AlertDescription>
        </Alert>
    )
}

function rejectionBannerDetail(order: SalesOrderDetailView, canAct: boolean) {
    const rejection = order.procurementRejection
    if (!rejection || !isOpenProcurementRejection(order)) return ""

    const changedCommercial =
        rejection.draftDifference.changedItemOrService ||
        rejection.draftDifference.changedSalesPrice
    const parts = [
        `第 ${rejection.rejectedSubmissionNo} 次报给采购`,
        [rejection.rejectedByLabel, rejection.rejectedAt]
            .filter(Boolean)
            .join(" · ") || null,
        rejection.estimatedCost ? `采购成本 ${rejection.estimatedCost}` : null,
        rejection.estimatedMarginPercent
            ? `预计毛利 ${rejection.estimatedMarginPercent}%`
            : null,
        changedCommercial
            ? "商品或价格已有改动，用页头「改完再报」核对整单"
            : "还没改商品或价格，改完后才能再报",
        canAct ? null : "当前账号不能改这张单，也不能作废",
    ]
    return parts.filter(Boolean).join(" · ")
}
