import { MoneyValue } from "@/components/business/values"
import { BusinessStatusBadge } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Progress } from "@/components/ui/progress"
import type { ProfitLossView } from "@/features/actual-profit-loss/types"
import { coveragePercentNumber } from "@/features/actual-profit-loss/lib/url-state"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function CoverageAlert({ data }: { data: ProfitLossView }) {
    return (
        <Alert>
            <AlertTitle>
                成本归集完整度 · {SCOPE_LABEL} · 可靠性{" "}
                {data.coverage.reliability === "reliable"
                    ? "可靠"
                    : data.coverage.reliability === "partial"
                      ? "部分可靠"
                      : "待核算"}
            </AlertTitle>
            <AlertDescription className="space-y-3">
                <Progress
                    value={coveragePercentNumber(data.coverage.coverageRate)}
                >
                    <span className="text-xs">成本归集完整率</span>
                    <span className="num ml-auto text-sm">
                        {data.coverage.coverageRate}
                    </span>
                </Progress>
                <div className="grid gap-2 text-sm sm:grid-cols-3">
                    <div>
                        <span className="text-muted-foreground">
                            已归集成本收入{" "}
                        </span>
                        <span className="num font-medium">
                            <MoneyValue
                                value={data.coverage.coveredNetRevenue}
                            />
                        </span>
                    </div>
                    <div>
                        <span className="text-muted-foreground">
                            待归集成本收入{" "}
                        </span>
                        <span className="num font-medium">
                            <MoneyValue
                                value={data.coverage.uncoveredNetRevenue}
                            />
                        </span>
                    </div>
                    <div>
                        <span className="text-muted-foreground">
                            利润可靠性{" "}
                        </span>
                        <BusinessStatusBadge
                            context="detail"
                            label={
                                data.coverage.coverageState === "complete"
                                    ? "完整"
                                    : data.coverage.coverageState === "partial"
                                      ? "部分成本待归集"
                                      : "成本未归集"
                            }
                            tone={
                                data.coverage.coverageState === "complete"
                                    ? "success"
                                    : data.coverage.coverageState === "partial"
                                      ? "warning"
                                      : "destructive"
                            }
                        />
                    </div>
                </div>
                <p className="text-xs text-muted-foreground">
                    核心成本尚未入账的订单暂不计算盈亏，避免虚增利润。
                </p>
            </AlertDescription>
        </Alert>
    )
}
