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

import { workspaceTaskPresentation } from "../lib/task-presentation"
import { findSourceSalesOrder } from "../lib/source-sales-order"
import { workspaceTaskRightContent } from "../lib/task-right-content"
import type { WorkspaceWorkItem } from "../types"

/** 表头和任务行共用列宽；窄窗口将本次处理与状态合并到右列。 */
export const workspaceTaskColumns =
    "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 @min-[600px]/workspace-queue:grid-cols-[minmax(0,1fr)_10.5rem_10rem_1rem]"

const FAMILY_APPEARANCE = {
    approval: {
        icon: FileTextIcon,
        className: "bg-violet-soft text-violet-soft-foreground",
    },
    procurement: {
        icon: FileTextIcon,
        className: "bg-info-soft text-info-soft-foreground",
    },
    fulfillment: {
        icon: TruckIcon,
        className: "bg-info-soft text-info-soft-foreground",
    },
    finance: {
        icon: WalletIcon,
        className: "bg-warning-soft text-warning-soft-foreground",
    },
    exception: {
        icon: CircleAlertIcon,
        className: "bg-orange-soft text-orange-soft-foreground",
    },
}

/** 同一行对齐业务对象、本次处理摘要与状态，保留原生按钮的键盘操作和稳定定位。 */
export function WorkspaceTaskCard({
    item,
    selected,
    onSelect,
}: {
    item: WorkspaceWorkItem
    selected: boolean
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    const presentation = workspaceTaskPresentation(item)
    const {
        title,
        typeLabel,
        number,
        summary,
        amount,
        attention,
        approvalProgress,
    } = presentation
    const sourceSales = findSourceSalesOrder(item.summarySections)
    const appearance = FAMILY_APPEARANCE[item.family]
    const Icon =
        item.workItemType === "CUSTOMER_ACCEPTANCE_REGISTRATION"
            ? BadgeCheckIcon
            : appearance.icon
    const right = workspaceTaskRightContent(item, amount, approvalProgress)
    const status = <TaskStatus content={right} />

    return (
        <button
            type="button"
            id={`workspace-task-${toAutomationIdSegment(item.workItemId)}`}
            data-testid={
                item.workItemType === "PROCUREMENT_ORDER_CREATION"
                    ? `work-item-procurement-order-creation-${item.workItemId}`
                    : undefined
            }
            aria-label={`${typeLabel} ${title} ${number}${sourceSales ? " 来源 " + sourceSales.orderNo : ""} ${right.metric.label} ${right.metric.value} ${right.status.label}`}
            aria-current={selected ? "true" : undefined}
            onClick={() => onSelect(item)}
            className={cn(
                workspaceTaskColumns,
                "relative min-h-24 w-full px-4 py-4 text-left transition-colors focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring sm:px-5",
                selected
                    ? "bg-muted/60 before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-foreground"
                    : "hover:bg-muted/30",
            )}
        >
            <span className="flex min-w-0 items-start gap-3">
                <span
                    className={cn(
                        "mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg",
                        appearance.className,
                    )}
                >
                    <Icon className="size-4" aria-hidden="true" />
                </span>
                <span className="flex min-w-0 flex-1 flex-col gap-1.5">
                    <span
                        className="truncate text-sm font-medium"
                        title={title}
                    >
                        {title}
                    </span>
                    <span className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1 text-xs text-muted-foreground">
                        <span>{typeLabel}</span>
                        <span aria-hidden="true">·</span>
                        <span
                            className="num min-w-0 truncate"
                            title={
                                sourceSales
                                    ? number + " · 来源 " + sourceSales.orderNo
                                    : number
                            }
                        >
                            {number}
                        </span>
                    </span>
                    {summary && (
                        <span
                            className={cn(
                                "line-clamp-2 text-xs leading-relaxed",
                                attention
                                    ? "text-warning-soft-foreground"
                                    : "text-muted-foreground",
                            )}
                            title={summary}
                        >
                            {summary}
                        </span>
                    )}
                </span>
            </span>
            <span className="flex min-w-0 flex-col items-end gap-1.5 @min-[600px]/workspace-queue:items-start">
                <span
                    className={cn(
                        "num line-clamp-2 text-sm",
                        right.metric.unavailable || right.tracking
                            ? "text-muted-foreground"
                            : "font-medium",
                    )}
                    title={right.metric.detail}
                >
                    {right.metric.value}
                </span>
                <span className="text-xs text-muted-foreground">
                    {right.metric.label}
                </span>
                {right.metric.detail && !right.metric.unavailable && (
                    <span
                        className="line-clamp-2 text-xs leading-relaxed text-muted-foreground"
                        title={right.metric.detail}
                    >
                        {right.metric.detail}
                    </span>
                )}
                <span className="mt-1 flex flex-col items-end gap-1.5 @min-[600px]/workspace-queue:hidden">
                    {status}
                </span>
            </span>
            <span className="hidden min-w-0 flex-col items-start gap-1.5 @min-[600px]/workspace-queue:flex">
                {status}
            </span>
            <ChevronRightIcon
                aria-hidden="true"
                className="hidden size-4 text-muted-foreground @min-[600px]/workspace-queue:block"
            />
        </button>
    )
}

/** 审批跟踪突出节点与结果；执行任务按实际期限标注日期用途。 */
function TaskStatus({
    content,
}: {
    content: ReturnType<typeof workspaceTaskRightContent>
}) {
    return (
        <>
            <StatusBadge
                label={content.status.label}
                tone={content.status.tone}
            />
            {content.status.detail && (
                <span
                    className="line-clamp-2 text-xs leading-relaxed text-muted-foreground"
                    title={content.status.detail}
                >
                    {content.status.detail}
                </span>
            )}
            {content.date && (
                <span className="flex flex-wrap gap-x-1 text-xs text-muted-foreground">
                    <span>{content.date.label}</span>
                    <time className="num" dateTime={content.date.dateTime}>
                        {content.date.value}
                    </time>
                </span>
            )}
        </>
    )
}
