"use client"

import type * as React from "react"
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
import type { CustomerQualityView } from "../types"

const chartConfig = {
    value: { label: "数值", color: "var(--chart-1)" },
    active: { label: "选中", color: "var(--chart-2)" },
} satisfies ChartConfig

type CustomerQualityDimension = CustomerQualityView["dimensions"][number]

export function CustomerQualityCharts({
    scaleDimension,
    profitDimension,
    natureDimension,
    chartDimension,
    chartCode,
    isVoucherOnly,
    patchUrl,
    setPagination,
}: {
    scaleDimension?: CustomerQualityDimension
    profitDimension?: CustomerQualityDimension
    natureDimension?: CustomerQualityDimension
    chartDimension?: string
    chartCode?: string
    isVoucherOnly: boolean
    patchUrl: (patch: Record<string, string | null | undefined>) => void
    setPagination: React.Dispatch<React.SetStateAction<PaginationState>>
}) {
    return (
        <>
            {/* Charts + equivalent tables */}
            <div className="grid min-w-0 gap-4 xl:grid-cols-2">
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>
                            {scaleDimension?.title ?? "客户规模分层"}
                        </CardTitle>
                        <CardDescription>
                            点击柱形筛选明细
                            {scaleDimension?.ruleVersion
                                ? ` · 标签规则版本 v${scaleDimension.ruleVersion}`
                                : ""}
                            。柱形颜色仅作区分，具体数值见下方表格。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4 pt-4">
                        <ChartContainer
                            config={chartConfig}
                            className="aspect-[16/9] w-full"
                        >
                            <BarChart
                                data={[...(scaleDimension?.items ?? [])].map(
                                    (i) => ({
                                        label: i.label,
                                        code: i.code,
                                        value:
                                            Number(
                                                String(i.value).replace(
                                                    /[^\d.-]/g,
                                                    "",
                                                ),
                                            ) || 0,
                                        raw: i.value,
                                    }),
                                )}
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
                                    {(scaleDimension?.items ?? []).map(
                                        (item) => (
                                            <Cell
                                                key={item.code}
                                                cursor="pointer"
                                                fill={
                                                    chartDimension ===
                                                        "scale" &&
                                                    chartCode === item.code
                                                        ? "var(--color-active)"
                                                        : "var(--color-value)"
                                                }
                                                onClick={() => {
                                                    const nextActive =
                                                        chartDimension ===
                                                            "scale" &&
                                                        chartCode === item.code
                                                    patchUrl({
                                                        chartDimension:
                                                            nextActive
                                                                ? null
                                                                : "scale",
                                                        chartCode: nextActive
                                                            ? null
                                                            : item.code,
                                                        scaleTag: nextActive
                                                            ? null
                                                            : item.code,
                                                    })
                                                    setPagination((p) => ({
                                                        ...p,
                                                        pageIndex: 0,
                                                    }))
                                                }}
                                            />
                                        ),
                                    )}
                                </Bar>
                            </BarChart>
                        </ChartContainer>
                        <div className="overflow-x-auto">
                            <table className="w-full text-sm">
                                <caption className="sr-only">
                                    客户规模分层等价数据表
                                </caption>
                                <thead>
                                    <tr className="border-b text-left text-muted-foreground">
                                        <th className="py-1.5 pr-3 font-medium">
                                            分层
                                        </th>
                                        <th className="py-1.5 pr-3 font-medium">
                                            成交规模
                                        </th>
                                        <th className="py-1.5 pr-3 font-medium">
                                            占比
                                        </th>
                                        <th className="py-1.5 font-medium">
                                            户数
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {(scaleDimension?.items ?? []).map(
                                        (item) => {
                                            const selected =
                                                chartDimension === "scale" &&
                                                chartCode === item.code
                                            return (
                                                <tr
                                                    key={item.code}
                                                    className={
                                                        selected
                                                            ? "bg-accent/60"
                                                            : "border-b border-border/60"
                                                    }
                                                >
                                                    <td className="py-1.5 pr-3">
                                                        <Button
                                                            type="button"
                                                            size="xs"
                                                            variant={
                                                                selected
                                                                    ? "secondary"
                                                                    : "ghost"
                                                            }
                                                            aria-pressed={
                                                                selected
                                                            }
                                                            onClick={() => {
                                                                const nextActive =
                                                                    selected
                                                                patchUrl({
                                                                    chartDimension:
                                                                        nextActive
                                                                            ? null
                                                                            : "scale",
                                                                    chartCode:
                                                                        nextActive
                                                                            ? null
                                                                            : item.code,
                                                                    scaleTag:
                                                                        nextActive
                                                                            ? null
                                                                            : item.code,
                                                                })
                                                                setPagination(
                                                                    (p) => ({
                                                                        ...p,
                                                                        pageIndex: 0,
                                                                    }),
                                                                )
                                                            }}
                                                        >
                                                            {item.label}
                                                        </Button>
                                                    </td>
                                                    <td className="num py-1.5 pr-3">
                                                        {item.value}
                                                    </td>
                                                    <td className="num py-1.5 pr-3">
                                                        {item.share ?? "—"}
                                                    </td>
                                                    <td className="num py-1.5">
                                                        {item.count ?? "—"}
                                                    </td>
                                                </tr>
                                            )
                                        },
                                    )}
                                </tbody>
                            </table>
                        </div>
                    </CardContent>
                </Card>

                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>
                            {profitDimension?.title ?? "利润贡献分布"}
                        </CardTitle>
                        <CardDescription>
                            仅成本完整非卡券；卡券收入不进入利润标签
                            {profitDimension?.ruleVersion
                                ? ` · 标签规则版本 v${profitDimension.ruleVersion}`
                                : ""}
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4 pt-4">
                        {isVoucherOnly ? (
                            <p className="text-sm text-muted-foreground">
                                当前为卡券业务性质筛选，利润贡献图隐藏。
                            </p>
                        ) : (
                            <>
                                <ChartContainer
                                    config={chartConfig}
                                    className="aspect-[16/9] w-full"
                                >
                                    <BarChart
                                        data={[
                                            ...(profitDimension?.items ?? []),
                                        ].map((i) => ({
                                            label: i.label,
                                            code: i.code,
                                            value:
                                                Number(
                                                    String(i.value).replace(
                                                        /[^\d.-]/g,
                                                        "",
                                                    ),
                                                ) || 0,
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
                                            {(profitDimension?.items ?? []).map(
                                                (item) => (
                                                    <Cell
                                                        key={item.code}
                                                        cursor="pointer"
                                                        fill={
                                                            chartDimension ===
                                                                "profit" &&
                                                            chartCode ===
                                                                item.code
                                                                ? "var(--color-active)"
                                                                : "var(--color-value)"
                                                        }
                                                        onClick={() => {
                                                            const nextActive =
                                                                chartDimension ===
                                                                    "profit" &&
                                                                chartCode ===
                                                                    item.code
                                                            patchUrl({
                                                                chartDimension:
                                                                    nextActive
                                                                        ? null
                                                                        : "profit",
                                                                chartCode:
                                                                    nextActive
                                                                        ? null
                                                                        : item.code,
                                                                profitTag:
                                                                    nextActive
                                                                        ? null
                                                                        : item.code,
                                                            })
                                                            setPagination(
                                                                (p) => ({
                                                                    ...p,
                                                                    pageIndex: 0,
                                                                }),
                                                            )
                                                        }}
                                                    />
                                                ),
                                            )}
                                        </Bar>
                                    </BarChart>
                                </ChartContainer>
                                <div className="overflow-x-auto">
                                    <table className="w-full text-sm">
                                        <caption className="sr-only">
                                            利润贡献分布等价数据表
                                        </caption>
                                        <thead>
                                            <tr className="border-b text-left text-muted-foreground">
                                                <th className="py-1.5 pr-3 font-medium">
                                                    标签
                                                </th>
                                                <th className="py-1.5 pr-3 font-medium">
                                                    盈亏（不含税）
                                                </th>
                                                <th className="py-1.5 pr-3 font-medium">
                                                    占比
                                                </th>
                                                <th className="py-1.5 font-medium">
                                                    户数
                                                </th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {(profitDimension?.items ?? []).map(
                                                (item) => {
                                                    const selected =
                                                        chartDimension ===
                                                            "profit" &&
                                                        chartCode === item.code
                                                    return (
                                                        <tr
                                                            key={item.code}
                                                            className={
                                                                selected
                                                                    ? "bg-accent/60"
                                                                    : "border-b border-border/60"
                                                            }
                                                        >
                                                            <td className="py-1.5 pr-3">
                                                                <Button
                                                                    type="button"
                                                                    size="xs"
                                                                    variant={
                                                                        selected
                                                                            ? "secondary"
                                                                            : "ghost"
                                                                    }
                                                                    aria-pressed={
                                                                        selected
                                                                    }
                                                                    onClick={() => {
                                                                        const nextActive =
                                                                            selected
                                                                        patchUrl(
                                                                            {
                                                                                chartDimension:
                                                                                    nextActive
                                                                                        ? null
                                                                                        : "profit",
                                                                                chartCode:
                                                                                    nextActive
                                                                                        ? null
                                                                                        : item.code,
                                                                                profitTag:
                                                                                    nextActive
                                                                                        ? null
                                                                                        : item.code,
                                                                            },
                                                                        )
                                                                        setPagination(
                                                                            (
                                                                                p,
                                                                            ) => ({
                                                                                ...p,
                                                                                pageIndex: 0,
                                                                            }),
                                                                        )
                                                                    }}
                                                                >
                                                                    {item.label}
                                                                </Button>
                                                            </td>
                                                            <td className="num py-1.5 pr-3">
                                                                {item.value}
                                                            </td>
                                                            <td className="num py-1.5 pr-3">
                                                                {item.share ??
                                                                    "—"}
                                                            </td>
                                                            <td className="num py-1.5">
                                                                {item.count ??
                                                                    "—"}
                                                            </td>
                                                        </tr>
                                                    )
                                                },
                                            )}
                                        </tbody>
                                    </table>
                                </div>
                            </>
                        )}
                        {natureDimension ? (
                            <div className="border-t pt-3">
                                <p className="mb-2 text-sm font-medium">
                                    {natureDimension.title}
                                </p>
                                <ul className="grid gap-1 text-sm sm:grid-cols-2">
                                    {natureDimension.items.map((item) => (
                                        <li
                                            key={item.code}
                                            className="flex justify-between gap-2 text-muted-foreground"
                                        >
                                            <span>
                                                {item.label}
                                                {item.code === "VOUCHER" ? (
                                                    <span className="ml-1 text-xs">
                                                        （计规模/回款，不计盈亏）
                                                    </span>
                                                ) : null}
                                            </span>
                                            <span className="num">
                                                {item.value} · {item.share}
                                            </span>
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        ) : null}
                    </CardContent>
                </Card>
            </div>
        </>
    )
}
