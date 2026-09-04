"use client"

import Link from "next/link"

import {
    CostCoverageNotice,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { openWorkspaceLabel } from "@/lib/ui-text"
import type { CustomerQualityView } from "../types"

export function CustomerQualityCoveragePanels({
    coverage,
    isVoucherOnly,
    onShowReviewedOnly,
}: {
    coverage: CustomerQualityView["coverage"]
    isVoucherOnly: boolean
    periodFrom: string
    periodTo: string
    onShowReviewedOnly: () => void
}) {
    return (
        <>
            {/* Coverage: card funds + cost — always co-displayed with affected metrics */}
            <div className="grid min-w-0 gap-4 xl:grid-cols-2">
                <Card
                    size="sm"
                    className={surfacePanelClassName}
                    data-slot="card-funds-coverage-notice"
                >
                    <CardHeader className="border-b border-grid">
                        <CardTitle>卡券票款复核进度</CardTitle>
                        <CardDescription>
                            与受影响应收指标同屏；未复核不得假装可靠。
                        </CardDescription>
                        <CardAction>
                            <Badge
                                variant={
                                    coverage.cardFundsState === "complete"
                                        ? "success"
                                        : "warning"
                                }
                            >
                                {coverage.cardFundsReviewRate}
                            </Badge>
                        </CardAction>
                    </CardHeader>
                    <CardContent className="space-y-3 pt-4">
                        <p className="text-sm">
                            已复核{" "}
                            <span className="num font-medium">
                                {coverage.reviewedVoucherOrderCount}
                            </span>{" "}
                            / 应复核{" "}
                            <span className="num font-medium">
                                {coverage.requiredVoucherOrderCount}
                            </span>{" "}
                            张卡券销售单（{coverage.cardFundsReviewRate}）
                        </p>
                        {coverage.cardFundsState !== "complete" ? (
                            <Alert variant="warning">
                                <AlertTitle>票款复核不足</AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center gap-2">
                                    应收余额、逾期金额等指标标记为部分可靠。可切换「仅已复核」或前往卡券票款复核。
                                    <Button
                                        id="customers-quality-show-reviewed-only"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={onShowReviewedOnly}
                                    >
                                        仅看已复核
                                    </Button>
                                    <Button
                                        id="customers-quality-open-card-review"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        render={
                                            <Link href="/finance/card-funds-review?from=W15" />
                                        }
                                    >
                                        {openWorkspaceLabel("W13")}
                                    </Button>
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </CardContent>
                </Card>

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
