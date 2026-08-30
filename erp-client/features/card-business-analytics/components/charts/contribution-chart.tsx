import { CartesianGrid, Legend, Line, LineChart, XAxis, YAxis } from "recharts"

import {
    BusinessEmptyState,
    surfacePanelClassName,
} from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    ChartContainer,
    ChartTooltip,
    ChartTooltipContent,
    type ChartConfig,
} from "@/components/ui/chart"
import { formatMoneyDisplay } from "../../lib/presentation"
import type { CardBusinessAnalyticsView } from "../../types"

const contributionChartConfig = {
    contribution: { label: "经营贡献(不含税)", color: "var(--chart-1)" },
    margin: { label: "消费毛差(不含税)", color: "var(--chart-2)" },
    coverage: { label: "覆盖率%", color: "var(--chart-4)" },
} satisfies ChartConfig

export interface ContributionChartProps {
    points: CardBusinessAnalyticsView["trends"]["contribution"]
    canViewProfit: boolean
}

/** 经营贡献与覆盖率趋势（利润金额不含税；无权限时隐藏并提示）。 */
export function ContributionChart({
    points,
    canViewProfit,
}: ContributionChartProps) {
    // fixed-decimal-display-boundary: Recharts coordinates require number.
    const contributionChartData = points.map((point) => ({
        period: point.period,
        // fixed-decimal-display-boundary: Recharts coordinates require number.
        contribution: Number(point.contributionNet) / 10000,
        // fixed-decimal-display-boundary: Recharts coordinates require number.
        margin: Number(point.marginNet) / 10000,
        coverage: point.coveragePercent,
        contributionLabel: formatMoneyDisplay(point.contributionNet),
        marginLabel: formatMoneyDisplay(point.marginNet),
        coverageLabel: point.coverageRate,
    }))

    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>经营贡献与覆盖率</CardTitle>
                <CardDescription>
                    利润金额不含税；覆盖率同屏辅助。全量口径，与指标一致，不随明细筛选变化。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
                {canViewProfit ? (
                    <>
                        <ChartContainer
                            config={contributionChartConfig}
                            className="aspect-[16/9] w-full"
                        >
                            <LineChart
                                data={contributionChartData}
                                accessibilityLayer
                            >
                                <CartesianGrid vertical={false} />
                                <XAxis
                                    dataKey="period"
                                    tickLine={false}
                                    axisLine={false}
                                />
                                <YAxis
                                    yAxisId="left"
                                    tickLine={false}
                                    axisLine={false}
                                    width={40}
                                />
                                <YAxis
                                    yAxisId="right"
                                    orientation="right"
                                    tickLine={false}
                                    axisLine={false}
                                    width={40}
                                    domain={[0, 100]}
                                />
                                <ChartTooltip
                                    content={<ChartTooltipContent />}
                                />
                                <Legend />
                                <Line
                                    yAxisId="left"
                                    type="monotone"
                                    dataKey="contribution"
                                    stroke="var(--color-contribution)"
                                    name="经营贡献(万元)"
                                    strokeWidth={2}
                                    dot={false}
                                />
                                <Line
                                    yAxisId="left"
                                    type="monotone"
                                    dataKey="margin"
                                    stroke="var(--color-margin)"
                                    name="消费毛差(万元)"
                                    strokeWidth={2}
                                    dot={false}
                                />
                                <Line
                                    yAxisId="right"
                                    type="monotone"
                                    dataKey="coverage"
                                    stroke="var(--color-coverage)"
                                    name="覆盖率%"
                                    strokeWidth={2}
                                    strokeDasharray="4 4"
                                    dot={false}
                                />
                            </LineChart>
                        </ChartContainer>
                        <div className="overflow-x-auto">
                            <table className="w-full text-left text-xs">
                                <caption className="mb-2 text-left text-muted-foreground">
                                    经营贡献趋势数据表（与图等价）
                                </caption>
                                <thead>
                                    <tr className="border-b text-muted-foreground">
                                        <th scope="col" className="py-1 pr-2">
                                            周
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            经营贡献(不含税)
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            消费毛差(不含税)
                                        </th>
                                        <th scope="col" className="py-1">
                                            覆盖率
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {contributionChartData.map((r) => (
                                        <tr
                                            key={r.period}
                                            className="border-b border-grid"
                                        >
                                            <th
                                                scope="row"
                                                className="py-1 pr-2 font-medium"
                                            >
                                                {r.period}
                                            </th>
                                            <td className="num py-1 pr-2">
                                                {r.contributionLabel}
                                            </td>
                                            <td className="num py-1 pr-2">
                                                {r.marginLabel}
                                            </td>
                                            <td className="num py-1">
                                                {r.coverageLabel}
                                            </td>
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        </div>
                    </>
                ) : (
                    <BusinessEmptyState
                        kind="no-scope"
                        title="无利润字段权限"
                        description="经营贡献趋势已隐藏；覆盖率与风险等级仍按授权可见。"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    />
                )}
            </CardContent>
        </Card>
    )
}
