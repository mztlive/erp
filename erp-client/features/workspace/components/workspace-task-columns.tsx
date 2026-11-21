"use client"

import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessStatusBadge,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { displayName } from "@/lib/display-name"
import {
    workspaceTaskSection,
    workspaceTaskPresentation,
} from "../lib/task-presentation"
import { workspaceTaskRightContent } from "../lib/task-right-content"
import { findSourceSalesOrder } from "../lib/source-sales-order"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceMoney } from "./workspace-money"
import { WorkspaceFulfillmentBadges } from "./workspace-fulfillment-badges"

/** 每行使用独立授权且版本匹配的材料；失败时保留任务与操作入口。 */
export function queueTaskRow(
    item: WorkspaceWorkItem,
    materialNotice?: string,
    retryMaterials?: () => void,
) {
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
        materialNotice,
        retryMaterials,
        summaryPreview: materialNotice ? undefined : summaryPreview,
        presentation: materialNotice
            ? {
                  ...presentation,
                  number: "—",
                  title: materialNotice,
                  summary: undefined,
              }
            : presentation,
        processable:
            item.status === "OPEN" &&
            !isBlockedWorkItem(item) &&
            item.allowedActions.some((action) =>
                ["PROCESS", "OPEN_DOCUMENT", "APPROVE", "REJECT"].includes(
                    action,
                ),
            ),
        source: materialNotice
            ? undefined
            : findSourceSalesOrder(item.summarySections),
        content: workspaceTaskRightContent(
            item,
            presentation.amount,
            presentation.approvalProgress,
        ),
    }
}
export type QueueTaskRow = ReturnType<typeof queueTaskRow>

