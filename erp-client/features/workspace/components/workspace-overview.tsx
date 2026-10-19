"use client"

import {
    ArrowRightIcon,
    BoxIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    ClipboardCheckIcon,
    FilePenLineIcon,
    InfoIcon,
    TruckIcon,
    UsersIcon,
    WalletIcon,
} from "lucide-react"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { useWorkspaceScroll } from "../hooks/use-workspace-scroll"
import { TYPE_META } from "@/features/workspace/api/work-item-meta"
import { isBlockedWorkItem } from "@/features/workspace/lib/work-item"
import type {
    TodayWorkspaceView,
    WorkspaceWorkItem,
} from "@/features/workspace/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

const FAMILY_STYLE = {
    approval: {
        icon: ClipboardCheckIcon,
        color: "bg-violet-soft/50 text-violet-soft-foreground",
    },
    procurement: {
        icon: BoxIcon,
        color: "bg-info-soft/50 text-info-soft-foreground",
    },
    fulfillment: {
        icon: TruckIcon,
        color: "bg-teal-soft/50 text-teal-soft-foreground",
    },
    finance: {
        icon: WalletIcon,
        color: "bg-cyan-soft/50 text-cyan-soft-foreground",
    },
    exception: {
        icon: CircleAlertIcon,
        color: "bg-orange-soft/50 text-orange-soft-foreground",
    },
}

const TYPE_GUIDANCE: Record<string, { title?: string; description: string }> = {
    DOCUMENT_APPROVAL: { description: "核对单据内容，完成审批决定" },
    PROCUREMENT_ORDER_CREATION: {
        title: "供给分配",
        description: "把销售需求分配到采购或库存",
    },
    PURCHASE_SOURCE_UPDATE: {
        title: "销售变更影响",
        description: "核对采购数量与交期，处理变更",
    },
    SUPPLIER_PORTAL_REVIEW: { description: "确认供应商提交的申请" },
    FULFILLMENT_OPERATION: { description: "办理收货、发货与交付" },
    CUSTOMER_ACCEPTANCE_REGISTRATION: { description: "核实客户验收结果并登记" },
    SUPPLIER_PAYMENT_EXECUTION: { description: "核对批准金额，登记付款与回单" },
    SALES_INVOICE_EXECUTION: { description: "核对开票要求，登记发票附件" },
    SUPPLIER_SETTLEMENT_REVIEW: { description: "核对供应商账单与结算差异" },
    PURCHASE_RETURN_REFUND: { description: "核对退货款项并登记处理结果" },
    PURCHASE_RETURN_INVOICE: { description: "核对退货红票并登记附件" },
    IMPORT_BUSINESS_CONFIRMATION: { description: "核对导入内容，确认业务范围" },
    INTEGRATION_RESULT_UNKNOWN: { description: "核实外部处理结果并补齐凭据" },
    BUSINESS_EXCEPTION: { description: "查看异常原因，完成对应处理" },
}

/** 分类名称沿用任务元数据；未注册类型不直接显示内部枚举。 */
export function workspaceCategoryLabel(type: string): string {
    return TYPE_GUIDANCE[type]?.title ?? TYPE_META[type]?.label ?? "其他待办"
}

