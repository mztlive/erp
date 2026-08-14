import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { surfacePanelClassName } from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { ItemsTable } from "@/features/history-backfill/components/items-table"
import type {
    HistoryBackfillItemView,
    HistoryBackfillJobCore,
} from "@/features/history-backfill/types"
import { COST_BASIS_LABEL } from "@/features/history-backfill/types"

export function CostSection({
    job,
    items,
}: {
    job: HistoryBackfillJobCore
    items: HistoryBackfillItemView[]
}) {
    return (
        <div className="space-y-4">
            <div className="grid gap-3 md:grid-cols-3">
                {job.costBasis.map((row) => (
                    <Card key={row.basis} className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30 pb-2">
                            <CardTitle className="text-base">
                                {COST_BASIS_LABEL[row.basis]}
                            </CardTitle>
                            <CardDescription>
                                {row.count.toLocaleString("zh-CN")} 笔
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-1 text-sm">
                            <div>
                                消费金额（含税）：{row.consumptionAmountGross}
                            </div>
                            <div>
                                成本净额：
                                {row.basis === "NONE"
                                    ? "空（禁止写 0）"
                                    : (row.costAmountNet ?? "—")}
                            </div>
                        </CardContent>
                    </Card>
                ))}
            </div>
            <Alert>
                <AlertTitle>禁止当前供给价</AlertTitle>
                <AlertDescription>
                    时点标准成本必须命中消费发生时点有效供给版本；未覆盖
                    不得用当前价、猜测税率或销项税率替代进项。覆盖率{" "}
                    {job.coverageRate ?? "—"}（未覆盖进分母）。
                </AlertDescription>
            </Alert>
            <Separator />
            <ItemsTable
                items={items.filter(
                    (i) =>
                        i.costBasis === "ACTUAL" ||
                        i.costBasis === "STANDARD" ||
                        i.costBasis === "NONE",
                )}
                section="facts"
                title="成本口径明细"
                totalCount={
                    items.filter(
                        (i) =>
                            i.costBasis === "ACTUAL" ||
                            i.costBasis === "STANDARD" ||
                            i.costBasis === "NONE",
                    ).length
                }
                page={1}
                onPageChange={() => undefined}
            />
        </div>
    )
}
