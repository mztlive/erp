"use client"

import * as React from "react"
import {
    AlertTriangleIcon,
    ArrowUpRightIcon,
    ChevronDownIcon,
    CircleHelpIcon,
    RefreshCwIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    INQUIRY_FEEDBACK_DRAFT_CHANGED,
    initialInquiryFeedback,
} from "@/features/sales-inquiries/components/inquiry-feedback-editor"
import {
    useInquiry,
    useInquirySkuDetailsQuery,
} from "@/features/sales-inquiries/hooks/queries"
import { feedbackIssues } from "@/features/sales-inquiries/lib/feedback-editing"
import {
    inquiryDateInput,
    inquiryDeadlineMeta,
} from "@/features/sales-inquiries/lib/format"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
} from "@/features/sales-inquiries/lib/local-draft"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedbackData,
    type ProcurementInquiry,
} from "@/features/sales-inquiries/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspaceProcurementInspectorProps } from "./workspace-procurement-inspector"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

type LineFilter = "all" | "pending" | "anomaly"

export function WorkspaceSalesInquiryInspector({
    item,
    onOpenFullPage,
    onCompleted,
    presentation,
    userId,
    permissions,
}: WorkspaceProcurementInspectorProps) {
    const profile = useAccountProfileQuery()
    const accountId = userId ?? profile.data?.userid ?? ""
    const granted = permissions ?? profile.data?.permissions ?? []
    const canRead = hasPermission(granted, "procurement_inquiry:detail")
    const canEdit = hasPermission(granted, "procurement_inquiry:save_feedback")
    const query = useInquiry(
        item.businessObjectId,
        true,
        item.workItemId,
        canRead,
    )
    const inquiry =
        canRead && query.data && "work_items" in query.data
            ? query.data
            : undefined
    const prefix = `workspace-inquiry-inspector-${toAutomationIdSegment(item.workItemId)}`
    const padClass = presentation === "sheet" ? "px-sheet-inline" : "px-5"
    const [draftRevision, setDraftRevision] = React.useState(0)
    const [hasLocalDraft, setHasLocalDraft] = React.useState(false)

    return (
        <div
            data-slot="workspace-sales-inquiry-inspector"
            className="@container/inquiry-inspector flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div
                data-slot="workspace-task-inspector-controls"
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/60 py-1.5 text-xs",
                    padClass,
                )}
            >
                <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-muted-foreground">
                        {hasLocalDraft
                            ? "询价反馈快览（含未提交草稿）"
                            : "询价反馈快览"}
                    </span>
                    {hasLocalDraft ? (
                        <StatusBadge
                            label="已暂存"
                            tone="info"
                            appearance="quiet"
                        />
                    ) : null}
                </div>
                <Button
                    id={`${prefix}-refresh`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-control-xs shrink-0 px-2 text-xs text-muted-foreground hover:text-foreground"
                    disabled={query.isFetching}
                    onClick={() => {
                        setDraftRevision((value) => value + 1)
                        void query.refetch()
                    }}
                >
                    <RefreshCwIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                        className={cn(query.isFetching && "animate-spin")}
                    />
                    刷新资料
                </Button>
            </div>
            <div
                className={cn(
                    "min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain py-4",
                    padClass,
                )}
            >
                {inquiry ? (
                    <InquiryInspectorPreview
                        key={`${accountId}:${item.workItemId}`}
                        item={item}
                        inquiry={inquiry}
                        taskId={item.workItemId}
                        userId={accountId}
                        canEdit={canEdit}
                        prefix={prefix}
                        draftRevision={draftRevision}
                        onHasLocalDraftChange={setHasLocalDraft}
                    />
                ) : profile.isError || query.isError ? (
                    <div className="flex min-h-full flex-col justify-center py-4">
                        <BusinessFailureState
                            idPrefix={`${prefix}-failure`}
                            title="询价资料读取失败"
                            error={
                                profile.isError ? profile.error : query.error
                            }
                            onRetry={() =>
                                void (profile.isError
                                    ? profile.refetch()
                                    : query.refetch())
                            }
                            action={
                                <Button
                                    id={`${prefix}-error-open-full`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={onOpenFullPage}
                                >
                                    在完整页面打开
                                    <ArrowUpRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            }
                        />
                    </div>
                ) : profile.isPending || (canRead && query.isPending) ? (
                    <div aria-busy="true" className="flex flex-col gap-4 py-1">
                        <span role="status" className="sr-only">
                            正在读取询价反馈…
                        </span>
                        <Skeleton className="h-4 w-56" />
                        <div className="grid grid-cols-2 gap-3 rounded-lg bg-surface-sunken p-3 @[520px]/inquiry-inspector:grid-cols-4">
                            <Skeleton className="h-10 w-full" />
                            <Skeleton className="h-10 w-full" />
                            <Skeleton className="h-10 w-full" />
                            <Skeleton className="h-10 w-full" />
                        </div>
                        <Skeleton className="h-14 w-full" />
                        <Skeleton className="h-14 w-full" />
                        <Skeleton className="h-14 w-full" />
                    </div>
                ) : (
                    <p className="py-4 text-body-compact text-muted-foreground">
                        当前无法查看询价反馈。
                    </p>
                )}
            </div>
            <footer
                data-slot="workspace-inspector-footer"
                className={cn(
                    "flex min-h-12 shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/60 bg-card py-2 shadow-footer",
                    padClass,
                )}
            >
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                    {presentation === "pane" && (
                        <WorkspaceTaskResponsibilityActions
                            item={item}
                            onCompleted={onCompleted}
                        />
                    )}
                </div>
                <Button
                    id={`${prefix}-open-full`}
                    type="button"
                    size="sm"
                    variant="default"
                    onClick={onOpenFullPage}
                >
                    进入完整作业面调整
                    <ArrowUpRightIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                    />
                </Button>
            </footer>
        </div>
    )
}

