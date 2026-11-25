import {
    DecisionKpiBar,
    DecisionKpiCell,
    MoneyValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { StatusBadge } from "@/components/ui/status-badge"
import type { ProfitLossView } from "@/features/actual-profit-loss/types"
import { compareDecimal } from "@/lib/fixed-decimal"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function ProfitLossMetrics({ data }: { data: ProfitLossView }) {
    const { canViewRevenue, canViewCost, canViewProfit } = data.fieldPermissions
    const columns =
        canViewCost && canViewProfit
            ? canViewRevenue
                ? 5
                : 4
            : canViewCost || canViewProfit
              ? canViewRevenue
                  ? 3
                  : 2
              : canViewRevenue
                ? 1
                : null
    const profitLossStatus =
        data.totals.actualProfitLossNet != null &&
        compareDecimal(data.totals.actualProfitLossNet, "0", 2) < 0
            ? ({
                  label: "亏损",
                  tone: "destructive",
              } as const)
            : data.coverage.reliability === "partial"
              ? ({
                    label: "部分可靠",
                    tone: "warning",
                } as const)
              : undefined

    return (
        <>
            {columns !== null && (
                <DecisionKpiBar
                    columns={columns === 5 ? 4 : columns}
                    gridClassName={
                        columns === 5
                            ? "@[960px]/decision-kpi:grid-cols-5"
                            : undefined
                    }
                    ariaLabel="实际经营盈亏核心指标（非卡券·不含税）"
                >
                    {canViewRevenue && (
                        <DecisionKpiCell
                            label="不含税销售收入"
                            value={
                                <MoneyValue
                                    display="compact"
                                    value={data.totals.netSalesRevenue}
                                />
                            }
                        />
                    )}
                    {canViewCost && (
                        <DecisionKpiCell
                            label="实际采购成本"
                            value={
                                data.totals.actualProcurementCostNet != null ? (
                                    <MoneyValue
                                        display="compact"
                                        value={
                                            data.totals.actualProcurementCostNet
                                        }
                                    />
                                ) : (
                                    "—"
                                )
                            }
                            footnote="实际发生 · 冲减另行加回"
                        />
                    )}
                    {canViewCost && (
                        <DecisionKpiCell
                            label="实际履约费用"
                            value={
                                data.totals.actualFulfillmentCostNet != null ? (
                                    <MoneyValue
                                        display="compact"
                                        value={
                                            data.totals.actualFulfillmentCostNet
                                        }
                                    />
                                ) : (
                                    "—"
                                )
                            }
                            footnote="印刷/仓储/配送等 · 不含税"
                        />
                    )}
                    {canViewProfit && (
                        <DecisionKpiCell
                            label="实际经营盈亏"
                            value={
                                data.totals.actualProfitLossNet != null ? (
                                    <MoneyValue
                                        display="compact"
                                        value={data.totals.actualProfitLossNet}
                                    />
                                ) : (
                                    (data.totals.marginUnavailableReason ??
                                    "不可用")
                                )
                            }
                            valueMeta={
                                profitLossStatus ? (
                                    <StatusBadge {...profitLossStatus} />
                                ) : undefined
                            }
                            footnote={
                                data.coverage.reliability === "partial"
                                    ? "部分成本待归集"
                                    : SCOPE_LABEL
                            }
                        />
                    )}
                    {canViewProfit && (
                        <DecisionKpiCell
                            label="实际利润率"
                            value={
                                data.totals.marginRate ??
                                data.totals.marginUnavailableReason ??
                                "不适用"
                            }
                            footnote="盈亏 / 适用不含税收入"
                        />
                    )}
                </DecisionKpiBar>
            )}

            <Alert>
                <AlertTitle>口径说明</AlertTitle>
                <AlertDescription className="text-xs">
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
