import Link from "next/link"
import {
    Bar,
    BarChart,
    CartesianGrid,
    Cell,
    Legend,
    Line,
    LineChart,
    XAxis,
    YAxis,
} from "recharts"

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
import type { CardBusinessAnalyticsView, CostBasisCode } from "../types"
import { COST_BASIS_LABEL } from "../types"
import { formatMoneyDisplay } from "../lib/presentation"

const consumptionChartConfig = {
    sales: { label: "销售(含税)", color: "var(--chart-1)" },
    consumption: { label: "消费(含税)", color: "var(--chart-2)" },
    refund: { label: "退款(含税)", color: "var(--chart-3)" },
    balance: { label: "余额(含税)", color: "var(--chart-4)" },
} satisfies ChartConfig

const basisChartConfig = {
    ACTUAL: { label: "实际成本", color: "var(--chart-1)" },
    STANDARD: { label: "标准成本", color: "var(--chart-2)" },
    NONE: { label: "无可用成本", color: "var(--chart-3)" },
} satisfies ChartConfig

const contributionChartConfig = {
    contribution: { label: "经营贡献(不含税)", color: "var(--chart-1)" },
    margin: { label: "消费毛差(不含税)", color: "var(--chart-2)" },
    coverage: { label: "覆盖率%", color: "var(--chart-4)" },
} satisfies ChartConfig

const BASIS_COLORS: Record<CostBasisCode, string> = {
    ACTUAL: "var(--chart-1)",
    STANDARD: "var(--chart-2)",
    NONE: "var(--chart-3)",
}

