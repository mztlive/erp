import type { CardBusinessAnalyticsView } from "../types"
import { BreakdownPanel } from "./charts/breakdown-panel"
import { ConsumptionTrendChart } from "./charts/consumption-trend-chart"
import { ContributionChart } from "./charts/contribution-chart"
import { CostBasisChart } from "./charts/cost-basis-chart"

/** Charts 2×2：消费/余额趋势、成本口径构成、经营贡献、类目/客户构成。 */
export function CardBusinessCharts({
    data,
}: {
    data: CardBusinessAnalyticsView
}) {
    return (
        <div className="grid min-w-0 gap-4 xl:grid-cols-2">
            <ConsumptionTrendChart points={data.trends.consumption} />
            <CostBasisChart coverage={data.coverage} />
            <ContributionChart
                points={data.trends.contribution}
                canViewProfit={data.fieldPermissions.canViewProfit}
            />
            <BreakdownPanel breakdowns={data.breakdowns} />
        </div>
    )
}