/** 只显示任务截止时间；商品交期留在摘要，已完成事项不再标为超期。无截止时间时显示接收时间，避免空折号。 */
function TaskDeadline({ item }: { item: WorkspaceWorkItem }) {
    if (!item.dueAt || Number.isNaN(new Date(item.dueAt).getTime())) {
        if (item.enteredAtLabel && item.enteredAtLabel !== "—") {
            return (
                <span
                    className="num whitespace-nowrap text-muted-foreground"
                    title={`进入待办时间：${item.enteredAtLabel}`}
                >
                    {item.enteredAtLabel}
                </span>
            )
        }
        return <TableEmptyCell />
    }
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
            className="num whitespace-nowrap"
        >
            {urgent ? (
                <BusinessStatusBadge
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

/** 待办展示对象与摘要，金额独立为指标列，保持主次层次分明。 */
function TaskOverview({
    row,
    showSummary,
}: {
    row: QueueTaskRow
    showSummary: boolean
}) {
    return (
        <TableIdentityCell
            className="max-w-table-column-reference"
            primary={row.presentation.title}
            primaryTitle={row.presentation.title}
            secondaryTitle={row.presentation.summary}
            secondary={
                row.retryMaterials ? (
                    <Button
                        id={`workspace-materials-retry-${toAutomationIdSegment(row.item.workItemId)}`}
                        variant="link"
                        size="sm"
                        className="h-auto p-0 text-xs"
                        onClick={(event) => {
                            event.stopPropagation()
                            row.retryMaterials?.()
                        }}
                    >
                        重新加载资料
                    </Button>
                ) : !showSummary ? undefined : row.summaryPreview ? (
                    <span
                        className={cn(
                            row.presentation.attention &&
                                "font-medium text-warning-soft-foreground",
                        )}
                    >
                        {row.summaryPreview}
                    </span>
                ) : row.materialNotice ? undefined : (
                    <TableEmptyCell />
                )
            }
        />
    )
}

/** 待办采用摘要布局，采购付款另列主体和金额；审批跟踪和处理记录保留独立指标。 */
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
    const payment =
        selectedTypes.length === 1 &&
        selectedTypes[0] === "task:SUPPLIER_PAYMENT_EXECUTION"
    const compact = !tracking && !history
    const numberLabel = purchase ? "采购单号" : "单号 / 事项"
    const partyLabel = compact
        ? purchase
            ? "供应商与摘要"
            : "客户/供应商与摘要"
        : purchase
          ? "供应商"
          : "客户/供应商 / 关联单据"
    const columns: ColumnDef<QueueTaskRow, unknown>[] = [
        {
            id: "number",
            header: numberLabel,
            enableHiding: false,
            enableSorting: false,
            meta: {
                width: "default",
                label: numberLabel,
            },
            cell: ({ row: { original: row } }) => {
                const rework =
                    row.item.workItemType === "APPROVAL_REWORK" &&
                    row.item.status === "OPEN"
                const searchMatches = row.item.searchMatches
                const { number, typeLabel } = row.presentation
                return (
                    <TableIdentityCell
                        className="max-w-table-column-default"
                        primary={
                            number === "—" ? (
                                <TableEmptyCell />
                            ) : (
                                <span className="num">{number}</span>
                            )
                        }
                        primaryTitle={number}
                        secondary={
                            rework ? (
                                <BusinessStatusBadge
                                    label={typeLabel}
                                    tone="destructive"
                                />
                            ) : (
                                searchMatches?.[0] || typeLabel
                            )
                        }
                        secondaryTitle={
                            rework
                                ? typeLabel
                                : searchMatches?.join("；") || typeLabel
                        }
                    />
                )
            },
        },
        {
            id: "counterparty",
            header: partyLabel,
            enableSorting: false,
            meta: {
                width: compact ? "flex" : "default",
                label: partyLabel,
            },
            cell: ({ row }) => (
                <TaskOverview row={row.original} showSummary={compact} />
            ),
        },
        {
            id: "source",
            header: "来源销售单",
            enableSorting: false,
            meta: { width: "default", label: "来源销售单" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.source?.orderNo}
                    clamp={1}
                    numeric
                />
            ),
        },
        {
            id: "summary",
            header: purchase ? "商品摘要" : "事项摘要",
            enableSorting: false,
            meta: {
                width: "flex",
                label: purchase ? "商品摘要" : "事项摘要",
            },
            cell: ({ row: { original: row } }) => (
                <TableTextCell
                    value={row.summaryPreview}
                    clamp={1}
                    title={row.presentation.summary}
                    className={cn(
                        "max-w-table-column-reference",
                        row.presentation.attention &&
                            "text-warning-soft-foreground",
                    )}
                />
            ),
        },
        {
            id: "metric",
            header: purchase ? "含税金额" : tracking ? "申请摘要" : "本次处理",
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
                    className="ml-auto max-w-table-column-amount truncate whitespace-nowrap"
                    title={
                        row.materialNotice
                            ? undefined
                            : (row.content.metric.detail ??
                              row.content.metric.value)
                    }
                >
                    <div className="num truncate font-medium text-foreground">
                        {row.materialNotice ||
                        row.content.metric.value === "—" ? (
                            <TableEmptyCell />
                        ) : (
                            <WorkspaceMoney
                                value={row.content.metric.value}
                                display="exact"
                            />
                        )}
                    </div>
                    {(!purchase ||
                        !["含税金额", "含税总额"].includes(
                            row.content.metric.label,
                        )) && (
                        <div className="text-xs text-muted-foreground">
                            {row.materialNotice
                                ? "业务资料"
                                : row.content.metric.label}
                        </div>
                    )}
                </div>
            ),
        },
        {
            id: "status",
            header: history ? "处理结果" : tracking ? "审批进度" : "当前节点",
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
                const blocked = isBlockedWorkItem(item)
                const node = displayName(item.approval?.currentNodeLabel)
                const tone = closed
                    ? item.statusTone
                    : blocked
                      ? "warning"
                      : tracking
                        ? content.status.tone
                        : "info"
                const label = closed
                    ? item.statusLabel
                    : tracking
                      ? content.status.label
                      : blocked
                        ? "受阻"
                        : node || item.statusLabel
                return (
                    <div className="flex flex-col items-start gap-1 whitespace-normal">
                        <BusinessStatusBadge
                            label={label}
                            tone={tone}
                            appearance={
                                tone === "success" ||
                                (!closed && !tracking && !blocked)
                                    ? "quiet"
                                    : "badge"
                            }
                            title={label}
                            className="max-w-table-column-default [&>svg]:shrink-0"
                        />
                        {tracking && !closed && node && (
                            <TableTextCell
                                value={node}
                                clamp={2}
                                className="text-xs text-muted-foreground"
                            />
                        )}
                        <WorkspaceFulfillmentBadges item={item} />
                    </div>
                )
            },
        },
        {
            id: "due",
            header: "待办期限",
            enableSorting: false,
            meta: { width: "status", label: "待办期限" },
            cell: ({ row }) => <TaskDeadline item={row.original.item} />,
        },
        {
            id: "action",
            header: "操作",
            enableHiding: false,
            enableSorting: false,
            meta: { width: "status", align: "end", label: "操作" },
            cell: ({
                row: {
                    original: { item, presentation, processable },
                },
            }) => {
                const canProcess = !tracking && !history && processable
                return (
                    <Button
                        id={`workspace-task-${toAutomationIdSegment(item.workItemId)}`}
                        data-testid={
                            item.workItemType === "PROCUREMENT_ORDER_CREATION"
                                ? `work-item-procurement-order-creation-${item.workItemId}`
                                : undefined
                        }
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-control-sm px-2 text-body-compact font-normal text-muted-foreground group-hover/row:text-primary group-focus-within/row:text-primary hover:bg-sidebar-accent/70 hover:text-primary"
                        onClick={(event) => {
                            event.stopPropagation()
                            onSelect(item)
                        }}
                        aria-label={`${canProcess ? "处理" : "查看"}${presentation.number}`}
                    >
                        {canProcess ? "处理" : "查看"}
                    </Button>
                )
            },
        },
    ]
    if (payment)
        columns.splice(
            2,
            0,
            {
                id: "payerCompany",
                header: "付款主体公司名称",
                enableSorting: false,
                meta: { width: "default", label: "付款主体公司名称" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={
                            workspaceTaskSection(
                                row.original.item,
                                "我方付款主体",
                            )?.value
                        }
                        clamp={2}
                        className="whitespace-normal"
                    />
                ),
            },
            {
                id: "paymentAmount",
                header: "付款金额",
                enableSorting: false,
                meta: {
                    width: "amount",
                    label: "付款金额",
                    numeric: true,
                    align: "end",
                },
                cell: ({ row }) => {
                    const value = workspaceTaskSection(
                        row.original.item,
                        "待付金额",
                    )?.value
                    return value && value !== "—" ? (
                        <span title="当前剩余待付金额" className="num">
                            <WorkspaceMoney value={value} display="exact" />
                        </span>
                    ) : (
                        <TableEmptyCell />
                    )
                },
            },
        )
    return compact
        ? columns.filter(
              (column) =>
                  column.id !== "summary" &&
                  (!payment || column.id !== "metric"),
          )
        : columns
}
