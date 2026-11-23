"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { CalendarClockIcon, ClockAlertIcon } from "lucide-react"
import {
    BusinessStatusBadge,
    TableEmptyCell,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { displayName } from "@/lib/display-name"
import {
    getApprovalInstanceStatusMeta,
    getWorkItemStatusMeta,
} from "@/lib/task-status-dictionary"
import { workspaceDocumentBadge } from "../api/work-item-meta"
import {
    workspaceTaskPresentation,
    workspaceTaskSection,
} from "../lib/task-presentation"
import { workspaceTaskRightContent } from "../lib/task-right-content"
import { findSourceSalesOrder } from "../lib/source-sales-order"
import { isBlockedWorkItem } from "../lib/work-item"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceMoney } from "./workspace-money"

const PAYMENT_STATE_LABEL: Record<string, string> = {
    UNPAID: "未付款",
    PARTIAL: "部分付款",
    PAID: "已付清",
    NOT_REQUIRED: "无需付款",
}

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

/**
 * 紧凑单行时限列（遵循 status-badge.tsx:17：图标 + 短文字，零裸圆点、零多行堆叠）。
 */
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
    if (urgent) {
        const overdue = item.dueBucket === "overdue"
        return (
            <time
                dateTime={item.dueAt}
                title={`待办截止：${label}`}
                className="inline-flex items-center whitespace-nowrap"
            >
                <BusinessStatusBadge
                    appearance="text"
                    icon={overdue ? ClockAlertIcon : CalendarClockIcon}
                    label={overdue ? "超期" : "今日"}
                    tone={overdue ? "destructive" : "warning"}
                />
            </time>
        )
    }
    return (
        <time
            dateTime={item.dueAt}
            title={`待办截止：${label}`}
            className="num whitespace-nowrap text-muted-foreground"
        >
            {label}
        </time>
    )
}

/** 单行紧凑状态提示：将履约付款状态/节点信息收入 tooltip，确保每行仅展示 1 枚主 StatusBadge。 */
function buildStatusTooltip(
    label: string,
    node: string | undefined,
    item: WorkspaceWorkItem,
): string {
    const parts = [label]
    if (node && node !== label) parts.push(`当前节点：${node}`)
    const descriptor = workspaceFulfillmentDescriptor(item)
    if (descriptor && descriptor.operationTypes[0] !== "RECEIPT") {
        const payment = item.fulfillmentStatus?.payment_state
        if (descriptor.role === "procurement" && payment) {
            parts.push(`付款状态：${PAYMENT_STATE_LABEL[payment] ?? "待核对"}`)
        }
        const blocker = item.actionBlockers.find(
            (entry) => entry.action === "PROCESS" || entry.action === "POST",
        )
        if (blocker?.message) parts.push(blocker.message)
    }
    return parts.join(" · ")
}

/** 单行紧凑对象列：往来单位/标题 + 可选行内精简摘要，严控 36px 单行高度。 */
function CompactTaskOverview({
    row,
    inlineSummary,
}: {
    row: QueueTaskRow
    inlineSummary: boolean
}) {
    const fullTitle = [row.presentation.title, row.presentation.summary]
        .filter(Boolean)
        .join(" — ")
    if (row.retryMaterials) {
        return (
            <div className="flex max-w-table-column-reference min-w-0 items-center gap-2">
                <span
                    className="truncate font-medium text-foreground"
                    title={row.presentation.title}
                >
                    {row.presentation.title}
                </span>
                <Button
                    id={`workspace-materials-retry-${toAutomationIdSegment(row.item.workItemId)}`}
                    variant="link"
                    size="xs"
                    className="h-auto shrink-0 p-0 text-xs"
                    onClick={(event) => {
                        event.stopPropagation()
                        row.retryMaterials?.()
                    }}
                >
                    重新加载
                </Button>
            </div>
        )
    }
    return (
        <div
            className="flex max-w-table-column-reference min-w-0 items-center gap-1.5"
            title={fullTitle || undefined}
        >
            <span className="max-w-56 shrink-0 truncate font-medium text-foreground">
                {row.presentation.title}
            </span>
            {inlineSummary && row.summaryPreview ? (
                <span
                    className={cn(
                        "min-w-0 truncate text-xs text-muted-foreground",
                        row.presentation.attention &&
                            "font-medium text-warning-soft-foreground",
                    )}
                >
                    {row.summaryPreview}
                </span>
            ) : null}
        </div>
    )
}