/** 首页只展示服务端完整统计，优先事项沿用服务端队列顺序。 */
export function WorkspaceOverview({
    view,
    onOpenQueue,
    onSelect,
}: {
    view: TodayWorkspaceView
    onOpenQueue: (type?: string, due?: "today" | "overdue") => void
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    const scrollRef = useWorkspaceScroll<HTMLDivElement>("overview")
    const inbox = view.metrics.find((metric) => metric.key === "inbox")?.count
    const overdue = view.metrics.find(
        (metric) => metric.key === "overdue",
    )?.count
    const categories = Object.entries(view.workItemTypeCounts ?? {})
        .filter(([, count]) => count > 0)
        .sort(([left], [right]) => {
            const order = [
                "PROCUREMENT_ORDER_CREATION",
                "PURCHASE_SOURCE_UPDATE",
                "SUPPLIER_PORTAL_REVIEW",
                ...Object.keys(TYPE_META),
            ]
            const rank = (type: string) => {
                const index = order.indexOf(type)
                return index < 0 ? order.length : index
            }
            return rank(left) - rank(right) || left.localeCompare(right)
        })
    const priorityItems = view.items.slice(0, 5)

    return (
        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
            <section
                aria-labelledby="workspace-overview-title"
                className="pt-3 pb-6"
            >
                <h2
                    id="workspace-overview-title"
                    className="text-xl font-medium tracking-tight"
                >
                    待办概览
                </h2>
                <div className="mt-5 flex flex-wrap items-center gap-x-7 gap-y-3">
                    <OverviewMetric
                        label="待处理"
                        value={inbox}
                        id="workspace-overview-all"
                        onClick={() => onOpenQueue()}
                    />
                    <OverviewMetric
                        label="已超期"
                        value={overdue}
                        id="workspace-overview-overdue"
                        tone="text-destructive"
                        onClick={() => onOpenQueue(undefined, "overdue")}
                    />
                    <OverviewMetric
                        label="今日到期"
                        value={view.dueTodayCount}
                        id="workspace-overview-today"
                        tone="text-warning-soft-foreground"
                        onClick={() => onOpenQueue(undefined, "today")}
                    />
                </div>
            </section>

            {view.workItemTypeCounts == null ? (
                <div
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border p-5 text-sm text-muted-foreground"
                    role="status"
                >
                    <span>分类统计暂不可用，可进入全部待办继续处理。</span>
                    <Button
                        id="workspace-overview-unavailable-queue"
                        variant="outline"
                        onClick={() => onOpenQueue()}
                    >
                        查看全部待办
                    </Button>
                </div>
            ) : categories.length > 0 ? (
                <section
                    aria-label="我的任务分类"
                    className="grid gap-4 md:grid-cols-2 xl:grid-cols-3"
                >
                    {categories.map(([type, count]) => {
                        const family = TYPE_META[type]?.family ?? "exception"
                        const appearance = FAMILY_STYLE[family]
                        const Icon =
                            type === "PURCHASE_SOURCE_UPDATE"
                                ? FilePenLineIcon
                                : type === "SUPPLIER_PORTAL_REVIEW"
                                  ? UsersIcon
                                  : appearance.icon
                        const color =
                            type === "PURCHASE_SOURCE_UPDATE"
                                ? "bg-teal-soft/50 text-teal-soft-foreground"
                                : type === "SUPPLIER_PORTAL_REVIEW"
                                  ? "bg-violet-soft/50 text-violet-soft-foreground"
                                  : appearance.color
                        return (
                            <button
                                key={type}
                                id={`workspace-category-${toAutomationIdSegment(type)}`}
                                type="button"
                                onClick={() => onOpenQueue(type)}
                                aria-label={`${workspaceCategoryLabel(type)} ${count} 项，进入处理`}
                                className={cn(
                                    "group flex min-w-0 items-start gap-4 rounded-xl border border-border/60 bg-card p-5 text-left transition-colors hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:p-6",
                                    "text-foreground",
                                )}
                            >
                                <span
                                    className={cn(
                                        "flex size-10 shrink-0 items-center justify-center rounded-lg",
                                        color,
                                    )}
                                >
                                    <Icon
                                        className="size-6"
                                        aria-hidden="true"
                                    />
                                </span>
                                <span className="min-w-0 flex-1 text-foreground">
                                    <span className="block text-sm font-medium">
                                        {workspaceCategoryLabel(type)}
                                    </span>
                                    <span className="num mt-1 block text-2xl font-medium">
                                        {count.toLocaleString("zh-CN")}
                                    </span>
                                    <span className="mt-2 block text-sm text-muted-foreground">
                                        {TYPE_GUIDANCE[type]?.description ??
                                            "查看任务要求，完成对应处理"}
                                    </span>
                                    <span className="mt-4 flex items-center gap-2 text-sm font-medium">
                                        进入处理{" "}
                                        <ArrowRightIcon
                                            className="size-4 transition-transform group-hover:translate-x-1"
                                            aria-hidden="true"
                                        />
                                    </span>
                                </span>
                                <ChevronRightIcon
                                    className="mt-1 size-4 shrink-0 text-muted-foreground"
                                    aria-hidden="true"
                                />
                            </button>
                        )
                    })}
                </section>
            ) : null}

            <section
                aria-labelledby="workspace-priority-title"
                className="mt-8"
            >
                <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
                    <h3
                        id="workspace-priority-title"
                        className="text-base font-medium"
                    >
                        优先处理
                    </h3>
                    <Button
                        id="workspace-overview-view-all"
                        variant="ghost"
                        size="sm"
                        onClick={() => onOpenQueue()}
                    >
                        查看全部待办 <ArrowRightIcon data-icon="inline-end" />
                    </Button>
                </div>
                {priorityItems.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-tasks"
                        title="当前没有待处理事项"
                        description="新任务到达后会出现在这里。"
                    />
                ) : (
                    <div className="divide-y divide-border rounded-lg border border-border bg-card px-4 sm:px-5">
                        {priorityItems.map((item) => (
                            <PriorityRow
                                key={item.workItemId}
                                item={item}
                                onSelect={onSelect}
                            />
                        ))}
                    </div>
                )}
                <p className="mt-4 flex items-start gap-2 text-xs text-muted-foreground">
                    <InfoIcon className="size-4 shrink-0" aria-hidden="true" />
                    按超期与优先级展示前 5 项；点击分类可集中处理同类任务。
                </p>
            </section>
        </div>
    )
}