export function CardBusinessCharts({
    data,
}: {
    data: CardBusinessAnalyticsView
}) {
    const consumptionChartData = data.trends.consumption.map((point) => ({
        period: point.period,
        sales: Number(point.salesGross) / 10000,
        consumption: Number(point.consumptionGross) / 10000,
        refund: Number(point.refundGross) / 10000,
        balance: Number(point.balanceGross) / 10000,
        salesLabel: formatMoneyDisplay(point.salesGross),
        consumptionLabel: formatMoneyDisplay(point.consumptionGross),
        refundLabel: formatMoneyDisplay(point.refundGross),
        balanceLabel: formatMoneyDisplay(point.balanceGross),
    }))
    const basisChartData = data.coverage.byBasis.map((slice) => ({
        basis: slice.basis,
        label: COST_BASIS_LABEL[slice.basis],
        amount: Number(slice.consumptionGross) / 10000,
        amountLabel: formatMoneyDisplay(slice.consumptionGross),
        share: slice.shareLabel,
        costLabel:
            slice.basis === "NONE"
                ? "不计入成本"
                : formatMoneyDisplay(slice.costNet),
    }))
    const contributionChartData = data.trends.contribution.map((point) => ({
        period: point.period,
        contribution: Number(point.contributionNet) / 10000,
        margin: Number(point.marginNet) / 10000,
        coverage: point.coveragePercent,
        contributionLabel: formatMoneyDisplay(point.contributionNet),
        marginLabel: formatMoneyDisplay(point.marginNet),
        coverageLabel: point.coverageRate,
    }))

    return (
        <>
            {/* Charts 2×2 */}
            <div className="grid min-w-0 gap-4 xl:grid-cols-2">
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>消费与余额趋势</CardTitle>
                        <CardDescription>
                            销售 / 消费 / 退款 /
                            余额（含税，万元展示）。全量口径，不随明细筛选变化。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4 pt-4">
                        <ChartContainer
                            config={consumptionChartConfig}
                            className="aspect-[16/9] w-full"
                        >
                            <BarChart
                                data={consumptionChartData}
                                accessibilityLayer
                            >
                                <CartesianGrid vertical={false} />
                                <XAxis
                                    dataKey="period"
                                    tickLine={false}
                                    axisLine={false}
                                />
                                <YAxis
                                    tickLine={false}
                                    axisLine={false}
                                    width={40}
                                />
                                <ChartTooltip
                                    content={<ChartTooltipContent />}
                                />
                                <Legend />
                                <Bar
                                    dataKey="sales"
                                    fill="var(--color-sales)"
                                    radius={4}
                                    name="销售(含税)"
                                />
                                <Bar
                                    dataKey="consumption"
                                    fill="var(--color-consumption)"
                                    radius={4}
                                    name="消费(含税)"
                                />
                                <Bar
                                    dataKey="refund"
                                    fill="var(--color-refund)"
                                    radius={4}
                                    name="退款(含税)"
                                />
                                <Line
                                    type="monotone"
                                    dataKey="balance"
                                    stroke="var(--color-balance)"
                                    strokeWidth={2}
                                    strokeDasharray="4 4"
                                    dot={false}
                                    name="余额(含税)"
                                />
                            </BarChart>
                        </ChartContainer>
                        {/* 键盘/读屏等价数据表 */}
                        <div className="overflow-x-auto">
                            <table className="w-full text-left text-xs">
                                <caption className="mb-2 text-left text-muted-foreground">
                                    消费与余额趋势数据表（与图等价）
                                </caption>
                                <thead>
                                    <tr className="border-b text-muted-foreground">
                                        <th scope="col" className="py-1 pr-2">
                                            周
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            销售(含税)
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            消费(含税)
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            退款(含税)
                                        </th>
                                        <th scope="col" className="py-1">
                                            余额(含税)
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {consumptionChartData.map((r) => (
                                        <tr
                                            key={r.period}
                                            className="border-b border-border/60"
                                        >
                                            <th
                                                scope="row"
                                                className="py-1 pr-2 font-medium"
                                            >
                                                {r.period}
                                            </th>
                                            <td className="num py-1 pr-2">
                                                {r.salesLabel}
                                            </td>
                                            <td className="num py-1 pr-2">
                                                {r.consumptionLabel}
                                            </td>
                                            <td className="num py-1 pr-2">
                                                {r.refundLabel}
                                            </td>
                                            <td className="num py-1">
                                                {r.balanceLabel}
                                            </td>
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        </div>
                    </CardContent>
                </Card>

                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>成本口径构成</CardTitle>
                        <CardDescription>
                            实际成本 / 标准成本 /
                            无可用成本消费金额占比。三者合计须等于累计卡券消费{" "}
                            {formatMoneyDisplay(
                                data.coverage.totalConsumptionGross,
                            )}
                            。全量口径，不随明细筛选变化。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4 pt-4">
                        <ChartContainer
                            config={basisChartConfig}
                            className="aspect-[16/9] w-full"
                        >
                            <BarChart data={basisChartData} accessibilityLayer>
                                <CartesianGrid vertical={false} />
                                <XAxis
                                    dataKey="label"
                                    tickLine={false}
                                    axisLine={false}
                                />
                                <YAxis
                                    tickLine={false}
                                    axisLine={false}
                                    width={40}
                                />
                                <ChartTooltip
                                    content={<ChartTooltipContent />}
                                />
                                <Bar
                                    dataKey="amount"
                                    radius={4}
                                    name="消费额(万元)"
                                >
                                    {basisChartData.map((entry) => (
                                        <Cell
                                            key={entry.basis}
                                            fill={
                                                BASIS_COLORS[
                                                    entry.basis as CostBasisCode
                                                ]
                                            }
                                        />
                                    ))}
                                </Bar>
                            </BarChart>
                        </ChartContainer>
                        <div className="overflow-x-auto">
                            <table className="w-full text-left text-xs">
                                <caption className="mb-2 text-left text-muted-foreground">
                                    成本口径构成数据表（名称 · 金额 · 占比 ·
                                    成本）
                                </caption>
                                <thead>
                                    <tr className="border-b text-muted-foreground">
                                        <th scope="col" className="py-1 pr-2">
                                            口径
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            消费(含税)
                                        </th>
                                        <th scope="col" className="py-1 pr-2">
                                            占比
                                        </th>
                                        <th scope="col" className="py-1">
                                            成本(不含税)
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {basisChartData.map((r) => (
                                        <tr
                                            key={r.basis}
                                            className="border-b border-border/60"
                                        >
                                            <th
                                                scope="row"
                                                className="py-1 pr-2 font-medium"
                                            >
                                                {r.label}
                                            </th>
                                            <td className="num py-1 pr-2">
                                                {r.amountLabel}
                                            </td>
                                            <td className="num py-1 pr-2">
                                                {r.share}
                                            </td>
                                            <td className="num py-1">
                                                {r.costLabel}
                                            </td>
                                        </tr>
                                    ))}
                                    <tr className="font-medium">
                                        <th scope="row" className="py-1 pr-2">
                                            合计
                                        </th>
                                        <td
                                            className="num py-1 pr-2"
                                            colSpan={3}
                                        >
                                            {formatMoneyDisplay(
                                                data.coverage
                                                    .totalConsumptionGross,
                                            )}{" "}
                                            = 累计卡券消费
                                        </td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>
                    </CardContent>
                </Card>

                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>经营贡献与覆盖率</CardTitle>
                        <CardDescription>
                            利润金额不含税；覆盖率同屏辅助。全量口径，与指标一致，不随明细筛选变化。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4 pt-4">
                        {data.fieldPermissions.canViewProfit ? (
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
                                                <th
                                                    scope="col"
                                                    className="py-1 pr-2"
                                                >
                                                    周
                                                </th>
                                                <th
                                                    scope="col"
                                                    className="py-1 pr-2"
                                                >
                                                    经营贡献(不含税)
                                                </th>
                                                <th
                                                    scope="col"
                                                    className="py-1 pr-2"
                                                >
                                                    消费毛差(不含税)
                                                </th>
                                                <th
                                                    scope="col"
                                                    className="py-1"
                                                >
                                                    覆盖率
                                                </th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {contributionChartData.map((r) => (
                                                <tr
                                                    key={r.period}
                                                    className="border-b border-border/60"
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

                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>类目 / 客户构成</CardTitle>
                        <CardDescription>
                            排名不越过数据范围。全量口径，不随明细筛选变化。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="grid gap-4 pt-4 sm:grid-cols-2">
                        <div>
                            <h3 className="mb-2 text-sm font-medium">按类目</h3>
                            <ul className="space-y-2 text-sm">
                                {data.breakdowns.byCategory.map((item) => (
                                    <li
                                        key={item.id}
                                        className="flex items-center justify-between gap-2"
                                    >
                                        <span>{item.label}</span>
                                        <span className="num text-muted-foreground">
                                            {formatMoneyDisplay(
                                                item.consumptionGross,
                                            )}{" "}
                                            · {item.share}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        </div>
                        <div>
                            <h3 className="mb-2 text-sm font-medium">按客户</h3>
                            <ul className="space-y-2 text-sm">
                                {data.breakdowns.byCustomer.map((item) => (
                                    <li
                                        key={item.id}
                                        className="flex items-center justify-between gap-2"
                                    >
                                        <Link
                                            href={`/sales/customers/${item.id}`}
                                            className="underline-offset-2 hover:underline"
                                        >
                                            {item.label}
                                        </Link>
                                        <span className="num text-muted-foreground">
                                            {formatMoneyDisplay(
                                                item.consumptionGross,
                                            )}{" "}
                                            · {item.share}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        </div>
                    </CardContent>
                </Card>
            </div>
        </>
    )
}
