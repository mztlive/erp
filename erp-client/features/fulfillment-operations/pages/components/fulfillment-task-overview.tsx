"use client"

import Link from "next/link"
import { ArrowDownIcon, CircleAlertIcon } from "lucide-react"
import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { salesOrderHref } from "../lib/gate-copy"
import { sourceContextFields } from "../lib/presentation"
import { displayText } from "../../lib/readable-label"
import { paymentReminder } from "../../lib/validation"
import type { FulfillmentOperation } from "../../types"
import { FulfillmentPaymentLink } from "./fulfillment-payment-notice"
import type { fulfillmentBatchQuantities } from "./fulfillment-task-quantities"

export type FulfillmentTaskContext = {
    id: string
    blockers: readonly string[]
    impact?: string
    commitment?: { label: string; value: string }
    owner?: string
    headerNumbers: readonly string[]
    paymentStatus?: { label: string; tone: StatusTone }
    amount?: { label: string; value: string }
}

export function FulfillmentRiskStrip({
    task,
    operation,
    actionError,
    pendingAddresses,
    onLocate,
}: {
    task: FulfillmentTaskContext
    operation: FulfillmentOperation
    actionError?: string | null
    pendingAddresses: number
    onLocate: () => void
}) {
    const blockers = [
        ...new Set(
            [
                ...task.blockers,
                ...operation.actionBlockers
                    .filter(({ action }) => action === "POST")
                    .map(({ message }) => message),
                actionError,
            ].flatMap((message) => message?.trim() || []),
        ),
    ]
    const impact = [
        ...new Set(
            [
                task.impact,
                pendingAddresses > 0
                    ? `${pendingAddresses} 个收货地址待补充`
                    : undefined,
                operation.impact === "请读取关联资料后核对并处理"
                    ? undefined
                    : operation.impact,
                ...(operation.loadWarnings ?? []),
            ].flatMap((message) => message?.trim() || []),
        ),
    ].filter((message) => !blockers.includes(message))
    const paymentPending = operation.gate.state === "BLOCKED"
    const blocked = blockers.length > 0
    if (!blocked && !impact.length && !paymentPending) return null

    return (
        <section
            className={cn(
                "flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-1 rounded-r-md border-l-2 px-3 py-2 text-xs",
                blocked
                    ? "border-destructive bg-destructive-soft text-destructive-soft-foreground"
                    : "border-warning bg-warning-soft text-warning-soft-foreground",
            )}
            role="status"
            aria-label="履约核对事项"
        >
            <div className="flex min-w-0 flex-[1_1_16rem] gap-2">
                <CircleAlertIcon
                    aria-hidden="true"
                    className="mt-0.5 size-4 shrink-0"
                />
                <div className="min-w-0 space-y-0.5 wrap-anywhere">
                    {blockers.map((message) => (
                        <p key={message} className="font-medium">
                            {message}
                        </p>
                    ))}
                    {impact.map((message) => (
                        <p key={message} className="whitespace-pre-line">
                            {message}
                        </p>
                    ))}
                    {paymentPending ? (
                        <p>{paymentReminder(operation)}</p>
                    ) : null}
                </div>
            </div>
            <div className="flex flex-wrap items-center gap-1 empty:hidden">
                {impact.length > 0 && operation.lines.length > 0 ? (
                    <Button
                        id={`${task.id}-risk-locate`}
                        type="button"
                        size="xs"
                        variant="outline"
                        onClick={onLocate}
                    >
                        定位履约明细
                        <ArrowDownIcon
                            aria-hidden="true"
                            data-icon="inline-end"
                        />
                    </Button>
                ) : null}
                {paymentPending ? (
                    <FulfillmentPaymentLink
                        operation={operation}
                        id={`${task.id}-risk-payment`}
                    />
                ) : null}
            </div>
        </section>
    )
}

const GATE_STATUS = {
    BLOCKED: { label: "预付待完成", tone: "warning" },
    SATISFIED: { label: "预付已满足", tone: "success" },
    NOT_APPLICABLE: { label: "无预付要求", tone: "neutral" },
} as const

