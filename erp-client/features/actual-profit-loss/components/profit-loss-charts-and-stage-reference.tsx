import {
    Bar,
    BarChart,
    CartesianGrid,
    Legend,
    Line,
    LineChart,
    XAxis,
    YAxis,
} from "recharts"

import { surfacePanelClassName } from "@/components/business"
import { Badge } from "@/components/ui/badge"
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
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import type { ProfitLossView } from "../types"
import {
    formatMoneyDisplay,
    PROFIT_LOSS_SCOPE_LABEL,
} from "@/features/actual-profit-loss/lib/presentation"

const trendChartConfig = {
    revenue: { label: "不含税收入", color: "var(--chart-1)" },
    cost: { label: "实际成本", color: "var(--chart-2)" },
    profit: { label: "实际盈亏", color: "var(--chart-3)" },
} satisfies ChartConfig

const compositionChartConfig = {
    net: { label: "不含税金额", color: "var(--chart-4)" },
} satisfies ChartConfig

export function ProfitLossChartsAndStageReference({
    data,
}: {
    data: ProfitLossView
}) {
    const trendChartData = data.trend.map((point) => ({
        period: point.period,
        revenue: Number(point.netSalesRevenue) / 10000,
        cost:
            point.actualCostNet === "—"
                ? null
                : Number(point.actualCostNet) / 10000,
        profit:
            point.actualProfitLossNet == null
                ? null
                : Number(point.actualProfitLossNet) / 10000,
        reliability: point.reliability,
    }))
    const compositionChartData = data.fieldPermissions.canViewCost
        ? data.costComposition
              .filter(
                  (composition) =>
                      composition.netAmount !== "—" &&
                      Number(composition.netAmount) !== 0,
              )
              .map((composition) => ({
                  label: composition.label,
                  net: Number(composition.netAmount) / 10000,
                  share: composition.share,
              }))
        : []

    return (
        <>
            <div className="grid min-w-0 gap-4 xl:grid-cols-[3fr_2fr]">
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>
                            盈亏趋势（{PROFIT_LOSS_SCOPE_LABEL} · 万元）
                        </CardTitle>
                        <CardDescription>
                            收入 / 实际成本 /
                            实际盈亏。趋势为固定口径序列，不随期间与覆盖筛选变化；指标与明细已按当前筛选汇总。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="pt-4">
                        {data.fieldPermissions.canViewProfit ? (
                            <>
                                <ChartContainer
                                    config={trendChartConfig}
                                    className="aspect-[16/9] w-full"
                                >
                                    <LineChart
                                        data={trendChartData}
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
                                        <Line
                                            type="monotone"
                                            dataKey="revenue"
                                            name="不含税收入"
                                            stroke="var(--color-revenue)"
                                            strokeWidth={2}
                                            dot={false}
                                        />
                                        <Line
                                            type="monotone"
                                            dataKey="cost"
                                            name="实际成本"
                                            stroke="var(--color-cost)"
                                            strokeWidth={2}
                                            strokeDasharray="4 4"
                                            dot={false}
                                        />
                                        <Line
                                            type="monotone"
                                            dataKey="profit"
                                            name="实际盈亏"
                                            stroke="var(--color-profit)"
                                            strokeWidth={2}
                                            dot={false}
                                        />
                                    </LineChart>
                                </ChartContainer>
                                <div className="mt-3 overflow-x-auto">
                                    <table className="w-full text-left text-xs">
                                        <caption className="sr-only">
                                            盈亏趋势数据表（单位：元，非卡券不含税；图内以万元展示）
                                        </caption>
                                        <thead>
                                            <tr className="border-b text-muted-foreground">
                                                <th className="py-1 pr-2">
                                                    期间
                                                </th>
                                                <th className="py-1 pr-2 text-right">
                                                    收入
                                                </th>
                                                <th className="py-1 pr-2 text-right">
                                                    成本
                                                </th>
                                                <th className="py-1 pr-2 text-right">
                                                    盈亏
                                                </th>
                                                <th className="py-1">可靠性</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {data.trend.map((t) => (
                                                <tr
                                                    key={t.period}
                                                    className="border-b border-border/60"
                                                >
                                                    <td className="py-1 pr-2">
                                                        {t.period}
                                                    </td>
                                                    <td className="num py-1 pr-2 text-right">
                                                        {formatMoneyDisplay(
                                                            t.netSalesRevenue,
                                                        )}
                                                    </td>
                                                    <td className="num py-1 pr-2 text-right">
                                                        {formatMoneyDisplay(
                                                            t.actualCostNet,
                                                        )}
                                                    </td>
                                                    <td className="num py-1 pr-2 text-right">
                                                        {t.actualProfitLossNet !=
                                                        null
                                                            ? formatMoneyDisplay(
                                                                  t.actualProfitLossNet,
                                                              )
                                                            : "不可用"}
                                                    </td>
                                                    <td className="py-1 text-muted-foreground">
                                                        {t.reliability ===
                                                        "reliable"
                                                            ? "可靠"
                                                            : t.reliability ===
                                                                "partial"
                                                              ? "部分可靠"
                                                              : "不可用"}
                                                    </td>
                                                </tr>
                                            ))}
                                        </tbody>
                                    </table>
                                </div>
                            </>
                        ) : (
                            <p className="text-sm text-muted-foreground">
                                无利润查看权限，趋势图暂不展示。
                            </p>
                        )}
                    </CardContent>
                </Card>

                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>
                            成本构成（{PROFIT_LOSS_SCOPE_LABEL}）
                        </CardTitle>
                        <CardDescription>
                            仅统计实际成本与冲减；返点等冲减显示为负值贡献。构成与图表为固定口径序列，不随覆盖筛选变化。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="pt-4">
                        {data.fieldPermissions.canViewCost &&
                        compositionChartData.length > 0 ? (
                            <>
                                <ChartContainer
                                    config={compositionChartConfig}
                                    className="aspect-[16/9] w-full"
                                >
                                    <BarChart
                                        data={compositionChartData}
                                        accessibilityLayer
                                        layout="vertical"
                                        margin={{ left: 48 }}
                                    >
                                        <CartesianGrid horizontal={false} />
                                        <XAxis
                                            type="number"
                                            tickLine={false}
                                            axisLine={false}
                                        />
                                        <YAxis
                                            type="category"
                                            dataKey="label"
                                            tickLine={false}
                                            axisLine={false}
                                            width={72}
                                        />
                                        <ChartTooltip
                                            content={<ChartTooltipContent />}
                                        />
                                        <Bar
                                            dataKey="net"
                                            name="不含税金额（万元）"
                                            fill="var(--color-net)"
                                            radius={4}
                                        />
                                    </BarChart>
                                </ChartContainer>
                                <ul className="mt-3 space-y-1 text-xs">
                                    {data.costComposition
                                        .filter(
                                            (c) =>
                                                c.netAmount !== "—" &&
                                                Number(c.netAmount) !== 0,
                                        )
                                        .map((c) => (
                                            <li
                                                key={c.costType}
                                                className="flex justify-between gap-2 border-b border-border/50 py-1"
                                            >
                                                <span>{c.label}</span>
                                                <span className="num">
                                                    {formatMoneyDisplay(
                                                        c.netAmount,
                                                    )}
                                                    {c.share ? (
                                                        <span className="ml-2 text-muted-foreground">
                                                            {c.share}
                                                        </span>
                                                    ) : null}
                                                </span>
                                            </li>
                                        ))}
                                </ul>
                            </>
                        ) : (
                            <p className="text-sm text-muted-foreground">
                                {data.fieldPermissions.canViewCost
                                    ? "当前范围无成本构成数据。"
                                    : "无成本明细权限；不展示构成占比，避免通过图表比例泄露。"}
                            </p>
                        )}
                    </CardContent>
                </Card>
            </div>

            {/* EXPECTED / CONFIRMED 对照区 */}
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>预计/已确认成本参考</CardTitle>
                    <CardDescription>
                        仅执行期对照，不参与实际经营盈亏或实际利润率；与实际成本使用不同文案样式。
                    </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                    <div className="grid gap-3 md:grid-cols-2">
                        {data.stageReference.map((line) => (
                            <div
                                key={line.stage}
                                className="rounded-lg border border-dashed border-border bg-muted/30 p-3"
                            >
                                <div className="flex items-center justify-between gap-2">
                                    <span className="text-sm font-medium">
                                        {line.label}
                                    </span>
                                    <Badge variant="outline">
                                        {line.stage === "EXPECTED"
                                            ? "预计"
                                            : "已确认"}
                                    </Badge>
                                </div>
                                <DescriptionList columns="two" className="mt-2">
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            采购（对照）
                                        </DescriptionTerm>
                                        <DescriptionDetails>
                                            <span className="num text-muted-foreground">
                                                {formatMoneyDisplay(
                                                    line.procurementCostNet,
                                                )}
                                            </span>
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            履约（对照）
                                        </DescriptionTerm>
                                        <DescriptionDetails>
                                            <span className="num text-muted-foreground">
                                                {formatMoneyDisplay(
                                                    line.fulfillmentCostNet,
                                                )}
                                            </span>
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            合计（对照）
                                        </DescriptionTerm>
                                        <DescriptionDetails>
                                            <span className="num text-muted-foreground">
                                                {formatMoneyDisplay(
                                                    line.totalNet,
                                                )}
                                            </span>
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                </DescriptionList>
                                <p className="mt-2 text-xs text-muted-foreground">
                                    {line.note}
                                </p>
                            </div>
                        ))}
                    </div>
                </CardContent>
            </Card>
        </>
    )
}
