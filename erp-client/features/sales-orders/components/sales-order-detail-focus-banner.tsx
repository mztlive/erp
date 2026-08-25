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
    nextStepOwner,
    type FocusTask,
} from "@/features/sales-orders/lib/sales-order-detail-model"

export function FocusTaskBanner({
    order,
    focusTask,
    action,
}: {
    order: SalesOrderDetailView
    focusTask: FocusTask
    action?: React.ReactNode
}) {
    const due = stageDueDisplay(order)
    const Icon = focusTask.tone === "warning" ? ShieldAlertIcon : InfoIcon
    const detail = [
        focusTask.description,
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