/**
 * 紧凑单行任务列定义（36px 行高）：
 * - 单号 / 类型 / 往来单位 / 金额 / 主状态（1 枚 StatusBadge）/ 期限（图标+短文字）/ 操作全部单行排列；
 * - 彻底消除垂直多徽章堆叠导致的 60–84px 行高膨胀。
 */
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
    const numberLabel = purchase ? "采购单号" : "单号"
    const partyLabel = compact
        ? purchase
            ? "供应商与摘要"
            : "客户/供应商与摘要"
        : purchase
          ? "供应商"
          : "客户/供应商"

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
                const { number, typeLabel } = row.presentation
                const matches = row.item.searchMatches
                const tooltip = [
                    number !== "—" ? number : undefined,
                    typeLabel,
                    matches?.length ? `命中：${matches.join("；")}` : undefined,
                ]
                    .filter(Boolean)
                    .join(" · ")
                return number === "—" ? (
                    <TableEmptyCell />
                ) : (
                    <span
                        className="num block max-w-table-column-default truncate font-medium text-foreground"
                        title={tooltip}
                    >
                        {number}
                    </span>
                )
            },
        },
        {
            id: "type",
            header: "类型",
            enableSorting: false,
            meta: {
                width: "status",
                label: "类型",
            },
            cell: ({ row: { original: row } }) => {
                const rework =
                    row.item.workItemType === "APPROVAL_REWORK" &&
                    row.item.status === "OPEN"
                const badge = workspaceDocumentBadge(
                    row.item.workItemType,
                    row.item.businessObjectType,
                    row.presentation.typeLabel,
                )
                const shortLabel = rework ? "退回重办" : badge.label
                return (
                    <span
                        className={cn(
                            "block max-w-table-column-status truncate text-xs",
                            rework
                                ? "font-medium text-destructive"
                                : "text-muted-foreground",
                        )}
                        title={row.presentation.typeLabel}
                    >
                        {shortLabel}
                    </span>
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
                <CompactTaskOverview
                    row={row.original}
                    inlineSummary={compact}
                />
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
            header: purchase ? "含税金额" : tracking ? "金额" : "本次金额",
            enableSorting: false,
            meta: {
                width: "amount",
                align: "end",
                numeric: true,
                label: purchase ? "含税金额" : tracking ? "金额" : "本次金额",
            },
            cell: ({ row: { original: row } }) => {
                const metric = row.content.metric
                const tooltip = row.materialNotice
                    ? "业务资料"
                    : `${metric.label}：${metric.detail ?? metric.value}`
                return (
                    <div
                        className="num ml-auto max-w-table-column-amount truncate font-medium text-foreground whitespace-nowrap"
                        title={tooltip}
                    >
                        {row.materialNotice || metric.value === "—" ? (
                            <TableEmptyCell />
                        ) : (
                            <WorkspaceMoney
                                value={metric.value}
                                display="exact"
                            />
                        )}
                    </div>
                )
            },
        },
        {
            id: "status",
            header: history ? "处理结果" : tracking ? "审批状态" : "状态",
            enableSorting: false,
            meta: {
                width: "status",
                label: history ? "处理结果" : tracking ? "审批状态" : "状态",
            },
            cell: ({
                row: {
                    original: { item },
                },
            }) => {
                const closed = item.status !== "OPEN"
                const blocked = isBlockedWorkItem(item)
                const node = displayName(item.approval?.currentNodeLabel)
                const approvalMeta = tracking
                    ? getApprovalInstanceStatusMeta(item.approval?.status)
                    : undefined
                const workItemMeta = getWorkItemStatusMeta(
                    item.status,
                    blocked
                        ? item.processingState === "READY"
                            ? "APPROVAL_BLOCKED"
                            : item.processingState
                        : undefined,
                )
                const tone = tracking ? approvalMeta!.tone : workItemMeta.tone
                const label = tracking
                    ? approvalMeta!.label
                    : closed || blocked
                      ? workItemMeta.label
                      : node || workItemMeta.label
                return (
                    <BusinessStatusBadge
                        label={label}
                        tone={tone}
                        appearance={
                            tone === "success" ||
                            (!closed && !tracking && !blocked)
                                ? "quiet"
                                : "badge"
                        }
                        title={buildStatusTooltip(label, node, item)}
                        className="max-w-table-column-status [&>svg]:shrink-0"
                    />
                )
            },
        },
        {
            id: "due",
            header: "时限",
            enableSorting: false,
            meta: { width: "status", label: "时限" },
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
                        size="xs"
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

    if (payment) {
        columns.splice(
            3,
            0,
            {
                id: "payerCompany",
                header: "付款主体公司",
                enableSorting: false,
                meta: { width: "default", label: "付款主体公司" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={
                            workspaceTaskSection(
                                row.original.item,
                                "我方付款主体",
                            )?.value
                        }
                        clamp={1}
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
    }

    return compact
        ? columns.filter(
              (column) =>
                  column.id !== "summary" &&
                  (!payment || column.id !== "metric"),
          )
        : columns
}
