"use client"

import { SystemFeedback } from "@/components/business/system-feedback"

import Link from "next/link"
import { useRouter } from "@/lib/navigation"
import {
    ArrowLeftIcon,
    CheckIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
    TriangleAlertIcon,
    UsersRoundIcon,
    type LucideIcon,
} from "lucide-react"

import { BusinessStatusBadge, FormalActionResult } from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    WorkspaceTaskFooter,
    useWorkspaceTaskPane,
} from "@/components/business/workspace-task-pane"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { responsibilityText } from "@/lib/ui-text"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import {
    WORK_ITEM_STATUS_LABEL,
    WORK_ITEM_TYPE_LABEL,
} from "@/features/supplier-orders/types"
import type { SupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

const RESPONSIBILITY_STATUS_META: Record<
    ResponsibilityStatus,
    { label: string; tone: StatusTone; icon: LucideIcon }
> = {
    assigned_to_me: {
        label: responsibilityText.assignedToMe,
        tone: "success",
        icon: ShieldCheckIcon,
    },
    assigned_to_other: {
        label: responsibilityText.assignedToOther,
        tone: "neutral",
        icon: UsersRoundIcon,
    },
    blocked: {
        label: responsibilityText.blocked,
        tone: "warning",
        icon: ShieldAlertIcon,
    },
    completed: {
        label: responsibilityText.completed,
        tone: "success",
        icon: CircleCheckIcon,
    },
    closed: {
        label: responsibilityText.closed,
        tone: "neutral",
        icon: CircleDashedIcon,
    },
}

export function WorkItemProcessPanel({
    workItem,
    workItemBlocker,
    responsibilityStatus,
    canCompleteTask,
    pending,
    onProcess,
}: {
    workItem?: SupplierOrderDetailView["workItem"]
    workItemBlocker?: SupplierOrderDetailView["workItemBlocker"]
    responsibilityStatus: ResponsibilityStatus
    canCompleteTask: boolean
    pending: boolean
    onProcess: () => void
}) {
    const router = useRouter()
    const inWorkspace = useWorkspaceTaskPane()
    const profile = useAccountProfileQuery()
    const canProcess = hasPermission(
        profile.data?.permissions,
        "supplier_fulfillment_order:complete",
    )
    const responsibility = RESPONSIBILITY_STATUS_META[responsibilityStatus]
    const processDisabled =
        responsibilityStatus !== "assigned_to_me" || !canCompleteTask || pending

    return (
        <>
            {workItemBlocker ? (
                <Alert variant="warning">
                    <AlertTitle>当前待办暂无法处理</AlertTitle>
                    <AlertDescription>
                        {workItemBlocker.message}
                    </AlertDescription>
                </Alert>
            ) : null}

            {workItem && canProcess ? (
                inWorkspace ? (
                    <WorkspaceTaskFooter>
                        <LoadingButton
                            id="supplier-order-center-complete-task"
                            type="button"
                            loading={pending}
                            disabled={processDisabled}
                            onClick={onProcess}
                        >
                            {!pending ? (
                                <CheckIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {pending ? "正在处理" : "确认处理结果并完成任务"}
                        </LoadingButton>
                    </WorkspaceTaskFooter>
                ) : (
                    <section
                        aria-label="关联待办处理操作"
                        className="flex flex-col gap-3 border-b border-border/60 pb-4 lg:flex-row lg:items-center lg:justify-between"
                    >
                        <div
                            className="flex min-w-0 flex-wrap items-center gap-2.5"
                            aria-live="polite"
                        >
                            <StatusBadge
                                tone={responsibility.tone}
                                icon={responsibility.icon}
                                label={responsibility.label}
                            />
                            {responsibilityStatus === "assigned_to_other" ? (
                                <span className="text-body-compact text-destructive">
                                    {responsibilityText.changed}
                                </span>
                            ) : null}
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                            <Button
                                id="supplier-order-center-process-bar-back"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={pending}
                                onClick={() => router.push("/workspace")}
                            >
                                <ArrowLeftIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                返回工作台
                            </Button>
                            <LoadingButton
                                id="supplier-order-center-complete-task"
                                type="button"
                                size="sm"
                                loading={pending}
                                disabled={processDisabled}
                                onClick={onProcess}
                            >
                                {!pending ? (
                                    <CheckIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                ) : null}
                                {pending
                                    ? "正在处理"
                                    : "确认处理结果并完成任务"}
                            </LoadingButton>
                        </div>
                    </section>
                )
            ) : null}
        </>
    )
}

export function StatusAlertsPanel({
    order,
    lastInvestigation,
    noQueryCapability,
    trackingError,
}: {
    order: SupplierOrderDetailView["order"]
    lastInvestigation?: SupplierOrderDetailView["lastInvestigation"]
    noQueryCapability: boolean
    trackingError?: string
}) {
    const isResultUnknown = order.fulfillmentStatus === "RESULT_UNKNOWN"
    const isCompletedPartialRefund =
        order.fulfillmentStatus === "COMPLETED" &&
        order.refundStatus === "PARTIAL"

    if (!trackingError && !isResultUnknown && !isCompletedPartialRefund) {
        return null
    }

    return (
        <>
            {trackingError ? (
                <>
                    <SystemFeedback
                        problemId={`supplier-order-tracking-${order.id}`}
                        error={trackingError}
                        title="供应商履约跟踪暂未完成"
                        description="当前订单和物流状态保留最近已确认的结果，请刷新执行状态或联系管理员。"
                    />
                    <p role="status" className="text-xs text-muted-foreground">
                        当前显示最近已确认的订单和物流状态。
                    </p>
                </>
            ) : null}

            {isResultUnknown ? (
                <Alert variant="warning" aria-live="polite">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>结果未知 — 请先查询原下单结果</AlertTitle>
                    <AlertDescription>
                        不得把结果未知直接改成成功，也不得在未查询前直接再次下单。
                        {lastInvestigation ? (
                            <span className="mt-1 block">
                                最近查询：
                                {lastInvestigation.outcomeLabel} —{" "}
                                {lastInvestigation.summary}
                                {lastInvestigation.canSafeRetry
                                    ? " · 已开放安全重发"
                                    : " · 重发未开放"}
                            </span>
                        ) : noQueryCapability ? (
                            <span className="mt-1 block">
                                尚未查询。该供应商无查询能力，请前往接口错误与对账中心人工处理。
                            </span>
                        ) : (
                            <span className="mt-1 block">
                                尚未查询。先执行「查询原下单结果」，确认无结果且系统允许后再重发。
                            </span>
                        )}
                    </AlertDescription>
                </Alert>
            ) : null}

            {isCompletedPartialRefund ? (
                <Alert variant="info">
                    <AlertTitle>已完成 + 部分退款</AlertTitle>
                    <AlertDescription>
                        履约与退款状态独立记录，互不覆盖。
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}

export function WorkItemCard({
    workItem,
    orderNo,
}: {
    workItem: NonNullable<SupplierOrderDetailView["workItem"]>
    orderNo: string
}) {
    return (
        <section
            className="space-y-2 border-t border-border/60 pt-4 first:border-t-0 first:pt-0"
            aria-label="关联待办任务"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h2 className="text-body-compact font-semibold text-foreground">
                    {WORK_ITEM_TYPE_LABEL[workItem.workItemType]}
                </h2>
                <BusinessStatusBadge
                    context="list"
                    label={WORK_ITEM_STATUS_LABEL[workItem.workItemStatus]}
                    tone={
                        workItem.workItemStatus === "COMPLETED"
                            ? "success"
                            : "info"
                    }
                />
            </div>
            <p className="text-xs text-muted-foreground">
                关联订单 <span className="num">{orderNo}</span>
                {workItem.ownerUser
                    ? ` · 当前处理人 ${workItem.ownerUser.displayName}`
                    : ""}
            </p>
            <p className="text-xs text-muted-foreground">
                查询原下单结果和再次提交不会完成任务；只有确认处理结果后才会完成待办。
            </p>
        </section>
    )
}

export function ResultPanel({
    result,
    order,
    costs,
    onClose,
}: {
    result: SupplierOrderCenterResult
    order: SupplierOrderDetailView["order"]
    costs: SupplierOrderDetailView["costs"]
    onClose: () => void
}) {
    const settlementHref = costs.settlementId
        ? `/supplier-api/settlements/${encodeURIComponent(costs.settlementId)}`
        : costs.settlementNo
          ? `/supplier-api/settlements?q=${encodeURIComponent(costs.settlementNo)}`
          : "/supplier-api/settlements"

    return (
        <FormalActionResult
            status={result.status}
            title={result.title}
            description={result.description}
            facts={result.facts}
            actions={
                <div className="flex flex-wrap gap-2">
                    <Button
                        id="supplier-order-center-result-close"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭结果
                    </Button>
                    {order.fulfillmentStatus === "COMPLETED" ||
                    costs.settlementId ? (
                        <Button
                            id="supplier-order-center-result-open-settlement"
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href={settlementHref} />}
                        >
                            {costs.settlementId
                                ? "查看所属结算单"
                                : "打开 API 结算"}
                        </Button>
                    ) : null}
                </div>
            }
        />
    )
}
