import {
    Bar,
    BarChart,
    CartesianGrid,
    Legend,
    Line,
    XAxis,
    YAxis,
} from "recharts"

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
import {
    Table,
    TableBody,
    TableCaption,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { formatMoneyDisplay } from "../../lib/presentation"
import type { CardBusinessAnalyticsView } from "../../types"

const consumptionChartConfig = {
    sales: { label: "销售(含税)", color: "var(--chart-1)" },
    consumption: { label: "消费(含税)", color: "var(--chart-2)" },
    refund: { label: "退款(含税)", color: "var(--chart-3)" },
    balance: { label: "余额(含税)", color: "var(--chart-4)" },
} satisfies ChartConfig

export interface ConsumptionTrendChartProps {
    points: CardBusinessAnalyticsView["trends"]["consumption"]
}

/** 消费与余额趋势（含税，万元展示）+ 键盘/读屏等价数据表。 */
export function ConsumptionTrendChart({ points }: ConsumptionTrendChartProps) {
    // fixed-decimal-display-boundary: Recharts coordinates require number.
    const consumptionChartData = points.map((point) => ({
        period: point.period,
        // fixed-decimal-display-boundary: Recharts coordinates require number.
        sales: Number(point.salesGross) / 10000,
        // fixed-decimal-display-boundary: Recharts coordinates require number.
        consumption: Number(point.consumptionGross) / 10000,
        // fixed-decimal-display-boundary: Recharts coordinates require number.
        refund: Number(point.refundGross) / 10000,
        // fixed-decimal-display-boundary: Recharts coordinates require number.
        balance: Number(point.balanceGross) / 10000,
        salesLabel: formatMoneyDisplay(point.salesGross),
        consumptionLabel: formatMoneyDisplay(point.consumptionGross),
        refundLabel: formatMoneyDisplay(point.refundGross),
        balanceLabel: formatMoneyDisplay(point.balanceGross),
    }))

    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
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
                    <BarChart data={consumptionChartData} accessibilityLayer>
                        <CartesianGrid vertical={false} />
                        <XAxis
                            dataKey="period"
                            tickLine={false}
                            axisLine={false}
                        />
                        <YAxis tickLine={false} axisLine={false} width={40} />
                        <ChartTooltip content={<ChartTooltipContent />} />
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
                <Table className="caption-top" data-density="compact">
                    <TableCaption className="mb-2 mt-0 text-left">
                        消费与余额趋势数据表（与图等价）
                    </TableCaption>
                    <TableHeader>
                        <TableRow>
                            <TableHead>周</TableHead>
                            <TableHead>销售(含税)</TableHead>
                            <TableHead>消费(含税)</TableHead>
                            <TableHead>退款(含税)</TableHead>
                            <TableHead>余额(含税)</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {consumptionChartData.map((r) => (
                            <TableRow key={r.period}>
                                <TableHead
                                    scope="row"
                                    className="bg-transparent font-medium text-foreground"
                                >
                                    {r.period}
                                </TableHead>
                                <TableCell className="num">
                                    {r.salesLabel}
                                </TableCell>
                                <TableCell className="num">
                                    {r.consumptionLabel}
                                </TableCell>
                                <TableCell className="num">
                                    {r.refundLabel}
                                </TableCell>
                                <TableCell className="num">
                                    {r.balanceLabel}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </CardContent>
        </Card>
    )
}
