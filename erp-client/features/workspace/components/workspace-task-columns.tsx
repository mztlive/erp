"use client"

import type { ColumnDef } from "@tanstack/react-table"
import {
    Building2Icon,
    CalendarClockIcon,
    ChevronRightIcon,
    CircleCheckIcon,
    FileTextIcon,
    LinkIcon,
    ListTodoIcon,
    PackageIcon,
    WalletCardsIcon,
    type LucideIcon,
} from "lucide-react"
import { queueTypeIcon } from "../lib/queue-presentation"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { displayName } from "@/lib/display-name"
import { workspaceTaskPresentation } from "../lib/task-presentation"
import { workspaceTaskRightContent } from "../lib/task-right-content"
import { findSourceSalesOrder } from "../lib/source-sales-order"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceMoney } from "./workspace-money"
import { WorkspaceFulfillmentBadges } from "./workspace-fulfillment-badges"

/** 每行只消费列表已有的授权摘要，不补查当前单据替换审批资料。 */
export function queueTaskRow(item: WorkspaceWorkItem) {
    const presentation = workspaceTaskPresentation(item)
    const first = item.briefLines?.[0]
    const goods = [
        "purchase_order",
        "sales_order",
        "voucher_sales_order",
    ].includes(item.businessObjectType)
    const totalLines =
        item.briefMoreCount == null
            ? undefined
            : (item.briefLines?.length ?? 0) + item.briefMoreCount
    const summaryPreview =
        !presentation.attention && goods && first
            ? `${first.title}${totalLines != null && totalLines > 1 ? ` 等 ${totalLines} 项` : ""}`
            : presentation.summary
    return {
        item,
        summaryPreview,
        presentation,
        processable:
            item.status === "OPEN" &&
            !isBlockedWorkItem(item) &&
            item.allowedActions.some((action) =>
                ["PROCESS", "OPEN_DOCUMENT", "APPROVE", "REJECT"].includes(
                    action,
                ),
            ),
        source: findSourceSalesOrder(item.summarySections),
        content: workspaceTaskRightContent(
            item,
            presentation.amount,
            presentation.approvalProgress,
        ),
    }
}
export type QueueTaskRow = ReturnType<typeof queueTaskRow>

/** 只显示任务截止时间；商品交期留在摘要，已完成事项不再标为超期。 */
function TaskDeadline({ item }: { item: WorkspaceWorkItem }) {
    if (!item.dueAt || Number.isNaN(new Date(item.dueAt).getTime()))
        return <span className="text-muted-foreground">—</span>
    const date = new Date(item.dueAt)
    const label = new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    }).format(date)
    const urgent = item.status === "OPEN" && item.dueBucket !== "later"
    return (
        <time
            dateTime={item.dueAt}
            title={`待办截止：${label}`}
            className="num"
        >
            {urgent ? (
                <StatusBadge
                    label={item.dueBucket === "overdue" ? "已超期" : "今日到期"}
                    tone={
                        item.dueBucket === "overdue" ? "destructive" : "warning"
                    }
                />
            ) : (
                label
            )}
        </time>
    )
}

function columnTitle(label: string, Icon: LucideIcon) {
    return (
        <span className="inline-flex items-center gap-2">
            <Icon className="size-3.5" aria-hidden="true" />
            {label}
        </span>
    )
}

/** 待办将对象、摘要和处理口径合并展示，不丢失异常提示或未知金额。 */
function TaskOverview({ row }: { row: QueueTaskRow }) {
    return (
        <div className="min-w-0 space-y-1.5 py-1 whitespace-normal">
            <div className="line-clamp-2" title={row.presentation.title}>
                {row.presentation.title}
            </div>
            <div className="flex flex-wrap items-baseline gap-x-1.5 gap-y-1 text-xs text-muted-foreground">
                <span
                    className={cn(
                        "line-clamp-2 max-w-96 whitespace-normal",
                        row.presentation.attention &&
                            "text-warning-soft-foreground",
                    )}
                    title={row.presentation.summary}
                >
                    {row.summaryPreview || "—"}
                </span>
                <span aria-hidden="true">·</span>
                <span title={row.content.metric.detail}>
                    {row.content.metric.label}{" "}
                    <WorkspaceMoney
                        value={row.content.metric.value}
                        display="exact"
                    />
                </span>
            </div>
        </div>
    )
}

