import { Bar, BarChart, CartesianGrid, Cell, XAxis, YAxis } from "recharts"

import { surfacePanelClassName } from "@/components/business"
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
import type { CardBusinessAnalyticsView, CostBasisCode } from "../../types"
import { COST_BASIS_LABEL } from "../../types"

const basisChartConfig = {
    ACTUAL: { label: "实际成本", color: "var(--chart-1)" },
    STANDARD: { label: "标准成本", color: "var(--chart-2)" },
    NONE: { label: "无可用成本", color: "var(--chart-3)" },
} satisfies ChartConfig

const BASIS_COLORS: Record<CostBasisCode, string> = {
    ACTUAL: "var(--chart-1)",
    STANDARD: "var(--chart-2)",
    NONE: "var(--chart-3)",
}

export interface CostBasisChartProps {
    coverage: CardBusinessAnalyticsView["coverage"]
}

/** 成本口径构成：实际/标准/无可用成本消费金额占比 + 等价数据表。 */
export function CostBasisChart({ coverage }: CostBasisChartProps) {
    const basisChartData = coverage.byBasis.map((slice) => ({
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

    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>成本口径构成</CardTitle>
                <CardDescription>
                    实际成本 / 标准成本 /
                    无可用成本消费金额占比。三者合计须等于累计卡券消费{" "}
                    {formatMoneyDisplay(coverage.totalConsumptionGross)}
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
                        <XAxis dataKey="label" tickLine={false} axisLine={false} />
                        <YAxis tickLine={false} axisLine={false} width={40} />
                        <ChartTooltip content={<ChartTooltipContent />} />
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
                            成本口径构成数据表（名称 · 金额 · 占比 · 成本）
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
                                    className="border-b border-grid"
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
                                <td className="num py-1 pr-2" colSpan={3}>
                                    {formatMoneyDisplay(
                                        coverage.totalConsumptionGross,
                                    )}{" "}
                                    = 累计卡券消费
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </CardContent>
        </Card>
    )
}
