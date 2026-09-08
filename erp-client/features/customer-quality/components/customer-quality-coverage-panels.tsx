"use client"

import { CostCoverageNotice } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { CustomerQualityView } from "../types"

export function CustomerQualityCoveragePanels({
    coverage,
    isVoucherOnly,
}: {
    coverage: CustomerQualityView["coverage"]
    isVoucherOnly: boolean
    periodFrom: string
    periodTo: string
}) {
    return (
        <>
            <div className="grid min-w-0 gap-4">
                <CostCoverageNotice
                    basis={coverage.costBasis}
                    coveragePercent={coverage.costCoveragePercent}
                    coverageLabel={coverage.costCoverageRate}
                    coverageState={coverage.costCoverageState}
                    breakdown={{
                        ACTUAL: coverage.costCoveredNetRevenue,
                        STANDARD: "—",
                        NONE: coverage.costUncoveredNetRevenue,
                    }}
                    profitBasis="非卡券净收入 − 实际净成本（不含税）；卡券不计入"
                    notice={
                        <>
                            成本覆盖收入{" "}
                            <span className="num">
                                {coverage.costCoveredNetRevenue}
                            </span>
                            、未覆盖收入{" "}
                            <span className="num">
                                {coverage.costUncoveredNetRevenue}
                            </span>
                            、覆盖率{" "}
                            <span className="num">
                                {coverage.costCoverageRate}
                            </span>
                            。缺失成本不显示为 0，利润须与覆盖率同屏解读。
                        </>
                    }
                />
            </div>

            {isVoucherOnly ? (
                <Alert variant="info">
                    <AlertTitle>业务性质：卡券</AlertTitle>
                    <AlertDescription>
                        卡券实际经营结果暂未提供；本页不显示卡券实际盈亏，卡券收入仍计入规模与回款分析。
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
