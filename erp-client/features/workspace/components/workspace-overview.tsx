"use client"

import {
    ArrowRightIcon,
    BoxIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    ClipboardCheckIcon,
    FilePenLineIcon,
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
import { workspaceTaskPresentation } from "../lib/task-presentation"
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
    onOpenManaged,
    onSelect,
}: {
    view: TodayWorkspaceView
    onOpenQueue: (type?: string, due?: "today" | "overdue") => void
    onOpenManaged: () => void
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
    const managed = view.metrics.find(
        (metric) => metric.key === "managed" && metric.visible,
    )
    const empty =
        inbox === 0 &&
        view.total === 0 &&
        priorityItems.length === 0 &&
        view.workItemTypeCounts != null &&
        categories.length === 0

    return (
        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
            {empty ? (
                <div className="space-y-4 pt-2">
                    <section
                        aria-labelledby="workspace-empty-title"
                        className="flex min-h-40 items-center gap-5 rounded-xl border border-border/60 bg-muted/15 px-5 py-8 sm:px-8"
                    >
                        <span className="flex size-12 shrink-0 items-center justify-center rounded-full bg-success-soft text-success-soft-foreground sm:size-14">
                            <CircleCheckIcon
                                className="size-6 sm:size-7"
                                aria-hidden="true"
                            />
                        </span>
                        <div className="min-w-0">
                            <p className="mb-2 text-xs text-muted-foreground">
                                待我处理
                            </p>
                            <h2
                                id="workspace-empty-title"
                                className="text-lg font-medium tracking-tight sm:text-xl"
                            >
                                当前暂无待你处理的事项
                            </h2>
                            <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
                                新的待办到达后，会显示在这里。
                            </p>
                        </div>
                    </section>
                    {managed && (
                        <section
                            aria-label="范围内待办"
                            className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-border/60 bg-card px-5 py-5 sm:px-8"
                        >
                            <div className="min-w-0">
                                <p className="flex flex-wrap items-baseline gap-3 text-sm font-medium">
                                    {managed.label}
                                    <span className="num text-muted-foreground">
                                        {managed.count.toLocaleString("zh-CN")}{" "}
                                        项
                                    </span>
                                </p>
                                <p className="mt-1.5 text-sm text-muted-foreground">
                                    你可查看的业务范围内的待办事项。
                                </p>
                            </div>
                            <Button
                                id="workspace-overview-open-managed"
                                variant="outline"
                                size="sm"
                                onClick={onOpenManaged}
                            >
                                查看待办{" "}
                                <ArrowRightIcon data-icon="inline-end" />
                            </Button>
                        </section>
                    )}
                </div>
            ) : (
                <div className="space-y-6 pt-2">
                    <section
                        aria-label="待办概览"
                        className="flex flex-wrap items-center justify-between gap-4 rounded-xl bg-muted/30 px-5 py-5 sm:px-6"
                    >
                        <div className="grid w-full max-w-xl grid-cols-3 gap-4 sm:gap-8">
                            <OverviewMetric
                                label="待我处理"
                                value={inbox}
                                id="workspace-overview-all"
                                onClick={() => onOpenQueue()}
                            />
                            <OverviewMetric
                                label="已超期"
                                value={overdue}
                                id="workspace-overview-overdue"
                                tone="text-destructive"
                                onClick={() =>
                                    onOpenQueue(undefined, "overdue")
                                }
                            />
                            <OverviewMetric
                                label="今日到期"
                                value={view.dueTodayCount}
                                id="workspace-overview-today"
                                tone="text-warning-soft-foreground"
                                onClick={() => onOpenQueue(undefined, "today")}
                            />
                        </div>
                        {managed && (
                            <Button
                                id="workspace-overview-open-managed"
                                variant="ghost"
                                size="sm"
                                onClick={onOpenManaged}
                            >
                                {managed.label}
                                <span className="num text-muted-foreground">
                                    {managed.count}
                                </span>
                                <ArrowRightIcon data-icon="inline-end" />
                            </Button>
                        )}
                    </section>
                    <section
                        aria-labelledby="workspace-priority-title"
                        className="overflow-hidden rounded-xl border border-border/60 bg-card"
                    >
                        <div className="space-y-4 border-b border-border/60 px-5 py-5 sm:px-6">
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <div>
                                    <h2
                                        id="workspace-priority-title"
                                        className="text-base font-medium"
                                    >
                                        优先处理
                                    </h2>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        {view.total > 5
                                            ? "按超期与优先级排序，展示前 5 项"
                                            : "按超期与优先级排序"}
                                    </p>
                                </div>
                                <Button
                                    id="workspace-overview-view-all"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => onOpenQueue()}
                                >
                                    查看全部待办{" "}
                                    <ArrowRightIcon data-icon="inline-end" />
                                </Button>
                            </div>
                            {view.workItemTypeCounts == null ? (
                                <div
                                    className="flex flex-wrap items-center justify-between gap-3 text-sm text-muted-foreground"
                                    role="status"
                                >
                                    <span>
                                        分类统计暂不可用，可进入全部待办继续处理。
                                    </span>
                                    <Button
                                        id="workspace-overview-unavailable-queue"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => onOpenQueue()}
                                    >
                                        查看全部待办
                                    </Button>
                                </div>
                            ) : categories.length > 0 ? (
                                <nav
                                    aria-label="按任务分类查看"
                                    className="flex flex-wrap gap-2"
                                >
                                    {categories.map(([type, count]) => (
                                        <CategoryShortcut
                                            key={type}
                                            type={type}
                                            count={count}
                                            onClick={() => onOpenQueue(type)}
                                        />
                                    ))}
                                </nav>
                            ) : null}
                        </div>
                        {priorityItems.length === 0 ? (
                            <BusinessEmptyState
                                kind="no-tasks"
                                title="当前没有待处理事项"
                                description="新任务到达后会出现在这里。"
                            />
                        ) : (
                            <div className="divide-y divide-border/60">
                                {priorityItems.map((item) => (
                                    <PriorityRow
                                        key={item.workItemId}
                                        item={item}
                                        onSelect={onSelect}
                                    />
                                ))}
                            </div>
                        )}
                    </section>
                </div>
            )}
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
            className="group flex min-w-0 flex-col items-start gap-1.5 rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            aria-label={`${label} ${value ?? "暂不可用"} 项`}
        >
            <span className="text-xs text-muted-foreground transition-colors group-hover:text-foreground">
                {label}
            </span>
            <span
                className={cn(
                    "num text-2xl font-medium",
                    value ? tone : "text-muted-foreground",
                )}
            >
                {value?.toLocaleString("zh-CN") ?? "—"}
            </span>
        </button>
    )
}

