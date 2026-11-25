"use client"

import Link from "next/link"
import { ChevronDownIcon } from "lucide-react"
import {
    DecisionKpiBar,
    DecisionKpiCell,
    MoneyValue,
    QuantityValue,
    TaskRiskSummaryBar,
    WorkspaceFactGrid,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { SourceImpactReasonView } from "../lib/source-impact"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "../types"

type ChangeKind =
    | "数量归零"
    | "销售减量"
    | "履约期限提前"
    | "收件分配变更"
    | "销售要求变更"

function changeKind(reason: SourceImpactReasonView): ChangeKind {
    const { description } = reason
    if (description === "销售数量已归零") return "数量归零"
    if (description.startsWith("销售数量 ")) return "销售减量"
    if (description.startsWith("履约期限提前")) return "履约期限提前"
    if (description === "收件地址或数量已变化") return "收件分配变更"
    return "销售要求变更"
}

function mayChangeQuantity(reason: SourceImpactReasonView) {
    const kind = changeKind(reason)
    return kind !== "履约期限提前" && kind !== "收件分配变更"
}

export type PurchaseSourceLineImpact = Readonly<{
    reasons: readonly SourceImpactReasonView[]
    targetQuantity?: string
    quantityDelta?: string
    destructive: boolean
}>

const QUANTITY_SCALE = { maxScale: 6, outputScale: 6 }
const MONEY_SCALE = { maxScale: 2, outputScale: 2 }

export function buildPurchaseSourceImpact(order: PurchaseOrderCenterView) {
    const update = order.sourceUpdate
    const { lines, costMasked, totals } = order.currentContent
    const returnPlan = update?.returnHandling
    const sourceSales = order.sourceSalesOrder
    const reasons = update?.reasons ?? []
    const matchedReasons = new Set<SourceImpactReasonView>()
    const byLine = new Map<string, PurchaseSourceLineImpact>()
    const targetByLine = new Map(
        returnPlan?.lines.map((line) => [line.purchase_line_id, line]),
    )
    const reasonsBySalesId = new Map<string, SourceImpactReasonView[]>()
    const rowKey = (lineNo: number, itemName: string) =>
        `${lineNo}\u0000${itemName}`
    const sourceByRow = new Map<string, string | null>()
    for (const line of sourceSales?.lines ?? []) {
        const key = rowKey(line.lineNo, line.itemName)
        sourceByRow.set(
            key,
            sourceByRow.has(key) ? null : line.salesOrderLineId,
        )
    }

    // 影响摘要只提供销售行号与品名。仅在这两个字段能唯一对上关联版本时挂到采购行。
    for (const reason of reasons) {
        const salesLineId = sourceByRow.get(
            rowKey(reason.lineNo, reason.itemName),
        )
        if (!salesLineId) continue
        const list = reasonsBySalesId.get(salesLineId) ?? []
        list.push(reason)
        reasonsBySalesId.set(salesLineId, list)
    }

    const quantityGroups = new Map<
        string,
        { unit?: string; values: string[] }
    >()
    const removedGrossAmounts: string[] = []
    for (const line of lines) {
        const target = targetByLine.get(line.lineId)
        const lineReasons =
            reasonsBySalesId.get(
                line.salesOrderLineId ?? target?.sales_line_id ?? "",
            ) ?? []
        const zeroed = lineReasons.some(
            (reason) => changeKind(reason) === "数量归零",
        )
        if (!lineReasons.length && !target) continue

        const targetQuantity =
            (target ? compactFixed(target.target_quantity) : undefined) ??
            (zeroed && !update?.awaitingSubmission && line.quantity != null
                ? "0"
                : undefined)
        const quantityDelta =
            targetQuantity != null && line.quantity != null
                ? subtractFixed(
                      targetQuantity,
                      compactFixed(line.quantity),
                      QUANTITY_SCALE,
                  )
                : undefined
        const quantityChanged =
            quantityDelta != null && compareDecimal(quantityDelta, "0", 6) !== 0
        if (!lineReasons.length && !quantityChanged) continue
        lineReasons.forEach((reason) => matchedReasons.add(reason))
        byLine.set(line.lineId, {
            reasons: lineReasons,
            targetQuantity,
            quantityDelta,
            destructive:
                zeroed ||
                (quantityDelta != null &&
                    compareDecimal(quantityDelta, "0", 6) < 0),
        })

        if (quantityChanged) {
            const unitKey = line.unit ?? line.unitName ?? ""
            const group = quantityGroups.get(unitKey) ?? {
                unit: line.unitName ?? line.unit,
                values: [],
            }
            group.values.push(quantityDelta)
            quantityGroups.set(unitKey, group)
        }
        if (zeroed && targetQuantity === "0" && !costMasked)
            removedGrossAmounts.push(compactFixed(line.grossAmount))
    }

    const unmatched = reasons.filter((reason) => !matchedReasons.has(reason))
    const unknownQuantity =
        Boolean(update?.awaitingSubmission) ||
        Boolean(update?.affected && !reasons.length && !returnPlan) ||
        unmatched.some(mayChangeQuantity) ||
        [...byLine.values()].some(
            (impact) =>
                impact.quantityDelta == null &&
                impact.reasons.some(mayChangeQuantity),
        )

    return {
        byLine,
        unmatched,
        changeKinds: [...new Set(reasons.map(changeKind))],
        quantityDeltas: [...quantityGroups].map(([key, group]) => ({
            key,
            unit: group.unit,
            value: sumFixed(group.values, {
                ...QUANTITY_SCALE,
                allowNegative: true,
            }),
        })),
        unknownQuantity,
        amountDelta: costMasked
            ? undefined
            : returnPlan
              ? subtractFixed(
                    compactFixed(returnPlan.target_gross),
                    compactFixed(totals.gross),
                    MONEY_SCALE,
                )
              : !unknownQuantity
                ? subtractFixed(
                      "0",
                      sumFixed(removedGrossAmounts, MONEY_SCALE),
                      MONEY_SCALE,
                  )
                : undefined,
    }
}

export function PurchaseSourceDelta({
    value,
    unit,
    money = false,
}: {
    value: string
    unit?: string
    money?: boolean
}) {
    const comparison = compareDecimal(value, "0", money ? 2 : 6)
    const magnitude = comparison < 0 ? value.slice(1) : value
    return (
        <span
            className={cn(
                "inline-flex flex-wrap items-baseline gap-x-1",
                comparison < 0 && "text-destructive",
                comparison > 0 && "text-success-soft-foreground",
            )}
        >
            {comparison < 0 ? "减" : comparison > 0 ? "增" : null}
            {money ? (
                <MoneyValue
                    value={magnitude}
                    display="exact"
                    inheritColor
                    className="wrap-anywhere"
                />
            ) : (
                <QuantityValue
                    value={magnitude}
                    unit={unit}
                    className="min-w-0 flex-wrap wrap-anywhere [&_.num]:text-inherit"
                />
            )}
        </span>
    )
}

/** 影响汇总、决策指标和可按需展开的采购条款。 */
export function PurchaseSourceUpdateContext({
    order,
    idPrefix = `purchase-source-update-${toAutomationIdSegment(order.identity.purchaseOrderId)}`,
    impact = buildPurchaseSourceImpact(order),
    resolution = order.sourceUpdate?.lastResolutionLabel ??
        order.identity.statusLabel,
    returnTo = "",
    onShowImpacted,
}: {
    order: PurchaseOrderCenterView
    idPrefix?: string
    impact?: ReturnType<typeof buildPurchaseSourceImpact>
    resolution?: string
    returnTo?: string
    onShowImpacted?: () => void
}) {
    const { header, sourceUpdate: update, sourceSalesOrder } = order
    const { lines, totals, costMasked } = order.currentContent
    const impacted = impact.byLine.size
    const unaffected = lines.length - impacted
    const hasRisk = Boolean(
        update?.affected || update?.awaitingSubmission || update?.blocksPayment,
    )
    const severe = Boolean(
        update?.blocksPayment || impact.changeKinds.includes("数量归零"),
    )
    const purchaseRevision =
        update?.purchaseSalesRevisionNo ?? sourceSalesOrder?.revisionNo
    const currentRevision = update?.currentSalesRevisionNo
    const salesHref = `/sales/orders/${encodeURIComponent(header.salesOrderId)}?${new URLSearchParams({ from: "workspace", returnTo })}`

    return (
        <div className="min-w-0 space-y-3">
            {hasRisk ? (
                <TaskRiskSummaryBar
                    aria-label="采购变更影响"
                    severity={severe ? "destructive" : "warning"}
                    leadLabel={null}
                    summary={
                        <span className="inline-flex flex-col gap-0.5">
                            <span className="font-medium">
                                {impact.changeKinds.join(" · ") ||
                                    (update?.awaitingSubmission
                                        ? "采购修改待提交"
                                        : "销售要求已变更")}
                                <span className="num">
                                    {" "}
                                    · {impacted} 行受影响
                                </span>
                            </span>
                            {update?.blocksPayment ||
                            impact.unmatched.length ? (
                                <span className="text-xs">
                                    {update?.blocksPayment
                                        ? "暂停付款申请和付款，处理完成后解除。"
                                        : null}
                                    {impact.unmatched.length
                                        ? "部分销售变更未能关联到当前采购行。"
                                        : null}
                                </span>
                            ) : null}
                        </span>
                    }
                    actions={
                        onShowImpacted &&
                        (impacted > 0 || impact.unmatched.length > 0)
                            ? [
                                  {
                                      id: "jump-impact",
                                      buttonId: `${idPrefix}-jump-impact`,
                                      label:
                                          impacted > 0
                                              ? "仅看受影响行"
                                              : "查看销售变更原因",
                                      count:
                                          impacted > 0 ? impacted : undefined,
                                      onClick: onShowImpacted,
                                  },
                              ]
                            : undefined
                    }
                />
            ) : null}

            <DecisionKpiBar columns={4} ariaLabel="采购变更关键指标">
                <DecisionKpiCell
                    label={costMasked ? "受影响采购行" : "采购单含税金额"}
                    emphasis="primary"
                    value={
                        costMasked ? (
                            <QuantityValue value={String(impacted)} unit="行" />
                        ) : (
                            <MoneyValue value={totals.gross} display="exact" />
                        )
                    }
                    formulaTooltip="金额以当前采购整单为基准。目标差额按当前采购价计算，待确认差额以修改或改单结果为准。"
                    formulaTooltipId={`${idPrefix}-kpi-help`}
                    formulaTooltipLabel="查看指标计算口径"
                />
                <DecisionKpiCell
                    label="采购目标差额"
                    emphasis="secondary"
                    value={
                        costMasked ? (
                            <span className="text-xs font-normal text-muted-foreground">
                                金额已隐藏
                            </span>
                        ) : impact.amountDelta != null ? (
                            <PurchaseSourceDelta
                                value={impact.amountDelta}
                                money
                            />
                        ) : (
                            <span className="text-xs font-normal text-muted-foreground">
                                金额待核对
                            </span>
                        )
                    }
                    footnote={
                        <span className="flex flex-wrap gap-x-3 gap-y-0.5 text-xs">
                            {impact.quantityDeltas.map((delta) => (
                                <PurchaseSourceDelta
                                    key={delta.key}
                                    value={delta.value}
                                    unit={delta.unit}
                                />
                            ))}
                            {impact.unknownQuantity ? (
                                <span className="text-muted-foreground">
                                    {update?.awaitingSubmission
                                        ? "数量修改已保存"
                                        : "数量待核对"}
                                </span>
                            ) : impact.quantityDeltas.length === 0 ? (
                                <span className="text-muted-foreground">
                                    数量无差额
                                </span>
                            ) : null}
                        </span>
                    }
                />
                <DecisionKpiCell
                    label="明细影响"
                    emphasis="secondary"
                    value={impacted}
                    valueMeta={
                        <span className="text-xs font-normal text-muted-foreground">
                            / <span className="num">{lines.length}</span> 行
                            {impacted > 0
                                ? "受影响"
                                : impact.unmatched.length > 0
                                  ? "未关联影响"
                                  : "未受影响"}
                        </span>
                    }
                    extra={
                        <div className="space-y-1">
                            <div
                                aria-hidden="true"
                                className="flex h-1.5 overflow-hidden rounded-full bg-border"
                            >
                                {impacted > 0 ? (
                                    <span
                                        className={
                                            severe
                                                ? "bg-destructive"
                                                : "bg-warning"
                                        }
                                        style={{ flexGrow: impacted }}
                                    />
                                ) : null}
                                {unaffected > 0 ? (
                                    <span
                                        className={
                                            impacted === 0
                                                ? "bg-success"
                                                : "bg-border"
                                        }
                                        style={{ flexGrow: unaffected }}
                                    />
                                ) : null}
                            </div>
                            {unaffected > 0 && impacted > 0 ? (
                                <p className="num text-tiny text-muted-foreground">
                                    其余 {unaffected} 行
                                    {impact.unmatched.length
                                        ? "未关联影响"
                                        : "未受影响"}
                                </p>
                            ) : null}
                        </div>
                    }
                />
                <DecisionKpiCell
                    label="来源销售版本"
                    emphasis="terms"
                    value={
                        purchaseRevision != null &&
                        currentRevision != null &&
                        purchaseRevision !== currentRevision
                            ? `第 ${purchaseRevision} 版 → 第 ${currentRevision} 版`
                            : currentRevision != null ||
                                purchaseRevision != null
                              ? `第 ${currentRevision ?? purchaseRevision} 版`
                              : "—"
                    }
                    extra={
                        sourceSalesOrder ? (
                            <StatusBadge
                                label={sourceSalesOrder.statusLabel}
                                tone={sourceSalesOrder.statusTone}
                            />
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                {resolution}
                            </span>
                        )
                    }
                />
            </DecisionKpiBar>

            <div
                aria-label="来源销售与采购状态"
                className="space-y-2 border-y border-border/60 py-3"
            >
                <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                    <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
                        <span className="text-xs font-medium text-muted-foreground">
                            来源销售单
                        </span>
                        <span className="num select-all text-body-compact font-semibold text-foreground">
                            {header.salesOrderNo || "—"}
                        </span>
                        <span
                            aria-hidden="true"
                            className="h-3 w-px bg-border/80"
                        />
                        <span className="text-xs font-medium text-muted-foreground">
                            采购状态
                        </span>
                        <StatusBadge
                            label={order.identity.statusLabel}
                            tone={order.identity.statusTone}
                        />
                    </div>
                    {header.salesOrderId && header.salesOrderNo ? (
                        <div className="shrink-0">
                            <Link
                                id={`${idPrefix}-sales`}
                                href={salesHref}
                                className={cn(
                                    "inline-flex items-center rounded-md px-2 py-1 text-xs font-medium",
                                    detailWorkspaceStyles.inlineLinkButton,
                                )}
                            >
                                查看来源销售单
                            </Link>
                        </div>
                    ) : null}
                </div>
                {update?.lastMaintainNote && !hasRisk ? (
                    <div className="flex items-start gap-2 rounded-r-md border-l-2 border-info bg-info-soft/30 px-2.5 py-1.5 text-xs">
                        <p className="min-w-0 whitespace-pre-wrap wrap-anywhere text-foreground/90">
                            <span className="font-medium text-info-soft-foreground">
                                维持原因：
                            </span>
                            {update.lastMaintainNote}
                        </p>
                    </div>
                ) : null}
            </div>
            <Collapsible>
                <CollapsibleTrigger
                    id={`${idPrefix}-commercial-context`}
                    className="group flex max-w-full cursor-pointer items-center gap-1.5 py-0.5 text-left text-xs text-muted-foreground hover:text-foreground"
                >
                    <ChevronDownIcon
                        aria-hidden="true"
                        className="size-3 shrink-0 transition-transform group-data-panel-open:rotate-180"
                    />
                    采购条款 · 付款条件、采购类型、履约责任
                    {header.targetWarehouseName ? "、目标仓" : ""}
                </CollapsibleTrigger>
                <CollapsibleContent className="pt-2.5">
                    {(() => {
                        const isLongPayment = Boolean(
                            header.paymentTermLabel &&
                            header.paymentTermLabel.length > 30,
                        )
                        const isLongWarehouse = Boolean(
                            header.targetWarehouseName &&
                            header.targetWarehouseName.length > 30,
                        )
                        const shortClauseCount =
                            (isLongPayment ? 0 : 1) +
                            2 +
                            (header.targetWarehouseName && !isLongWarehouse
                                ? 1
                                : 0)
                        const clauseColumns: 2 | 3 | 4 =
                            shortClauseCount <= 2
                                ? 2
                                : shortClauseCount === 4
                                  ? 4
                                  : 3
                        return (
                            <WorkspaceFactGrid columns={clauseColumns}>
                                <WorkspaceFactItem
                                    label="付款条件"
                                    value={header.paymentTermLabel}
                                    span={isLongPayment ? "full" : 1}
                                />
                                <WorkspaceFactItem
                                    label="采购类型"
                                    value={
                                        PURCHASE_TYPE_LABEL[header.purchaseType]
                                    }
                                />
                                <WorkspaceFactItem
                                    label="履约责任"
                                    value={
                                        FULFILLMENT_RESPONSIBILITY_LABEL[
                                            header.fulfillmentResponsibility
                                        ]
                                    }
                                />
                                {header.targetWarehouseName ? (
                                    <WorkspaceFactItem
                                        label="目标仓"
                                        value={header.targetWarehouseName}
                                        span={isLongWarehouse ? "full" : 1}
                                    />
                                ) : null}
                            </WorkspaceFactGrid>
                        )
                    })()}
                </CollapsibleContent>
            </Collapsible>
        </div>
    )
}
