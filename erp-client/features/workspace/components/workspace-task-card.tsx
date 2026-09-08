"use client"

import {
    BadgeCheckIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    FileTextIcon,
    TruckIcon,
    WalletIcon,
} from "lucide-react"

import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { splitDetailSections } from "../lib/detail-facts"
import { findSourceSalesOrder } from "../lib/source-sales-order"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"

/** 表头和任务行共用列宽；手机将金额与截止时间合并到右列。 */
export const workspaceTaskColumns =
    "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 @min-[600px]/workspace-queue:grid-cols-[minmax(0,1fr)_7.5rem_8rem_1rem]"

const FAMILY_APPEARANCE = {
    approval: {
        icon: FileTextIcon,
        className:
            "bg-violet-50 text-violet-600 dark:bg-violet-950/40 dark:text-violet-300",
    },
    procurement: {
        icon: FileTextIcon,
        className:
            "bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300",
    },
    fulfillment: {
        icon: TruckIcon,
        className:
            "bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300",
    },
    finance: {
        icon: WalletIcon,
        className:
            "bg-amber-50 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300",
    },
    exception: {
        icon: CircleAlertIcon,
        className:
            "bg-orange-50 text-orange-700 dark:bg-orange-950/40 dark:text-orange-300",
    },
}

/** 同一行对齐业务对象、金额与期限，保留原生按钮的键盘操作和稳定定位。 */
export function WorkspaceTaskCard({
    item,
    selected,
    onSelect,
}: {
    item: WorkspaceWorkItem
    selected: boolean
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const tracking = item.workItemType === "APPROVAL_INSTANCE"
    const amount =
        item.amountSummary ??
        splitDetailSections(item.summarySections, item.counterpartyName)
            .amounts[0]
    const number = stripDocumentNumberPrefix(item.stableNumber)
    const sourceSales = findSourceSalesOrder(item.summarySections)
    const appearance = FAMILY_APPEARANCE[item.family]
    const Icon =
        item.workItemType === "CUSTOMER_ACCEPTANCE_REGISTRATION"
            ? BadgeCheckIcon
            : appearance.icon
    const title = item.counterpartyName || item.objectTitle
    const deadline = tracking ? (
        <StatusBadge label={item.statusLabel} tone={item.statusTone} />
    ) : (
        <>
            <time
                dateTime={item.dueAt || undefined}
                className="num text-xs sm:text-sm"
            >
                {item.dueAt ? item.dueAtLabel : "未设截止"}
            </time>
            {blocked ? (
                <StatusBadge label="受阻" tone="warning" />
            ) : overdue ? (
                <StatusBadge label="已超期" tone="warning" />
            ) : null}
        </>
    )

    return (
        <button
            type="button"
            id={`workspace-task-${toAutomationIdSegment(item.workItemId)}`}
            data-testid={
                item.workItemType === "PROCUREMENT_ORDER_CREATION"
                    ? `work-item-procurement-order-creation-${item.workItemId}`
                    : undefined
            }
            aria-label={`${item.workItemTypeLabel} ${title} ${number}${sourceSales ? " 来源 " + sourceSales.orderNo : ""}`}
            aria-current={selected ? "true" : undefined}
            onClick={() => onSelect(item)}
            className={cn(
                workspaceTaskColumns,
                "relative min-h-22 w-full px-4 py-3 text-left transition-colors focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring sm:px-5",
                selected
                    ? "bg-muted/60 before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-foreground"
                    : "hover:bg-muted/30",
            )}
        >
            <span className="flex min-w-0 items-start gap-3">
                <span
                    className={cn(
                        "mt-1 flex size-9 shrink-0 items-center justify-center rounded-lg",
                        appearance.className,
                    )}
                >
                    <Icon className="size-4" aria-hidden="true" />
                </span>
                <span className="flex min-w-0 flex-col gap-0.5">
                    <span className="truncate text-sm text-muted-foreground">
                        {item.workItemTypeLabel}
                    </span>
                    <span
                        className="truncate text-base font-medium"
                        title={title}
                    >
                        {title}
                    </span>
                    <span
                        className="num truncate text-xs text-muted-foreground"
                        title={
                            sourceSales
                                ? number + " · 来源 " + sourceSales.orderNo
                                : number
                        }
                    >
                        {number}
                    </span>
                    {item.listSummary ? (
                        <span className="truncate text-xs text-muted-foreground">
                            {item.listSummary}
                        </span>
                    ) : null}
                </span>
            </span>
            <span className="flex flex-col items-end gap-2 @min-[600px]/workspace-queue:items-start">
                <span
                    className="num whitespace-nowrap text-sm font-medium"
                    aria-label={
                        amount
                            ? amount.label + " " + amount.value
                            : "金额未提供"
                    }
                >
                    {amount?.value ?? "—"}
                </span>
                {amount ? (
                    <span className="text-xs text-muted-foreground">
                        {amount.label}
                    </span>
                ) : null}
                <span className="flex flex-col items-end gap-1 text-muted-foreground @min-[600px]/workspace-queue:hidden">
                    {deadline}
                </span>
            </span>
            <span className="hidden flex-col items-start gap-1.5 @min-[600px]/workspace-queue:flex">
                {deadline}
            </span>
            <ChevronRightIcon
                aria-hidden="true"
                className="hidden size-4 text-muted-foreground @min-[600px]/workspace-queue:block"
            />
        </button>
    )
}
