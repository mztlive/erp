"use client"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { cn } from "@/lib/utils"

function SettlementCenterOverview({
    detail,
    patchUrl,
}: {
    detail: SettlementDetailView
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
}) {
    const st = detail.statement
    return (
        <Card
            size="sm"
            className={cn(surfaceInsetClassName, "shadow-none ring-0")}
        >
            <CardHeader className="rounded-t-lg border-b border-grid py-3">
                <CardTitle className="text-base">概览</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 pt-4 text-sm">
                <p>
                    供应商：{st.supplierName}（记录时，不受后续更名影响）
                </p>
                <p className="num">
                    期间：{st.periodStart} ~ {st.periodEnd}
                </p>
                <p>状态：{st.statusLabel}</p>
                <p>
                    未决阻断差异：
                    {detail.differenceSummary.blocking} / 差异合计{" "}
                    {detail.differenceSummary.total}
                </p>
                <p className="text-muted-foreground">
                    账单/订单/成本原值只读，不可在本页改写以消差。
                </p>
                <div className="flex flex-wrap gap-2 pt-2">
                    <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={() => patchUrl({ section: "differences" })}
                    >
                        打开差异处理
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => patchUrl({ section: "items" })}
                    >
                        查看结算明细
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}

export { SettlementCenterOverview }
