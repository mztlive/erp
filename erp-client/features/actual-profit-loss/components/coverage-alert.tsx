import { BusinessStatusBadge } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Progress } from "@/components/ui/progress"
import type { ProfitLossView } from "@/features/actual-profit-loss/types"
import { coveragePercentNumber } from "@/features/actual-profit-loss/lib/url-state"
import {
    formatMoneyDisplay,
    PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL,
} from "@/features/actual-profit-loss/lib/presentation"

export function CoverageAlert({ data }: { data: ProfitLossView }) {
    return (
        <Alert>
            <AlertTitle>
                成本覆盖 · {SCOPE_LABEL} · 可靠性{" "}
                {data.coverage.reliability === "reliable"
                    ? "可靠"
                    : data.coverage.reliability === "partial"
                      ? "部分可靠"
                      : "不可用"}
            </AlertTitle>
            <AlertDescription className="space-y-3">
                <Progress
                    value={coveragePercentNumber(data.coverage.coverageRate)}
                >
                    <span className="text-xs">成本覆盖率</span>
                    <span className="num ml-auto text-sm">
                        {data.coverage.coverageRate}
                    </span>
                </Progress>
                <div className="grid gap-2 text-sm sm:grid-cols-3">
                    <div>
                        <span className="text-muted-foreground">覆盖收入 </span>
                        <span className="num font-medium">
                            {formatMoneyDisplay(
                                data.coverage.coveredNetRevenue,
                            )}
                        </span>
                    </div>
                    <div>
                        <span className="text-muted-foreground">
                            未覆盖收入{" "}
                        </span>
                        <span className="num font-medium">
                            {formatMoneyDisplay(
                                data.coverage.uncoveredNetRevenue,
                            )}
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
                                      ? "部分覆盖"
                                      : "完全未覆盖"
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
                    缺失成本显示为未覆盖并注明原因，不会按零成本计利润。
                </p>
            </AlertDescription>
        </Alert>
    )
}
