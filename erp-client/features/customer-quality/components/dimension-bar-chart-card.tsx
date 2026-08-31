"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { Bar, BarChart, CartesianGrid, Cell, XAxis, YAxis } from "recharts"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CustomerQualityView } from "../types"

const chartConfig = {
    value: { label: "数值", color: "var(--chart-1)" },
    active: { label: "选中", color: "var(--chart-2)" },
} satisfies ChartConfig

export type CustomerQualityDimension = CustomerQualityView["dimensions"][number]

export function DimensionBarChartCard({
    dimension,
    dimensionKey,
    tagKey,
    titleFallback,
    description,
    tableCaption,
    labelColumnHeader,
    valueColumnHeader,
    hiddenNotice,
    footer,
    chartDimension,
    chartCode,
    patchUrl,
    setPagination,
}: {
    dimension?: CustomerQualityDimension
    dimensionKey: "scale" | "profit"
    tagKey: "scaleTag" | "profitTag"
    titleFallback: string
    description: (ruleVersion?: string) => React.ReactNode
    tableCaption: string
    labelColumnHeader: string
    valueColumnHeader: string
    hiddenNotice?: React.ReactNode
    footer?: React.ReactNode
    chartDimension?: string
    chartCode?: string
    patchUrl: (patch: Record<string, string | null | undefined>) => void
    setPagination: React.Dispatch<React.SetStateAction<PaginationState>>
}) {
    const items = dimension?.items ?? []

    function selectItem(code: string) {
        const nextActive = chartDimension === dimensionKey && chartCode === code
        patchUrl({
            chartDimension: nextActive ? null : dimensionKey,
            chartCode: nextActive ? null : code,
            [tagKey]: nextActive ? null : code,
        })
        setPagination((p) => ({ ...p, pageIndex: 0 }))
    }

    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>{dimension?.title ?? titleFallback}</CardTitle>
                <CardDescription>
                    {description(dimension?.ruleVersion)}
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
                {hiddenNotice ? (
                    hiddenNotice
                ) : (
                    <>
                        <ChartContainer
                            config={chartConfig}
                            className="aspect-[16/9] w-full"
                        >
                            <BarChart
                                data={[...items].map((i) => ({
                                    label: i.label,
                                    code: i.code,
                                    value:
                                        // fixed-decimal-display-boundary: Recharts coordinates require number.
                                        Number(
                                            String(i.value).replace(
                                                /[^\d.-]/g,
                                                "",
                                            ),
                                        ) || 0,
                                    raw: i.value,
                                }))}
                                accessibilityLayer
                            >
                                <CartesianGrid vertical={false} />
                                <XAxis
                                    dataKey="label"
                                    tickLine={false}
                                    axisLine={false}
                                />
                                <YAxis
                                    tickLine={false}
                                    axisLine={false}
                                    width={48}
                                />
                                <ChartTooltip
                                    content={<ChartTooltipContent />}
                                />
                                <Bar dataKey="value" radius={4}>
                                    {items.map((item) => (
                                        <Cell
                                            id={`customers-quality-chart-${toAutomationIdSegment(dimensionKey)}-${toAutomationIdSegment(item.code)}-bar`}
                                            key={item.code}
                                            cursor="pointer"
                                            fill={
                                                chartDimension ===
                                                    dimensionKey &&
                                                chartCode === item.code
                                                    ? "var(--color-active)"
                                                    : "var(--color-value)"
                                            }
                                            onClick={() =>
                                                selectItem(item.code)
                                            }
                                        />
                                    ))}
                                </Bar>
                            </BarChart>
                        </ChartContainer>
                        <Table data-density="compact">
                            <TableCaption className="sr-only">
                                {tableCaption}
                            </TableCaption>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>{labelColumnHeader}</TableHead>
                                    <TableHead>{valueColumnHeader}</TableHead>
                                    <TableHead>占比</TableHead>
                                    <TableHead>户数</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {items.map((item) => {
                                    const selected =
                                        chartDimension === dimensionKey &&
                                        chartCode === item.code
                                    return (
                                        <TableRow
                                            key={item.code}
                                            data-state={
                                                selected
                                                    ? "selected"
                                                    : undefined
                                            }
                                        >
                                            <TableCell>
                                                <Button
                                                    id={`customers-quality-chart-${toAutomationIdSegment(dimensionKey)}-${toAutomationIdSegment(item.code)}-filter`}
                                                    type="button"
                                                    size="xs"
                                                    variant={
                                                        selected
                                                            ? "secondary"
                                                            : "ghost"
                                                    }
                                                    aria-pressed={selected}
                                                    onClick={() =>
                                                        selectItem(item.code)
                                                    }
                                                >
                                                    {item.label}
                                                </Button>
                                            </TableCell>
                                            <TableCell className="num">
                                                {item.value}
                                            </TableCell>
                                            <TableCell className="num">
                                                {item.share ?? "—"}
                                            </TableCell>
                                            <TableCell className="num">
                                                {item.count ?? "—"}
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    </>
                )}
                {footer}
            </CardContent>
        </Card>
    )
}
