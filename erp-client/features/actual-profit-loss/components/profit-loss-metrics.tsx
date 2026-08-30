import { MetricItem, MetricStrip } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ProfitLossView } from "@/features/actual-profit-loss/types"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    formatMoneyDisplay,
    PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL,
} from "@/features/actual-profit-loss/lib/presentation"

export function ProfitLossMetrics({ data }: { data: ProfitLossView }) {
    return (
        <>
            <MetricStrip
                columns={5}
                aria-label="实际经营盈亏核心指标（非卡券·不含税）"
            >
                <MetricItem
                    label="不含税销售收入"
                    value={formatMoneyDisplay(data.totals.netSalesRevenue)}
                />
                <MetricItem
                    label="实际采购成本"
                    value={
                        data.totals.actualProcurementCostNet != null
                            ? formatMoneyDisplay(
                                  data.totals.actualProcurementCostNet,
                              )
                            : "无权限"
                    }
                    detail="实际发生+冲减 · 不含税"
                />
                <MetricItem
                    label="实际履约费用"
                    value={
                        data.totals.actualFulfillmentCostNet != null
                            ? formatMoneyDisplay(
                                  data.totals.actualFulfillmentCostNet,
                              )
                            : "无权限"
                    }
                    detail="印刷/仓储/配送等 · 不含税"
                />
                <MetricItem
                    label="实际经营盈亏"
                    value={
                        data.totals.actualProfitLossNet != null
                            ? formatMoneyDisplay(
                                  data.totals.actualProfitLossNet,
                              )
                            : (data.totals.marginUnavailableReason ?? "不可用")
                    }
                    detail={
                        data.coverage.reliability === "partial"
                            ? "部分覆盖 · 仅可靠子集"
                            : SCOPE_LABEL
                    }
                    status={
                        data.totals.actualProfitLossNet != null &&
                        compareDecimal(
                            data.totals.actualProfitLossNet,
                            "0",
                            2,
                        ) < 0
                            ? {
                                  label: "亏损",
                                  tone: "destructive",
                              }
                            : data.coverage.reliability === "partial"
                              ? {
                                    label: "部分可靠",
                                    tone: "warning",
                                }
                              : undefined
                    }
                />
                <MetricItem
                    label="实际利润率"
                    value={
                        data.totals.marginRate ??
                        data.totals.marginUnavailableReason ??
                        "不适用"
                    }
                    detail="盈亏 / 适用不含税收入"
                />
            </MetricStrip>

            <Alert>
                <AlertTitle>口径说明</AlertTitle>
                <AlertDescription className="text-xs leading-relaxed">
                    {data.formulaText}
                    <span className="mt-1 block">{data.excludedNote}</span>
                    <span className="mt-1 block">
                        当前范围：{data.filterSummary}
                    </span>
                </AlertDescription>
            </Alert>
        </>
    )
}
