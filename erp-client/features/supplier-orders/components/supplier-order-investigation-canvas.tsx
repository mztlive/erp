"use client"

import { useState, type ReactNode } from "react"
import {
    ChevronRightIcon,
    CircleHelpIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    MoneyValue,
    QuantityValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
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
                <div
                    role="alert"
                    className={cn(
                        "flex flex-wrap items-center justify-between gap-2 rounded-r-md border-l-2 px-3 py-2 text-xs",
                        exception
                            ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                    )}
                >
                    <div className="flex min-w-0 flex-[1_1_12rem] items-start gap-2">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <div className="min-w-0 flex-1 space-y-0.5">
                            {exception ? (
                                <p>
                                    <strong>
                                        {isResultUnknown
                                            ? "供应商同步结果未知"
                                            : "供应商同步异常"}
                                    </strong>
                                    {exceptionReason
                                        ? ` · ${exceptionReason}`
                                        : ""}
                                </p>
                            ) : null}
                            {lineMismatch ? (
                                <p>
                                    <strong className="num">
                                        {risks.length}
                                    </strong>{" "}
                                    行确认量与 ERP 下单量不一致
                                </p>
                            ) : null}
                            {hasVariance ? <p>供应商订单存在成本差额</p> : null}
                            {partialRefund ? (
                                <p>订单已履约，仅部分退款已入账。</p>
                            ) : null}
                        </div>
                    </div>
                    <div className="flex flex-wrap gap-1">
                        {exception || hasVariance || partialRefund ? (
                            <Button
                                id={`${idPrefix}-open-diagnostics`}
                                size="xs"
                                variant="ghost"
                                className={cn(
                                    "font-medium shadow-none",
                                    exception
                                        ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                        : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                )}
                                onClick={onOpenDiagnostics}
                            >
                                查看同步诊断{" "}
                                <ChevronRightIcon aria-hidden="true" />
                            </Button>
                        ) : null}
                        {lineMismatch ? (
                            <Button
                                id={`${idPrefix}-locate-risk`}
                                size="xs"
                                variant="ghost"
                                className={cn(
                                    "font-medium shadow-none",
                                    exception
                                        ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                        : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                )}
                                onClick={() =>
                                    setLocateRequest({
                                        itemId: lineMismatch.itemId,
                                    })
                                }
                            >
                                定位异常行{" "}
                                <ChevronRightIcon aria-hidden="true" />
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : null}

            <TooltipProvider>
                <dl className="grid grid-cols-1 gap-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/supplier-investigation:grid-cols-2 @[840px]/supplier-investigation:grid-cols-4">
                    <div className="min-w-0 space-y-1">
                        <dt className="flex items-center gap-1 text-xs text-muted-foreground">
                            {hasVariance ? "成本差额" : "下单成本（含税）"}
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${idPrefix}-cost-basis`}
                                    render={
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label="查看金额计算口径"
                                            className="rounded-full text-muted-foreground"
                                        />
                                    }
                                >
                                    <CircleHelpIcon aria-hidden="true" />
                                </TooltipTrigger>
                                <TooltipContent>
                                    {hasVariance
                                        ? "显示订单记录中的整单成本差额（正差表示高于原预算，负差表示低于原预算）。"
                                        : "将各行下单数量乘以含税快照单价后逐行保留两位，再汇总整单。"}
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd>
                            {primaryAmount == null ? (
                                <span className="text-xs text-muted-foreground">
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
                                        className="[&_[data-slot=money-value-number]]:whitespace-normal"
                                    />
                                </span>
                            ) : (
                                <MoneyValue
                                    value={primaryAmount}
                                    size="section"
                                    display="exact"
                                    className="[&_[data-slot=money-value-number]]:whitespace-normal"
                                />
                            )}
                        </dd>
                        {hasVariance && totalCost != null ? (
                            <dd className="text-tiny text-muted-foreground">
                                下单成本{" "}
                                <MoneyValue value={totalCost} display="exact" />
                            </dd>
                        ) : null}
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            同步状态
                        </dt>
                        <dd className="flex flex-wrap items-center gap-1.5 text-xs">
                            <span className="text-muted-foreground">
                                供应商
                            </span>{" "}
                            {remoteStatus ?? "尚未核实"}
                        </dd>
                        <dd className="flex flex-wrap items-center gap-1.5 text-xs">
                            <span className="text-muted-foreground">ERP</span>
                            <StatusBadge
                                label={order.fulfillmentLabel}
                                tone={order.fulfillmentTone}
                            />
                        </dd>
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="flex items-center gap-1 text-xs text-muted-foreground">
                            履约进度 · 子单确认
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${idPrefix}-progress-basis`}
                                    render={
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label="查看履约进度计算口径"
                                            className="rounded-full text-muted-foreground"
                                        />
                                    }
                                >
                                    <CircleHelpIcon aria-hidden="true" />
                                </TooltipTrigger>
                                <TooltipContent>
                                    按已确认子订单明细与 ERP
                                    下单量逐行比对；单一单位按确认数量占比核算，多单位按一致行数占比核算。
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="num text-xs font-medium">
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
                        </dd>
                        <dd
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
                        </dd>
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            供应商
                        </dt>
                        <dd className="text-body-compact font-medium">
                            {order.supplierName}
                        </dd>
                        <dd className="text-xs text-muted-foreground">
                            最近同步 <InvestigationTime value={lastSync} />
                        </dd>
                    </div>
                </dl>
            </TooltipProvider>

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
