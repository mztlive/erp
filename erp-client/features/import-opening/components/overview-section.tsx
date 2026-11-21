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
                    <CardTitle>预检摘要</CardTitle>
                    <CardDescription>
                        此处展示本次数据预检的整体统计结果，具体异常可在明细中筛选查看。
                    </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                    <MetricStrip
                        variant="plain"
                        columns={4}
                        aria-label="预检指标"
                    >
                        <MetricItem
                            label="总行数"
                            value={batch.metrics.total}
                        />
                        <MetricItem
                            label="待导入"
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
                        以下为本批次导入数据的口径说明，仅供核对参考。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.openingPolicyHints.map((hint) => (
                        <div
                            key={hint.objectCode}
                            className="space-y-1 text-body-compact"
                        >
                            <div className="font-medium">
                                {OBJECT_CODE_LABEL[hint.objectCode]}
                            </div>
                            <p className="text-muted-foreground">
                                {hint.message}
                            </p>
                        </div>
                    ))}
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
                        ）。数据或规则已更新，需按最新预检结果重新核对后才能正式导入。
                    </AlertDescription>
                </Alert>
            ) : null}
        </div>
    )
}