function OverviewMetric({
    label,
    value,
    id,
    tone,
    onClick,
}: {
    label: string
    value?: number
    id: string
    tone?: string
    onClick: () => void
}) {
    return (
        <button
            id={id}
            type="button"
            onClick={onClick}
            className="flex items-baseline gap-3 rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            aria-label={`${label} ${value ?? "暂不可用"} 项`}
        >
            <span className="text-sm text-muted-foreground">{label}</span>
            <span className={cn("num text-2xl font-medium", tone)}>
                {value?.toLocaleString("zh-CN") ?? "—"}
            </span>
        </button>
    )
}

function PriorityRow({
    item,
    onSelect,
}: {
    item: WorkspaceWorkItem
    onSelect: (item: WorkspaceWorkItem) => void
}) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const today = item.dueBucket === "today"
    const label = blocked
        ? "受阻"
        : overdue
          ? "已超期"
          : today
            ? "今日到期"
            : "待处理"
    return (
        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-4 py-5 lg:grid-cols-[6rem_10rem_minmax(0,1fr)_minmax(0,1fr)_auto]">
            <div className="hidden lg:block">
                <StatusBadge
                    label={label}
                    tone={
                        blocked || today
                            ? "warning"
                            : overdue
                              ? "destructive"
                              : "neutral"
                    }
                />
            </div>
            <div className="hidden text-sm font-medium lg:block">
                {item.workItemTypeLabel}
            </div>
            <div className="min-w-0">
                <div className="mb-1 flex flex-wrap items-center gap-2 lg:hidden">
                    <StatusBadge
                        label={label}
                        tone={
                            blocked || today
                                ? "warning"
                                : overdue
                                  ? "destructive"
                                  : "neutral"
                        }
                    />
                    <span className="text-xs text-muted-foreground">
                        {item.workItemTypeLabel}
                    </span>
                </div>
                <p
                    className="truncate text-sm font-medium"
                    title={item.counterpartyName || item.objectTitle}
                >
                    {item.counterpartyName || item.objectTitle}
                </p>
                <p className="num mt-1 truncate text-xs text-muted-foreground">
                    {item.stableNumber}
                </p>
            </div>
            <p className="hidden min-w-0 text-sm text-muted-foreground lg:line-clamp-2">
                {item.listSummary || item.reasonLabel || item.nextActionHint}
            </p>
            <Button
                id={`workspace-task-${toAutomationIdSegment(item.workItemId)}`}
                size="sm"
                onClick={() => onSelect(item)}
            >
                {blocked ? "查看原因" : "去处理"}
            </Button>
        </div>
    )
}
