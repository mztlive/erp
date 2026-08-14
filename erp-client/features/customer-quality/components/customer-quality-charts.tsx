"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import {
    DimensionBarChartCard,
    type CustomerQualityDimension,
} from "./dimension-bar-chart-card"
import { NatureDimensionList } from "./nature-dimension-list"

export type { CustomerQualityDimension }

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
                <DimensionBarChartCard
                    dimension={scaleDimension}
                    dimensionKey="scale"
                    tagKey="scaleTag"
                    titleFallback="客户规模分层"
                    description={(ruleVersion) => (
                        <>
                            点击柱形筛选明细
                            {ruleVersion
                                ? ` · 标签规则版本 v${ruleVersion}`
                                : ""}
                            。柱形颜色仅作区分，具体数值见下方表格。
                        </>
                    )}
                    tableCaption="客户规模分层等价数据表"
                    labelColumnHeader="分层"
                    valueColumnHeader="成交规模"
                    chartDimension={chartDimension}
                    chartCode={chartCode}
                    patchUrl={patchUrl}
                    setPagination={setPagination}
                />
                <DimensionBarChartCard
                    dimension={profitDimension}
                    dimensionKey="profit"
                    tagKey="profitTag"
                    titleFallback="利润贡献分布"
                    description={(ruleVersion) => (
                        <>
                            仅成本完整非卡券；卡券收入不进入利润标签
                            {ruleVersion
                                ? ` · 标签规则版本 v${ruleVersion}`
                                : ""}
                        </>
                    )}
                    tableCaption="利润贡献分布等价数据表"
                    labelColumnHeader="标签"
                    valueColumnHeader="盈亏（不含税）"
                    hiddenNotice={
                        isVoucherOnly ? (
                            <p className="text-sm text-muted-foreground">
                                当前为卡券业务性质筛选，利润贡献图隐藏。
                            </p>
                        ) : undefined
                    }
                    footer={
                        natureDimension ? (
                            <NatureDimensionList dimension={natureDimension} />
                        ) : undefined
                    }
                    chartDimension={chartDimension}
                    chartCode={chartCode}
                    patchUrl={patchUrl}
                    setPagination={setPagination}
                />
            </div>
        </>
    )
}
