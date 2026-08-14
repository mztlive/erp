import Link from "next/link"

import { CostCoverageNotice } from "@/components/business"
import { Button } from "@/components/ui/button"
import { formatMoneyDisplay } from "../lib/presentation"
import type { CardBusinessAnalyticsView } from "../types"
import { COVERAGE_STATUS_UI } from "../types"

export interface CardBusinessCostCoverageProps {
    data: CardBusinessAnalyticsView
}

/** CostCoverageNotice — 强制在利润指标前展示成本覆盖情况。 */
export function CardBusinessCostCoverage({
    data,
}: CardBusinessCostCoverageProps) {
    return (
        <CostCoverageNotice
            basis={data.coverage.dominantBasis}
            coveragePercent={data.coverage.ratePercent}
            coverageLabel={data.coverage.rate ?? "—"}
            coverageState={
                COVERAGE_STATUS_UI[data.coverage.status].noticeState
            }
            breakdown={{
                ACTUAL: (
                    <span>
                        消费{" "}
                        {formatMoneyDisplay(
                            data.coverage.byBasis.find(
                                (b) => b.basis === "ACTUAL",
                            )?.consumptionGross,
                        )}{" "}
                        ·{" "}
                        {data.coverage.byBasis.find(
                            (b) => b.basis === "ACTUAL",
                        )?.shareLabel ?? "—"}
                        {data.fieldPermissions.canViewCost ? (
                            <>
                                {" "}
                                · 成本{" "}
                                {formatMoneyDisplay(
                                    data.coverage.byBasis.find(
                                        (b) => b.basis === "ACTUAL",
                                    )?.costNet,
                                )}
                            </>
                        ) : null}
                    </span>
                ),
                STANDARD: (
                    <span>
                        消费{" "}
                        {formatMoneyDisplay(
                            data.coverage.byBasis.find(
                                (b) => b.basis === "STANDARD",
                            )?.consumptionGross,
                        )}{" "}
                        ·{" "}
                        {data.coverage.byBasis.find(
                            (b) => b.basis === "STANDARD",
                        )?.shareLabel ?? "—"}
                        <span className="block text-xs text-muted-foreground">
                            按历史有效供给价估算，非实际
                        </span>
                    </span>
                ),
                NONE: (
                    <span>
                        消费{" "}
                        {formatMoneyDisplay(
                            data.coverage.byBasis.find(
                                (b) => b.basis === "NONE",
                            )?.consumptionGross,
                        )}{" "}
                        ·{" "}
                        {data.coverage.byBasis.find(
                            (b) => b.basis === "NONE",
                        )?.shareLabel ?? "—"}
                        <span className="block text-xs text-muted-foreground">
                            无可用成本，不显示金额，不计入利润
                        </span>
                    </span>
                ),
            }}
            profitBasis="不含税；当前经营贡献不等于最终利润，须结合未履约余额查看。"
            notice={
                <>
                    {data.coverage.notice}
                    {data.coverage.profitReferenceOnly ? (
                        <span className="mt-1 block font-medium">
                            成本不完整，结果仅供参考。
                        </span>
                    ) : null}
                    <span className="mt-2 flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            render={
                                <Link
                                    href={data.governanceLinks.noneCoverageHref}
                                />
                            }
                        >
                            查看未归集（接口错误中心）
                        </Button>
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            render={
                                <Link
                                    href={data.governanceLinks.backfillHref}
                                />
                            }
                        >
                            历史消费回填
                        </Button>
                    </span>
                </>
            }
        />
    )
}
