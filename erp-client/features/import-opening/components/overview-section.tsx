"use client"

import Link from "next/link"
import { ExternalLinkIcon, TriangleAlertIcon } from "lucide-react"

import {
    MetricItem,
    MetricStrip,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    BatchSection,
    ImportBatchView,
} from "@/features/import-opening/types"
import { OBJECT_CODE_LABEL } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

export function OverviewSection({
    batch,
    onGoSection,
}: {
    batch: ImportBatchView
    onGoSection: (s: BatchSection) => void
}) {
    return (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>试算摘要</CardTitle>
                    <CardDescription>
                        试算统计由系统统一计算，与问题明细可能因筛选存在差异。
                    </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                    <MetricStrip columns={4} aria-label="试算指标">
                        <MetricItem
                            label="总行数"
                            value={batch.metrics.total}
                        />
                        <MetricItem
                            label="可应用"
                            value={batch.metrics.valid}
                        />
                        <MetricItem
                            label="冲突"
                            value={batch.metrics.conflict}
                        />
                        <MetricItem label="问题" value={batch.metrics.failed} />
                    </MetricStrip>
                    <div className="mt-4 flex flex-wrap gap-2">
                        <Button
                            id="operations-import-batch-detail-overview-view-trial"
                            type="button"
                            size="sm"
                            variant="secondary"
                            onClick={() => onGoSection("trial")}
                        >
                            查看问题明细
                        </Button>
                        <Button
                            id="operations-import-batch-detail-overview-view-confirm"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => onGoSection("confirm")}
                        >
                            责任确认
                        </Button>
                    </div>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>期初口径</CardTitle>
                    <CardDescription>
                        提示按本批对象固定生成，不可修改。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.openingPolicyHints.map((hint) => (
                        <div
                            key={hint.objectCode}
                            className="space-y-1 text-sm"
                        >
                            <div className="font-medium">
                                {OBJECT_CODE_LABEL[hint.objectCode]}
                            </div>
                            <p className="text-muted-foreground">
                                {hint.message}
                            </p>
                        </div>
                    ))}
                    {batch.sourceObjectSet.includes("CARD_OPENING_AR") ||
                    batch.sourceObjectSet.includes("CARD_SALES_ORDER") ? (
                        <Button
                            id="operations-import-batch-detail-overview-card-funds-review"
                            size="sm"
                            variant="outline"
                            render={<Link href="/finance/card-funds-review" />}
                        >
                            前往卡券票款复核
                            <ExternalLinkIcon className="size-4" />
                        </Button>
                    ) : null}
                    {batch.sourceObjectSet.includes("OPENING_STOCK") ? (
                        <Button
                            id="operations-import-batch-detail-overview-inventory-link"
                            size="sm"
                            variant="outline"
                            render={<Link href="/inventory?view=balance" />}
                        >
                            查看库存台账
                            <ExternalLinkIcon className="size-4" />
                        </Button>
                    ) : null}
                </CardContent>
            </Card>

            {batch.invalidation ? (
                <Alert variant="warning" className="lg:col-span-2">
                    <TriangleAlertIcon />
                    <AlertTitle>旧确认已失效</AlertTitle>
                    <AlertDescription>
                        {batch.invalidation.reason}（
                        {formatDateTime(
                            batch.invalidation.invalidatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                        ）。禁止按旧试算版本{" "}
                        <span className="num font-mono">
                            {batch.invalidation.previousTrialVersion}
                        </span>{" "}
                        提交应用。
                    </AlertDescription>
                </Alert>
            ) : null}
        </div>
    )
}