function CategoryShortcut({
    type,
    count,
    onClick,
}: {
    type: string
    count: number
    onClick: () => void
}) {
    const appearance = FAMILY_STYLE[TYPE_META[type]?.family ?? "exception"]
    const Icon =
        type === "PURCHASE_SOURCE_UPDATE"
            ? FilePenLineIcon
            : type === "SUPPLIER_PORTAL_REVIEW"
              ? UsersIcon
              : appearance.icon
    return (
        <button
            id={`workspace-category-${toAutomationIdSegment(type)}`}
            type="button"
            onClick={onClick}
            aria-label={`${workspaceCategoryLabel(type)} ${count} 项，进入处理`}
            title={TYPE_GUIDANCE[type]?.description}
            className="group inline-flex max-w-full items-center gap-2 rounded-lg border border-border/60 px-3 py-2 text-sm transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
            <Icon
                className="size-4 shrink-0 text-muted-foreground"
                aria-hidden="true"
            />
            <span className="min-w-0 truncate">
                {workspaceCategoryLabel(type)}
            </span>
            <span className="num rounded bg-muted px-1.5 text-xs font-medium">
                {count.toLocaleString("zh-CN")}
            </span>
            <ChevronRightIcon
                className="size-3.5 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5"
                aria-hidden="true"
            />
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
    const presentation = workspaceTaskPresentation(item)
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const today = item.dueBucket === "today"
    const attentionLabel = blocked
        ? "受阻"
        : overdue
          ? "已超期"
          : today
            ? "今日到期"
            : undefined
    return (
        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-5 gap-y-2 px-5 py-5 transition-colors hover:bg-muted/15 sm:px-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)_auto] lg:gap-x-8">
            <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                    <p
                        className="min-w-0 truncate text-sm font-medium"
                        title={presentation.title}
                    >
                        {presentation.title}
                    </p>
                    {attentionLabel && (
                        <StatusBadge
                            label={attentionLabel}
                            tone={blocked || today ? "warning" : "destructive"}
                        />
                    )}
                </div>
                <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                    <span>{presentation.typeLabel}</span>
                    <span aria-hidden="true">·</span>
                    <span className="num break-all">{presentation.number}</span>
                </div>
            </div>
            {presentation.summary && (
                <p
                    className={cn(
                        "col-start-1 row-start-2 line-clamp-2 min-w-0 text-sm leading-relaxed lg:col-start-2 lg:row-start-1",
                        presentation.attention
                            ? "text-warning-soft-foreground"
                            : "text-muted-foreground",
                    )}
                    title={presentation.summary}
                >
                    {presentation.summary}
                </p>
            )}
            <Button
                id={`workspace-task-${toAutomationIdSegment(item.workItemId)}`}
                size="sm"
                variant="outline"
                className="col-start-2 row-start-1 lg:col-start-3"
                onClick={() => onSelect(item)}
            >
                {blocked ? "查看原因" : "去处理"}
                <ArrowRightIcon data-icon="inline-end" />
            </Button>
        </div>
    )
}
