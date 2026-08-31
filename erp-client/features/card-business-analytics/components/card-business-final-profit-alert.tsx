import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { CardBusinessAnalyticsView } from "../types"

export interface CardBusinessFinalProfitAlertProps {
    data: CardBusinessAnalyticsView
    onSwitchToExpiry: () => void
}

/** 履约范围未全部到期时：最终利润不展示 + 一键切换履约到期日视角。 */
export function CardBusinessFinalProfitAlert({
    data,
    onSwitchToExpiry,
}: CardBusinessFinalProfitAlertProps) {
    if (data.scopeFullyExpired) return null
    return (
        <Alert>
            <AlertTitle>最终利润未展示</AlertTitle>
            <AlertDescription className="flex flex-wrap items-center gap-2">
                <span>
                    {data.finalProfitUnavailableReason}
                    当前同屏展示「当前经营贡献」与「未履约余额」。若需最终盈亏视角，可将日期口径切换为履约到期日并筛选已到期范围。
                </span>
                <Button
                    id="card-contracts-analytics-final-profit-switch-expiry"
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={onSwitchToExpiry}
                >
                    切换为履约到期日 + 已到期
                </Button>
            </AlertDescription>
        </Alert>
    )
}
