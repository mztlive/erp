"use client"

import Link from "next/link"
import {
    BusinessStatusBadge,
    DecisionKpiBar,
    DecisionKpiCell,
    MoneyValue,
    TaskRiskSummaryBar,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
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
        <TaskRiskSummaryBar
            role="status"
            aria-label="履约核对事项"
            severity={blocked ? "destructive" : "warning"}
            leadLabel={null}
            summary={
                <span className="inline-flex flex-col gap-0.5">
                    {blockers.map((message) => (
                        <span key={message} className="font-medium">
                            {message}
                        </span>
                    ))}
                    {impact.map((message) => (
                        <span key={message} className="whitespace-pre-line">
                            {message}
                        </span>
                    ))}
                    {paymentPending ? (
                        <span className="inline-flex flex-wrap items-center gap-2">
                            <span>{paymentReminder(operation)}</span>
                            <FulfillmentPaymentLink
                                operation={operation}
                                id={`${task.id}-risk-payment`}
                            />
                        </span>
                    ) : null}
                </span>
            }
            actions={
                impact.length > 0 && operation.lines.length > 0
                    ? [
                          {
                              id: "locate",
                              buttonId: `${task.id}-risk-locate`,
                              label: "定位履约明细",
                              onClick: onLocate,
                          },
                      ]
                    : undefined
            }
        />
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
        <DecisionKpiBar columns={4} ariaLabel="本次履约核心指标">
            <DecisionKpiCell
                label="本次待履约量"
                basis="按单位汇总"
                emphasis="primary"
                value={pending}
                formulaTooltip="统计当前批次已有的待履约余量，按计量单位分别汇总。"
                formulaTooltipId={`${task.id}-quantity-help`}
                formulaTooltipLabel="待履约量统计口径"
                footnote={
                    task.amount ? (
                        <>
                            {task.amount.label}{" "}
                            <span className="num select-all">
                                {task.amount.value}
                            </span>
                        </>
                    ) : (
                        <>共 {operation.lines.length} 项明细</>
                    )
                }
            />
            <DecisionKpiCell
                label="履约完成进度"
                basis="各明细均摊"
                emphasis="secondary"
                value={progress === "—" ? progress : `${progress}%`}
                formulaTooltip="按各明细 (应履约量 − 剩余待履约量) ÷ 应履约量 分别计算后取均值，避免跨单位直接相加。"
                formulaTooltipId={`${task.id}-progress-help`}
                formulaTooltipLabel="履约完成进度计算口径"
                extra={
                    progress !== "—" ? (
                        <div
                            aria-hidden="true"
                            className="h-1.5 overflow-hidden rounded-full bg-border"
                        >
                            <div
                                className="h-full rounded-full bg-success"
                                style={{ width: `${progress}%` }}
                            />
                        </div>
                    ) : null
                }
            />
            <DecisionKpiCell
                label="先款条件"
                basis="不阻断履约"
                emphasis="terms"
                value={
                    <span className="flex flex-wrap items-center gap-1.5">
                        <BusinessStatusBadge {...gate} />
                        {task.paymentStatus ? (
                            <BusinessStatusBadge
                                appearance="text"
                                {...task.paymentStatus}
                            />
                        ) : null}
                    </span>
                }
                formulaTooltip="采购预付进度仅作核对提醒，不阻止按实际履约情况确认。"
                formulaTooltipId={`${task.id}-gate-help`}
                formulaTooltipLabel="先款条件说明"
                footnote={
                    operation.gate.requiredAmount != null &&
                    operation.gate.effectivePaidAmount != null ? (
                        <>
                            已付{" "}
                            <MoneyValue
                                value={operation.gate.effectivePaidAmount}
                            />{" "}
                            / 预付{" "}
                            <MoneyValue value={operation.gate.requiredAmount} />
                        </>
                    ) : undefined
                }
            />
            <DecisionKpiCell
                label={warehouseTask ? "履约仓库" : "履约供应商"}
                emphasis="terms"
                value={party || "—"}
                footnote={
                    task.commitment
                        ? `${task.commitment.label} ${task.commitment.value}`
                        : undefined
                }
            />
        </DecisionKpiBar>
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
            <WorkspaceFactGrid columns={3}>
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
                    const isLong = /地址|备注|说明/.test(field.label)
                    return (
                        <WorkspaceFactItem
                            key={field.label}
                            label={field.label}
                            numeric={isDocument}
                            selectable={isDocument}
                            span={isLong ? "full" : 1}
                        >
                            {href ? (
                                <Link
                                    id={`${task?.id ?? "fulfillment-operations-work-surface"}-source-${toAutomationIdSegment(field.label)}`}
                                    href={href}
                                    title={isDocument ? field.value : undefined}
                                    className="text-primary underline underline-offset-4"
                                >
                                    {field.value}
                                </Link>
                            ) : isDocument ? (
                                <span title={field.value}>{field.value}</span>
                            ) : (
                                field.value
                            )}
                        </WorkspaceFactItem>
                    )
                })}
            </WorkspaceFactGrid>
        </section>
    )
}
