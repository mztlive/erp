"use client"

import { CircleCheckIcon, TriangleAlertIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { money } from "@/features/procurement-confirmation/lib/format"
import type { ProcurementRecommendation } from "@/features/procurement-confirmation/types"

type RecommendationStatusAlertProps = {
    isPending: boolean
    isError: boolean
    recommendation: ProcurementRecommendation | undefined
}

export function RecommendationStatusAlert({
    isPending,
    isError,
    recommendation,
}: RecommendationStatusAlertProps) {
    return (
        <Alert
            variant={
                isError
                    ? "destructive"
                    : recommendation?.ready
                      ? "success"
                      : "warning"
            }
        >
            {recommendation?.ready ? (
                <CircleCheckIcon aria-hidden="true" />
            ) : (
                <TriangleAlertIcon aria-hidden="true" />
            )}
            <AlertTitle>
                {isPending
                    ? "正在计算最低成本方案"
                    : isError
                      ? "最低成本方案计算失败"
                      : recommendation?.ready
                        ? `已组合 ${recommendation.purchaseOrders.length} 组采购创建建议`
                        : "当前无法形成完整采购方案"}
            </AlertTitle>
            <AlertDescription>
                {recommendation?.ready
                    ? `预计采购含税 ${money.format(Number(recommendation.estimatedPurchaseGross))}，预计毛利 ${money.format(Number(recommendation.estimatedGrossMargin))}。交期仍需采购核对。`
                    : recommendation?.blockingIssues
                          .map((issue) => issue.message)
                          .join("；") ||
                      "请等待系统完成计算，或刷新后重试。"}
            </AlertDescription>
        </Alert>
    )
}
