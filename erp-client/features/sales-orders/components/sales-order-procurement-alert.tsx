"use client"

import { ArrowRightIcon, TriangleAlertIcon } from "lucide-react"

import { QuantityValue } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { getProcurementAttention } from "@/features/sales-orders/lib/procurement-attention"
import type { SalesOrderProcurementProgress } from "@/features/sales-orders/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/** 页级提示只说明采购异常，逐项处理信息留在采购页签。 */
export function SalesOrderProcurementAlert({
    progress,
    onOpenPurchase,
}: {
    progress: SalesOrderProcurementProgress
    onOpenPurchase: () => void
}) {
    const { hasGap, hasExcess, needsAttention } =
        getProcurementAttention(progress)
    if (!needsAttention) return null

    const excessLines = (progress.lines ?? []).filter(
        (line) => compareDecimal(line.excessQuantity, "0", 6) > 0,
    )
    const singleExcess = excessLines.length === 1 ? excessLines[0] : undefined

    return (
        <Alert id="sales-order-procurement-alert" variant="warning">
            <TriangleAlertIcon aria-hidden="true" />
            <AlertDescription className="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2">
                <div className="min-w-0 flex-1 basis-64 break-words">
                    <span className="font-medium">采购异常：</span>
                    {hasExcess ? (
                        <span>
                            {singleExcess ? (
                                <>
                                    {singleExcess.name}超出{" "}
                                    <QuantityValue
                                        value={singleExcess.excessQuantity}
                                        unit={singleExcess.unit}
                                        tone="warning"
                                    />
                                </>
                            ) : excessLines.length > 0 ? (
                                `${excessLines.length} 项明细采购超出`
                            ) : (
                                "存在采购超出"
                            )}
                            ，待采购调减。
                        </span>
                    ) : null}
                    {hasGap ? <span>供给未完全覆盖，待分配供给。</span> : null}
                    {progress.sourceChanged ? (
                        <span>
                            {progress.affectedPurchaseCount}{" "}
                            张采购单依据待更新。
                        </span>
                    ) : null}
                    {progress.awaitingSubmissionCount > 0 ? (
                        <span>
                            {progress.awaitingSubmissionCount}{" "}
                            张采购单已修改待提交审批。
                        </span>
                    ) : null}
                </div>
                <Button
                    id="sales-order-procurement-alert-view-purchases"
                    variant="link"
                    size="sm"
                    className="h-auto p-0"
                    onClick={onOpenPurchase}
                >
                    查看采购
                    <ArrowRightIcon aria-hidden="true" data-icon="inline-end" />
                </Button>
            </AlertDescription>
        </Alert>
    )
}