/** 待办采用五列摘要布局；审批跟踪和处理记录保留各自的独立指标列。 */
export function workspaceTaskColumns({
    selectedTypes,
    tracking,
    history,
    onSelect,
}: {
    selectedTypes: readonly string[]
    tracking: boolean
    history: boolean
    onSelect: (item: WorkspaceWorkItem) => void
}): ColumnDef<QueueTaskRow, unknown>[] {
    const purchase =
        selectedTypes.length === 1 &&
        selectedTypes[0] === "approval:purchase_order"
    const mixed =
        selectedTypes.length !== 1 ||
        selectedTypes[0] === "task:DOCUMENT_APPROVAL"
    const compact = !tracking && !history
    const numberLabel = purchase ? "采购单号" : compact ? "事项" : "单号 / 事项"
    const partyLabel = compact
        ? purchase
            ? "供应商与摘要"
            : "往来方与摘要"
        : purchase
          ? "供应商"
          : "往来方 / 对象"
    const columns: ColumnDef<QueueTaskRow, unknown>[] = [
        {
            id: "number",
            header: () => columnTitle(numberLabel, FileTextIcon),
            enableHiding: false,
            enableSorting: false,
            meta: {
                width: "reference",
                label: numberLabel,
            },
            cell: ({ row: { original: row } }) => {
                const rework =
                    row.item.workItemType === "APPROVAL_REWORK" &&
                    row.item.status === "OPEN"
                const Icon = queueTypeIcon(
                    row.item.workItemType === "DOCUMENT_APPROVAL" || tracking
                        ? `approval:${row.item.businessObjectType}`
                        : `task:${row.item.workItemType}`,
                )
                return (
                    <div className="flex items-center gap-3 py-1">
                        <Icon
                            className="size-4 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <div className="min-w-0 space-y-1.5">
                            <div
                                className="num max-w-64 truncate font-medium"
                                title={row.presentation.number}
                            >
                                {row.presentation.number}
                            </div>
                            {rework && (
                                <StatusBadge
                                    label={row.presentation.typeLabel}
                                    tone="destructive"
                                />
                            )}
                            {((mixed && !rework) ||
                                row.item.searchMatches?.length) && (
                                <div
                                    className="max-w-64 truncate text-xs text-muted-foreground"
                                    title={row.item.searchMatches?.join("；")}
                                >
                                    {row.item.searchMatches?.[0] ||
                                        row.presentation.typeLabel}
                                </div>
                            )}
                        </div>
                    </div>
                )
            },
        },
        {
            id: "counterparty",
            header: () => columnTitle(partyLabel, Building2Icon),
            enableSorting: false,
            meta: {
                width: compact ? "flex" : "reference",
                label: partyLabel,
            },
            cell: ({ row: { original: row } }) =>
                compact ? (
                    <TaskOverview row={row} />
                ) : (
                    <div
                        className="max-w-72 truncate"
                        title={row.presentation.title}
                    >
                        {row.presentation.title}
                    </div>
                ),
        },
        {
            id: "source",
            header: () => columnTitle("来源销售单", LinkIcon),
            enableSorting: false,
            meta: { width: "reference", label: "来源销售单" },
            cell: ({ row }) => (
                <span className="num" title={row.original.source?.orderNo}>
                    {row.original.source?.orderNo || "—"}
                </span>
            ),
        },
        {
            id: "summary",
            header: () =>
                columnTitle(purchase ? "商品摘要" : "事项摘要", PackageIcon),
            enableSorting: false,
            meta: {
                width: "flex",
                label: purchase ? "商品摘要" : "事项摘要",
            },
            cell: ({ row: { original: row } }) => (
                <div
                    className={cn(
                        "max-w-80 truncate text-body-compact",
                        row.presentation.attention &&
                            "text-warning-soft-foreground",
                    )}
                    title={row.presentation.summary}
                >
                    {row.summaryPreview || "—"}
                </div>
            ),
        },
        {
            id: "metric",
            header: () =>
                columnTitle(
                    purchase ? "含税金额" : tracking ? "申请摘要" : "本次处理",
                    WalletCardsIcon,
                ),
            enableSorting: false,
            meta: {
                width: "amount",
                align: "end",
                numeric: true,
                label: purchase
                    ? "含税金额"
                    : tracking
                      ? "申请摘要"
                      : "本次处理",
            },
            cell: ({ row: { original: row } }) => (
                <div
                    className="space-y-1 whitespace-normal"
                    title={row.content.metric.detail}
                >
                    <div className="font-medium">
                        <WorkspaceMoney
                            value={row.content.metric.value}
                            display="exact"
                        />
                    </div>
                    {(!purchase ||
                        !["含税金额", "含税总额"].includes(
                            row.content.metric.label,
                        )) && (
                        <div className="text-xs text-muted-foreground">
                            {row.content.metric.label}
                        </div>
                    )}
                </div>
            ),
        },
        {
            id: "status",
            header: () =>
                columnTitle(
                    history ? "处理结果" : tracking ? "审批进度" : "当前节点",
                    CircleCheckIcon,
                ),
            enableSorting: false,
            meta: {
                width: "status",
                label: history
                    ? "处理结果"
                    : tracking
                      ? "审批进度"
                      : "当前节点",
            },
            cell: ({
                row: {
                    original: { item, content },
                },
            }) => {
                const closed = item.status !== "OPEN"
                const node = displayName(item.approval?.currentNodeLabel)
                return (
                    <div className="space-y-1 whitespace-normal">
                        {closed || tracking || isBlockedWorkItem(item) ? (
                            <StatusBadge
                                label={
                                    closed
                                        ? item.statusLabel
                                        : tracking
                                          ? content.status.label
                                          : "受阻"
                                }
                                tone={
                                    closed
                                        ? item.statusTone
                                        : isBlockedWorkItem(item)
                                          ? "warning"
                                          : content.status.tone
                                }
                            />
                        ) : (
                            <div className="line-clamp-2 text-body-compact">
                                {node || item.statusLabel}
                            </div>
                        )}
                        {tracking && !closed && node && (
                            <div className="line-clamp-2 text-xs text-muted-foreground">
                                {node}
                            </div>
                        )}
                        <WorkspaceFulfillmentBadges item={item} />
                    </div>
                )
            },
        },
        {
            id: "due",
            header: () => columnTitle("待办期限", CalendarClockIcon),
            enableSorting: false,
            meta: { width: "status", label: "待办期限" },
            cell: ({ row }) => <TaskDeadline item={row.original.item} />,
        },
        {
            id: "action",
            header: () => columnTitle("操作", ListTodoIcon),
            enableHiding: false,
            enableSorting: false,
            meta: { width: "quantity", align: "end", label: "操作" },
            cell: ({
                row: {
                    original: { item, presentation, processable },
                },
            }) => (
                <Button
                    id={`workspace-task-${toAutomationIdSegment(item.workItemId)}`}
                    data-testid={
                        item.workItemType === "PROCUREMENT_ORDER_CREATION"
                            ? `work-item-procurement-order-creation-${item.workItemId}`
                            : undefined
                    }
                    variant="ghost"
                    size="sm"
                    onClick={() => onSelect(item)}
                    aria-label={`${!tracking && !history && processable ? "处理" : "查看"}${presentation.number}`}
                >
                    {!tracking && !history && processable ? "处理" : "查看"}
                    <ChevronRightIcon aria-hidden="true" />
                </Button>
            ),
        },
    ]
    return compact
        ? columns.filter(
              (column) => column.id !== "summary" && column.id !== "metric",
          )
        : columns
}