export function FulfillmentTaskKpis({
    task,
    operation,
    pending,
    progress,
}: {
    task: FulfillmentTaskContext
    operation: FulfillmentOperation
    pending: string
    progress: string
}) {
    const warehouseTask =
        operation.operationType === "RECEIPT" ||
        operation.operationType === "WAREHOUSE_SHIP"
    const party = warehouseTask
        ? displayText(
              operation.source.warehouseLabel,
              operation.source.warehouseId,
          )
        : displayText(operation.source.supplierLabel)
    const gate = GATE_STATUS[operation.gate.state]

    return (
        <dl
            aria-label="本次履约核心指标"
            className="grid min-w-0 grid-cols-1 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/fulfillment-task:grid-cols-2 @[840px]/fulfillment-task:grid-cols-4"
        >
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>本次待履约量</span>
                    <span className="text-tiny text-muted-foreground">
                        按单位汇总
                    </span>
                    <Tooltip>
                        <TooltipTrigger
                            id={`${task.id}-quantity-help`}
                            render={
                                <Button
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="待履约量统计口径"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent>
                            统计当前批次已有的待履约余量，按计量单位分别汇总。
                        </TooltipContent>
                    </Tooltip>
                </dt>
                <dd className="num wrap-anywhere text-value-section font-semibold tracking-tight">
                    {pending}
                </dd>
                {task.amount ? (
                    <dd className="wrap-anywhere text-xs text-muted-foreground">
                        {task.amount.label}{" "}
                        <span className="num select-all">
                            {task.amount.value}
                        </span>
                    </dd>
                ) : (
                    <dd className="num text-xs text-muted-foreground">
                        共 {operation.lines.length} 项明细
                    </dd>
                )}
            </div>
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>履约完成进度</span>
                    <span className="text-tiny text-muted-foreground">
                        各明细均摊
                    </span>
                    <Tooltip>
                        <TooltipTrigger
                            id={`${task.id}-progress-help`}
                            render={
                                <Button
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="履约完成进度计算口径"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent>
                            按各明细 (应履约量 − 剩余待履约量) ÷ 应履约量
                            分别计算后取均值，避免跨单位直接相加。
                        </TooltipContent>
                    </Tooltip>
                </dt>
                <dd className="num mt-0.5 text-value-compact font-semibold">
                    {progress === "—" ? progress : `${progress}%`}
                </dd>
                {progress !== "—" ? (
                    <dd
                        aria-hidden="true"
                        className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-border"
                    >
                        <div
                            className="h-full rounded-full bg-success"
                            style={{ width: `${progress}%` }}
                        />
                    </dd>
                ) : null}
            </div>
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>先款条件</span>
                    <span className="text-tiny text-muted-foreground">
                        不阻断履约
                    </span>
                    <Tooltip>
                        <TooltipTrigger
                            id={`${task.id}-gate-help`}
                            render={
                                <Button
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="先款条件说明"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent>
                            采购预付进度仅作核对提醒，不阻止按实际履约情况确认。
                        </TooltipContent>
                    </Tooltip>
                </dt>
                <dd className="mt-1 flex flex-wrap items-center gap-1.5">
                    <BusinessStatusBadge {...gate} />
                    {task.paymentStatus ? (
                        <BusinessStatusBadge
                            appearance="text"
                            {...task.paymentStatus}
                        />
                    ) : null}
                </dd>
                {operation.gate.requiredAmount != null &&
                operation.gate.effectivePaidAmount != null ? (
                    <dd className="mt-1 wrap-anywhere text-xs text-muted-foreground">
                        已付{" "}
                        <MoneyValue
                            value={operation.gate.effectivePaidAmount}
                        />{" "}
                        / 预付{" "}
                        <MoneyValue value={operation.gate.requiredAmount} />
                    </dd>
                ) : null}
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    {warehouseTask ? "履约仓库" : "履约供应商"}
                </dt>
                <dd className="mt-1 wrap-anywhere text-body-compact font-semibold text-foreground">
                    {party || "—"}
                </dd>
                {task.commitment ? (
                    <dd className="num mt-1 wrap-anywhere text-xs text-muted-foreground">
                        {task.commitment.label} {task.commitment.value}
                    </dd>
                ) : null}
            </div>
        </dl>
    )
}

export function FulfillmentBatchSummary({
    quantities,
    operation,
}: {
    quantities: ReturnType<typeof fulfillmentBatchQuantities>
    operation: FulfillmentOperation
}) {
    const remainingLabel =
        operation.operationType === "RECEIPT"
            ? "剩余待收"
            : operation.operationType === "WAREHOUSE_SHIP" ||
                operation.operationType === "SUPPLIER_DIRECT"
              ? "剩余待发"
              : "剩余待履约"
    return (
        <dl
            className="flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-1 text-xs"
            aria-live="polite"
            aria-label="本次登记汇总"
        >
            {[
                { label: "本次登记", value: `${quantities.lineCount} 行` },
                { label: "总数量", value: quantities.total },
                { label: remainingLabel, value: quantities.remaining },
            ].map(({ label, value }) => (
                <div key={label} className="flex min-w-0 items-baseline gap-1">
                    <dt className="shrink-0 text-muted-foreground">{label}</dt>
                    <dd className="num min-w-0 wrap-anywhere font-medium">
                        {value}
                    </dd>
                </div>
            ))}
        </dl>
    )
}

export function FulfillmentSourceContext({
    operation,
    currentUrl,
    customerInTitle = false,
    task,
}: {
    operation: FulfillmentOperation
    currentUrl: string
    customerInTitle?: boolean
    task?: FulfillmentTaskContext
}) {
    const profile = useAccountProfileQuery()
    const canViewPurchase = hasPermission(
        profile.data?.permissions,
        "purchase_order:detail",
    )
    const canViewSales = hasPermission(
        profile.data?.permissions,
        "sales_order:detail",
    )
    const fields = [
        ...sourceContextFields(
            operation,
            salesOrderHref(operation.source.salesOrderId, currentUrl),
        ),
    ]
        .filter((field) => !customerInTitle || field.label !== "客户")
        .filter(
            (field) =>
                !task ||
                (!task.headerNumbers.includes(field.value) &&
                    field.label !== "仓库" &&
                    (field.label !== "供应商" ||
                        operation.operationType === "RECEIPT" ||
                        operation.operationType === "WAREHOUSE_SHIP")),
        )
        .sort((left, right) => {
            const order = ["客户", "供应商", "仓库", "销售单", "采购单"]
            return order.indexOf(left.label) - order.indexOf(right.label)
        })
    if (task?.owner) fields.push({ label: "当前采购负责人", value: task.owner })
    if (fields.length === 0) return null
    return (
        <section aria-label="来源单据">
            <dl className="flex flex-wrap gap-x-5 gap-y-1.5 text-xs">
                {fields.map((field) => {
                    const purchaseLink =
                        field.label === "采购单" &&
                        operation.source.purchaseOrderId &&
                        canViewPurchase
                            ? `/procurement/orders/${encodeURIComponent(operation.source.purchaseOrderId)}`
                            : undefined
                    const href =
                        purchaseLink || (canViewSales ? field.href : undefined)
                    const isDocument =
                        field.label === "销售单" || field.label === "采购单"
                    return (
                        <div
                            key={field.label}
                            className="flex min-w-0 items-baseline gap-3"
                        >
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                {field.label}
                            </dt>
                            <dd
                                title={isDocument ? field.value : undefined}
                                className={cn(
                                    "min-w-0 wrap-anywhere font-medium",
                                    isDocument && "num select-all",
                                )}
                            >
                                {href ? (
                                    <Link
                                        id={`${task?.id ?? "fulfillment-operations-work-surface"}-source-${toAutomationIdSegment(field.label)}`}
                                        href={href}
                                        className="text-primary underline underline-offset-4"
                                    >
                                        {field.value}
                                    </Link>
                                ) : (
                                    field.value
                                )}
                            </dd>
                        </div>
                    )
                })}
            </dl>
        </section>
    )
}
