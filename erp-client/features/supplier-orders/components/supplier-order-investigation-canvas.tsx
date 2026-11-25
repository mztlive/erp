"use client"

import { useState, type ReactNode } from "react"

import {
    DecisionKpiBar,
    DecisionKpiCell,
    MoneyValue,
    QuantityValue,
    TaskRiskSummaryBar,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import { compareDecimal, divideFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import type { SupplierExecutionOrderTree } from "../api/execution-setup"
import type { SupplierOrderDetailView } from "../types"
import { InvestigationTime } from "./supplier-order-investigation-diagnostics"
import {
    SupplierInvestigationLines,
    confirmedQuantity,
    getInvestigationLines,
} from "./supplier-order-investigation-lines"

export function SupplierInvestigationCanvas({
    idPrefix,
    detail,
    tree,
    investigation,
    lastSync,
    remoteStatus,
    onOpenDiagnostics,
    totalQuantity,
    totalCostGross,
    isResultUnknown,
    queryBlocker,
    notice,
    children,
}: {
    idPrefix: string
    detail: SupplierOrderDetailView
    tree?: SupplierExecutionOrderTree | null
    investigation: SupplierOrderDetailView["lastInvestigation"]
    lastSync?: string
    remoteStatus?: string
    onOpenDiagnostics: () => void
    totalQuantity: string
    totalCostGross: string | null
    isResultUnknown: boolean
    queryBlocker?: string
    notice?: ReactNode
    children: ReactNode
}) {
    const { order } = detail
    const [locateRequest, setLocateRequest] = useState<{
        itemId: string
    } | null>(null)
    const lines = getInvestigationLines(detail.items, tree)
    const risks = lines.filter((line) => line.mismatch)
    const verifiedLines = lines.filter(
        (line) => line.remoteQuantity !== undefined,
    )
    const matchedLines = verifiedLines.length - risks.length
    const units = new Set(lines.map((line) => line.unit))
    const singleUnit = units.size === 1 ? lines[0].unit || undefined : undefined
    const hasQuantityProgress =
        singleUnit && compareDecimal(totalQuantity, "0", 6) > 0
    const verifiedQuantity = hasQuantityProgress
        ? confirmedQuantity(lines)
        : undefined
    const hasProgress = verifiedLines.length > 0
    const progress = hasProgress
        ? multiplyFixed(
              divideFixed(
                  verifiedQuantity ?? String(matchedLines),
                  verifiedQuantity !== undefined
                      ? totalQuantity
                      : String(lines.length),
                  {
                      numeratorMaxScale: 6,
                      denominatorMaxScale: 6,
                      outputScale: 4,
                  },
              ),
              "100",
              { leftMaxScale: 4, rightMaxScale: 0, outputScale: 2 },
          )
        : "0"

    const costVariance = detail.costs.costVariance
    const varianceSign =
        costVariance != null ? compareDecimal(costVariance, "0", 4) : 0
    const hasVariance = varianceSign !== 0
    const absCostVariance = costVariance ? costVariance.replace(/^-/, "") : null
    const totalCost = lines.every((line) => line.unitCostGross != null)
        ? totalCostGross
        : null
    const primaryAmount = hasVariance ? costVariance : totalCost
    const allMatched =
        hasProgress &&
        risks.length === 0 &&
        verifiedLines.length === lines.length &&
        compareDecimal(progress, "100", 2) >= 0
    const exception =
        isResultUnknown ||
        investigation?.outcome === "RESULT_UNKNOWN" ||
        detail.trackingError ||
        order.errorSummary ||
        order.fulfillmentStatus === "EXCEPTION" ||
        order.fulfillmentStatus === "REJECTED"
    const exceptionReason =
        detail.trackingError ||
        order.errorSummary ||
        investigation?.summary ||
        queryBlocker
    const lineMismatch = risks[0]
    const partialRefund =
        order.fulfillmentStatus === "COMPLETED" &&
        order.refundStatus === "PARTIAL"

    return (
        <div className="@container/supplier-investigation min-w-0 space-y-4 bg-card py-3.5 wrap-anywhere">
            {notice}
            {exception || lineMismatch || hasVariance || partialRefund ? (
                <TaskRiskSummaryBar
                    severity={exception ? "destructive" : "warning"}
                    leadLabel={null}
                    summary={
                        <span className="inline-flex flex-col gap-0.5">
                            {exception ? (
                                <span>
                                    <strong className="font-semibold">
                                        {isResultUnknown
                                            ? "供应商同步结果未知"
                                            : "供应商同步异常"}
                                    </strong>
                                    {exceptionReason
                                        ? ` · ${exceptionReason}`
                                        : ""}
                                </span>
                            ) : null}
                            {lineMismatch ? (
                                <span>
                                    <strong className="num font-semibold">
                                        {risks.length}
                                    </strong>{" "}
                                    行确认量与 ERP 下单量不一致
                                </span>
                            ) : null}
                            {hasVariance ? (
                                <span>供应商订单存在成本差额</span>
                            ) : null}
                            {partialRefund ? (
                                <span>订单已履约，仅部分退款已入账。</span>
                            ) : null}
                        </span>
                    }
                    actions={[
                        exception || hasVariance || partialRefund
                            ? {
                                  id: "open-diagnostics",
                                  buttonId: `${idPrefix}-open-diagnostics`,
                                  label: "查看同步诊断",
                                  onClick: onOpenDiagnostics,
                              }
                            : null,
                        lineMismatch
                            ? {
                                  id: "locate-risk",
                                  buttonId: `${idPrefix}-locate-risk`,
                                  label: "定位异常行",
                                  onClick: () =>
                                      setLocateRequest({
                                          itemId: lineMismatch.itemId,
                                      }),
                              }
                            : null,
                    ]}
                />
            ) : null}

            <DecisionKpiBar
                columns={4}
                ariaLabel="供应商订单排查关键指标"
                gridClassName="gap-3"
            >
                <DecisionKpiCell
                    label={hasVariance ? "成本差额" : "下单成本（含税）"}
                    emphasis={primaryAmount == null ? "terms" : "primary"}
                    formulaTooltipId={`${idPrefix}-cost-basis`}
                    formulaTooltipLabel="查看金额计算口径"
                    formulaTooltip={
                        hasVariance
                            ? "显示订单记录中的整单成本差额（正差表示高于原预算，负差表示低于原预算）。"
                            : "将各行下单数量乘以含税快照单价后逐行保留两位，再汇总整单。"
                    }
                    value={
                        primaryAmount == null ? (
                            <span className="text-xs font-normal text-muted-foreground">
                                暂无可核对金额
                            </span>
                        ) : hasVariance && absCostVariance ? (
                            <span
                                className={cn(
                                    "num inline-flex flex-wrap items-baseline gap-x-1.5",
                                    varianceSign > 0
                                        ? "text-destructive"
                                        : "text-success-soft-foreground",
                                )}
                            >
                                <span className="text-body-compact font-medium">
                                    {varianceSign > 0 ? "超支" : "节余"}
                                </span>
                                <MoneyValue
                                    value={absCostVariance}
                                    size="section"
                                    display="exact"
                                    inheritColor
                                />
                            </span>
                        ) : (
                            <MoneyValue
                                value={primaryAmount}
                                size="section"
                                display="exact"
                            />
                        )
                    }
                    footnote={
                        hasVariance && totalCost != null ? (
                            <>
                                下单成本{" "}
                                <MoneyValue value={totalCost} display="exact" />
                            </>
                        ) : undefined
                    }
                />

                <DecisionKpiCell
                    label="同步状态"
                    emphasis="terms"
                    value={
                        <span className="flex flex-wrap items-center gap-1.5 text-xs font-normal">
                            <span className="text-muted-foreground">
                                供应商
                            </span>{" "}
                            <span>{remoteStatus ?? "尚未核实"}</span>
                        </span>
                    }
                    extra={
                        <div className="flex flex-wrap items-center gap-1.5 text-xs">
                            <span className="text-muted-foreground">ERP</span>
                            <StatusBadge
                                label={order.fulfillmentLabel}
                                tone={order.fulfillmentTone}
                            />
                        </div>
                    }
                />

                <DecisionKpiCell
                    label="履约进度 · 子单确认"
                    emphasis="terms"
                    formulaTooltipId={`${idPrefix}-progress-basis`}
                    formulaTooltipLabel="查看履约进度计算口径"
                    formulaTooltip="按已确认子订单明细与 ERP 下单量逐行比对；单一单位按确认数量占比核算，多单位按一致行数占比核算。"
                    value={
                        <span className="num text-xs font-medium">
                            {hasProgress ? (
                                <>
                                    已核对 {verifiedLines.length} /{" "}
                                    {lines.length} 行
                                    {verifiedQuantity !== undefined &&
                                    singleUnit ? (
                                        <span className="block">
                                            <QuantityValue
                                                value={verifiedQuantity}
                                                unit=""
                                            />{" "}
                                            /{" "}
                                            <QuantityValue
                                                value={totalQuantity}
                                                unit={singleUnit}
                                            />
                                        </span>
                                    ) : (
                                        <span className="block">
                                            数量一致 {matchedLines} 行
                                        </span>
                                    )}
                                </>
                            ) : (
                                "等待供应商确认明细"
                            )}
                        </span>
                    }
                    extra={
                        <div
                            aria-hidden="true"
                            className="h-1.5 overflow-hidden rounded-full bg-border/60"
                        >
                            <div
                                className={cn(
                                    "h-full rounded-full",
                                    risks.length > 0
                                        ? "bg-warning"
                                        : allMatched
                                          ? "bg-success"
                                          : "bg-primary",
                                )}
                                style={{ width: `${progress}%` }}
                            />
                        </div>
                    }
                />

                <DecisionKpiCell
                    label="供应商"
                    emphasis="terms"
                    value={order.supplierName}
                    footnote={
                        <>
                            最近同步 <InvestigationTime value={lastSync} />
                        </>
                    }
                />
            </DecisionKpiBar>

            <WorkspaceFactGrid columns={3}>
                <WorkspaceFactItem
                    label="外部单号"
                    value={
                        order.externalOrderNo ?? investigation?.externalOrderNo
                    }
                    emptyText="尚未返回"
                    numeric
                    selectable
                />
                {detail.purchaseOrderNo ? (
                    <WorkspaceFactItem
                        label="采购单"
                        value={detail.purchaseOrderNo}
                        numeric
                        selectable
                    />
                ) : null}
                <WorkspaceFactItem label="创建时间" numeric selectable>
                    <InvestigationTime value={order.createdAt} />
                </WorkspaceFactItem>
                <WorkspaceFactItem label="取消" value={order.cancelLabel} />
                <WorkspaceFactItem label="退款" value={order.refundLabel} />
                {detail.workItem?.ownerUser ? (
                    <WorkspaceFactItem
                        label="处理人"
                        value={detail.workItem.ownerUser.displayName}
                    />
                ) : null}
            </WorkspaceFactGrid>

            {detail.workItemBlocker ? (
                <p
                    role="status"
                    className="text-xs text-warning-soft-foreground"
                >
                    {detail.workItemBlocker.message}
                </p>
            ) : null}

            {children}
            <SupplierInvestigationLines
                idPrefix={idPrefix}
                lines={lines}
                totalQuantity={totalQuantity}
                totalCost={totalCost}
                singleUnit={singleUnit}
                onOpenDiagnostics={onOpenDiagnostics}
                locateRequest={locateRequest}
            />
        </div>
    )
}