function InquiryInspectorPreview({
    item,
    inquiry,
    taskId,
    userId,
    canEdit,
    prefix,
    draftRevision,
    onHasLocalDraftChange,
}: {
    item: WorkspaceWorkItem
    inquiry: ProcurementInquiry
    taskId: string
    userId: string
    canEdit: boolean
    prefix: string
    draftRevision: number
    onHasLocalDraftChange: (hasDraft: boolean) => void
}) {
    const task = inquiry.work_items.find((workItem) => workItem.id === taskId)
    const storageKey =
        userId &&
        task?.status === "OPEN" &&
        canEdit &&
        (inquiry.status === "awaiting_procurement" ||
            inquiry.status === "procurement_processing")
            ? inquiryStorageKey(userId, `${inquiry.id}:${task.id}:feedback`)
            : null
    const [localDraft, setLocalDraft] = React.useState<{
        key: string
        data?: InquiryFeedbackData
    }>()
    React.useEffect(() => {
        if (!storageKey) {
            onHasLocalDraftChange(false)
            return
        }
        const sync = () => {
            try {
                const draft = readInquiryLocalValue<{
                    data: InquiryFeedbackData
                }>(storageKey)
                setLocalDraft({ key: storageKey, data: draft?.data })
                onHasLocalDraftChange(Boolean(draft?.data))
            } catch {
                setLocalDraft({ key: storageKey })
                onHasLocalDraftChange(false)
            }
        }
        const onStorage = (event: StorageEvent) => {
            if (event.key === null || event.key === storageKey) sync()
        }
        const onDraftChanged = (event: Event) => {
            if (event instanceof CustomEvent && event.detail === storageKey)
                sync()
        }
        sync()
        window.addEventListener("storage", onStorage)
        window.addEventListener(INQUIRY_FEEDBACK_DRAFT_CHANGED, onDraftChanged)
        window.addEventListener("focus", sync)
        return () => {
            window.removeEventListener("storage", onStorage)
            window.removeEventListener(
                INQUIRY_FEEDBACK_DRAFT_CHANGED,
                onDraftChanged,
            )
            window.removeEventListener("focus", sync)
        }
    }, [storageKey, draftRevision, onHasLocalDraftChange])

    const draft =
        storageKey && localDraft?.key === storageKey
            ? localDraft.data
            : undefined
    const baseline = task
        ? initialInquiryFeedback(inquiry, task, userId).data
        : undefined
    const feedback = new Map(
        baseline?.lines.map((line) => [line.line_id, line]),
    )
    for (const line of draft?.lines ?? []) feedback.set(line.line_id, line)

    const sku = useInquirySkuDetailsQuery(
        inquiry.draft.lines.map((line) => line.sku_id),
    )
    const [filter, setFilter] = React.useState<LineFilter>("all")
    const [expandedId, setExpandedId] = React.useState<string>()
    const rows = inquiry.draft.lines.map((line) => {
        const value = feedback.get(line.line_id)
        const pending = !value || value.conclusion === "pending"
        const issues = value && !pending ? feedbackIssues(value, line) : []
        return { line, value, pending, issues }
    })
    const pendingCount = rows.filter((row) => row.pending).length
    const anomalyCount = rows.filter((row) => row.issues.length > 0).length
    const readyCount = rows.length - pendingCount - anomalyCount
    const effectiveFilter =
        (filter === "pending" && !pendingCount) ||
        (filter === "anomaly" && !anomalyCount)
            ? "all"
            : filter
    const filtered = rows.filter(
        (row) =>
            effectiveFilter === "all" ||
            (effectiveFilter === "pending"
                ? row.pending
                : row.issues.length > 0),
    )
    const isActiveInquiry = !["confirmed", "closed"].includes(inquiry.status)
    const deadline = inquiryDeadlineMeta(inquiry.deadline_at, isActiveInquiry)
    const blocked = isBlockedWorkItem(item)
    const overdue = item.status === "OPEN" && item.dueBucket === "overdue"
    const showFilterPills = pendingCount > 0 || anomalyCount > 0
    const metrics = [
        { label: "商品", value: rows.length },
        {
            label: "已完善",
            value: readyCount,
            tone: "text-success-soft-foreground",
        },
        {
            label: "待回复",
            value: pendingCount,
            tone: "text-warning-soft-foreground",
        },
        { label: "异常", value: anomalyCount, tone: "text-destructive" },
    ]

    return (
        <div className="min-w-0 space-y-4">
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
                {overdue ? (
                    <StatusBadge label="已超期" tone="destructive" />
                ) : null}
                <DetailWorkspaceMeta
                    items={[
                        inquiry.inquiry_no ? (
                            <span
                                key="inquiry-no"
                                className="num select-all"
                                data-workspace-document-number={
                                    inquiry.inquiry_no
                                }
                            >
                                {inquiry.inquiry_no}
                            </span>
                        ) : null,
                        inquiry.requirement_version > 0 ? (
                            <span key="req-ver" className="num">
                                需求第 {inquiry.requirement_version} 版
                            </span>
                        ) : null,
                        inquiry.deadline_at > 0 ? (
                            <span
                                key="deadline"
                                className={cn(
                                    "num",
                                    deadline.tone === "destructive" &&
                                        "text-destructive",
                                    deadline.tone === "warning" &&
                                        "text-warning-soft-foreground",
                                )}
                            >
                                截止 {deadline.label}
                                {deadline.hint ? ` · ${deadline.hint}` : ""}
                            </span>
                        ) : null,
                        item.ownerUserLabel
                            ? `采购 ${item.ownerUserLabel}`
                            : null,
                    ]}
                />
            </div>

            {blocked && item.actionBlockers.length > 0 ? (
                <Alert variant="warning" className="border-l-2">
                    <AlertTriangleIcon aria-hidden="true" />
                    <AlertTitle>当前存在办理限制</AlertTitle>
                    <AlertDescription>
                        {item.actionBlockers
                            .map((blocker) => blocker.message)
                            .join("；")}
                    </AlertDescription>
                </Alert>
            ) : null}

            <div className="space-y-2 rounded-lg bg-surface-sunken px-3.5 py-3">
                <dl
                    aria-label="询价反馈汇总"
                    className="grid grid-cols-2 gap-x-3 gap-y-2 @[520px]/inquiry-inspector:grid-cols-4"
                >
                    {metrics.map(({ label, value, tone }) => (
                        <div key={label} className="min-w-0 space-y-0.5">
                            <dt className="flex items-center gap-1 text-xs text-muted-foreground">
                                {label}
                                {label === "已完善" ? (
                                    <Tooltip>
                                        <TooltipTrigger
                                            render={
                                                <Button
                                                    id={`${prefix}-kpi-formula`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon-xs"
                                                    aria-label="已完善指标口径"
                                                />
                                            }
                                        >
                                            <CircleHelpIcon aria-hidden="true" />
                                        </TooltipTrigger>
                                        <TooltipContent>
                                            已完善 =
                                            已选定最终供给结论且无缺漏或校验异常的商品行数。
                                        </TooltipContent>
                                    </Tooltip>
                                ) : null}
                            </dt>
                            <dd
                                className={cn(
                                    "num text-value-compact font-semibold",
                                    value > 0 && tone,
                                )}
                            >
                                {value}
                            </dd>
                        </div>
                    ))}
                </dl>
                {rows.length > 0 ? (
                    <div
                        aria-hidden="true"
                        className="flex h-1.5 overflow-hidden rounded-full bg-border"
                    >
                        {readyCount > 0 ? (
                            <span
                                className="bg-success"
                                style={{ flexGrow: readyCount }}
                            />
                        ) : null}
                        {pendingCount > 0 ? (
                            <span
                                className="bg-warning"
                                style={{ flexGrow: pendingCount }}
                            />
                        ) : null}
                        {anomalyCount > 0 ? (
                            <span
                                className="bg-destructive"
                                style={{ flexGrow: anomalyCount }}
                            />
                        ) : null}
                    </div>
                ) : null}
            </div>

            {showFilterPills ? (
                <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                    <div
                        role="group"
                        aria-label="按反馈状态筛选"
                        className={detailWorkspaceStyles.subTabs}
                    >
                        {(
                            [
                                {
                                    id: "all",
                                    label: "全部",
                                    count: rows.length,
                                },
                                {
                                    id: "pending",
                                    label: "待回复",
                                    count: pendingCount,
                                },
                                {
                                    id: "anomaly",
                                    label: "异常",
                                    count: anomalyCount,
                                },
                            ] as const
                        )
                            .filter(
                                (option) =>
                                    option.id === "all" || option.count > 0,
                            )
                            .map((option) => (
                                <button
                                    key={option.id}
                                    id={`${prefix}-filter-${option.id}`}
                                    type="button"
                                    aria-pressed={effectiveFilter === option.id}
                                    data-active={
                                        effectiveFilter === option.id
                                            ? "true"
                                            : undefined
                                    }
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => setFilter(option.id)}
                                >
                                    {option.label}
                                    <span className="num text-xs">
                                        {option.count}
                                    </span>
                                </button>
                            ))}
                    </div>
                </div>
            ) : null}

            {filtered.length === 0 ? (
                <p className="py-3 text-xs text-muted-foreground">
                    暂无商品需求
                </p>
            ) : (
                <ul className="divide-y divide-border/60 border-y border-border/60">
                    {filtered.map(({ line, value, issues, pending }) => {
                        const rowPrefix = `${prefix}-line-${toAutomationIdSegment(line.line_id)}`
                        const expanded = expandedId === line.line_id
                        const unit = sku.units.get(line.unit_code) ?? ""
                        const deliveryDate = inquiryDateInput(
                            line.delivery_due_at,
                        )
                        return (
                            <li key={line.line_id} className="min-w-0 py-2">
                                <button
                                    type="button"
                                    id={`${rowPrefix}-expand`}
                                    aria-expanded={expanded}
                                    aria-controls={`${rowPrefix}-preview`}
                                    className="flex w-full min-w-0 cursor-pointer items-start gap-2 rounded-md px-1 py-1 text-left text-body-compact hover:bg-surface-sunken focus-visible:outline-2 focus-visible:outline-ring"
                                    onClick={() =>
                                        setExpandedId(
                                            expanded ? undefined : line.line_id,
                                        )
                                    }
                                >
                                    <ChevronDownIcon
                                        aria-hidden="true"
                                        className={cn(
                                            "mt-0.5 size-3.5 shrink-0 text-muted-foreground",
                                            !expanded && "-rotate-90",
                                        )}
                                    />
                                    <span className="min-w-0 flex-1 space-y-0.5">
                                        <span className="block font-medium wrap-anywhere">
                                            {line.product_name}
                                        </span>
                                        <span className="flex flex-wrap items-baseline gap-x-2 text-xs text-muted-foreground">
                                            <span className="num wrap-anywhere">
                                                {line.sku_code}
                                            </span>
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={unit}
                                            />
                                            {deliveryDate && (
                                                <span className="num">
                                                    {deliveryDate}
                                                </span>
                                            )}
                                        </span>
                                    </span>
                                    <span
                                        className={cn(
                                            "shrink-0 rounded-sm px-1.5 py-0.5 text-xs",
                                            issues.length > 0
                                                ? "bg-destructive-soft text-destructive-soft-foreground"
                                                : pending
                                                  ? "bg-warning-soft text-warning-soft-foreground"
                                                  : "bg-success-soft text-success-soft-foreground",
                                        )}
                                    >
                                        {pending
                                            ? "待回复"
                                            : issues.length > 0
                                              ? "异常"
                                              : "已完善"}
                                    </span>
                                </button>
                                <div
                                    id={`${rowPrefix}-preview`}
                                    hidden={!expanded}
                                    className="space-y-2 pt-2 pl-6 text-xs text-muted-foreground"
                                >
                                    {expanded && (
                                        <>
                                            {line.specification &&
                                                line.specification !==
                                                    "无规格" && (
                                                    <p className="wrap-anywhere">
                                                        {line.specification}
                                                    </p>
                                                )}
                                            {value && (
                                                <dl className="flex flex-wrap gap-x-4 gap-y-1">
                                                    <div className="flex items-baseline gap-1">
                                                        <dt>供给结论</dt>
                                                        <dd>
                                                            {
                                                                INQUIRY_CONCLUSION_LABEL[
                                                                    value
                                                                        .conclusion
                                                                ]
                                                            }
                                                        </dd>
                                                    </div>
                                                    {!pending &&
                                                        value.conclusion !==
                                                            "unavailable" && (
                                                            <div className="flex items-baseline gap-1">
                                                                <dt>可供</dt>
                                                                <dd>
                                                                    <QuantityValue
                                                                        value={
                                                                            value.available_quantity
                                                                        }
                                                                        unit={
                                                                            unit
                                                                        }
                                                                    />
                                                                </dd>
                                                            </div>
                                                        )}
                                                    {value.prices[0]
                                                        ?.unit_price_gross && (
                                                        <div className="flex items-baseline gap-1">
                                                            <dt>含税单价</dt>
                                                            <dd>
                                                                <MoneyValue
                                                                    value={
                                                                        value
                                                                            .prices[0]
                                                                            .unit_price_gross
                                                                    }
                                                                    display="exact"
                                                                    showHint={
                                                                        false
                                                                    }
                                                                />
                                                            </dd>
                                                        </div>
                                                    )}
                                                </dl>
                                            )}
                                            {value?.public_note && (
                                                <p className="whitespace-pre-wrap wrap-anywhere">
                                                    {value.public_note}
                                                </p>
                                            )}
                                            {issues.length > 0 && (
                                                <ul className="list-disc space-y-0.5 pl-4 text-destructive-soft-foreground">
                                                    {issues.map((issue) => (
                                                        <li
                                                            key={`${issue.field}:${issue.message}`}
                                                        >
                                                            {issue.message}
                                                        </li>
                                                    ))}
                                                </ul>
                                            )}
                                        </>
                                    )}
                                </div>
                            </li>
                        )
                    })}
                </ul>
            )}
        </div>
    )
}
