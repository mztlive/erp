"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type {
    BatchSection,
    ImportBatchView,
} from "@/features/import-opening/types"
import { OBJECT_CODE_LABEL } from "@/features/import-opening/types"

export function OverviewSection({
    batch,
    onGoSection,
}: {
    batch: ImportBatchView
    onGoSection: (s: BatchSection) => void
}) {
    return (
        <>
            <DetailWorkspaceSection
                title="预检摘要"
                subtitle="此处展示本次数据预检的整体统计结果，具体异常可在明细中筛选查看。"
            >
                <DetailWorkspaceMetrics
                    variant="inline"
                    columns={4}
                    ariaLabel="预检指标"
                >
                    <DetailWorkspaceMetricItem
                        emphasis="primary"
                        label="总行数"
                        value={batch.metrics.total}
                    />
                    <DetailWorkspaceMetricItem
                        label="待导入"
                        value={batch.metrics.valid}
                    />
                    <DetailWorkspaceMetricItem
                        label="冲突"
                        value={batch.metrics.conflict}
                    />
                    <DetailWorkspaceMetricItem
                        label="问题"
                        value={batch.metrics.failed}
                    />
                </DetailWorkspaceMetrics>
                <div className="flex flex-wrap gap-2">
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
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="期初口径"
                subtitle="以下为本批次导入数据的口径说明，仅供核对参考。"
            >
                <div className="space-y-3">
                    {batch.openingPolicyHints.length > 0 ? (
                        batch.openingPolicyHints.map((hint) => (
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
                        ))
                    ) : (
                        <p className="text-body-compact text-muted-foreground">
                            当前批次无额外期初口径说明。
                        </p>
                    )}
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
                </div>
            </DetailWorkspaceSection>
        </>
    )
}
