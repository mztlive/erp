"use client"

import { BusinessStatusBadge } from "@/components/business"
import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { RecommendationStatusAlert } from "@/features/procurement-confirmation/components/legacy-plan-recommendation-alert"
import { LegacyPlanSubmissionSection } from "@/features/procurement-confirmation/components/legacy-plan-submission-section"
import type {
    ConfirmationLineDraft,
    CoverageByLine,
    FulfillmentMode,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"

type SelectionOption = {
    value: string
    label: string
}

type LegacyProcurementPlanEditorProps = {
    task: ProcurementConfirmationTask
    recommendation: ProcurementRecommendation | undefined
    recommendationPending: boolean
    recommendationFailed: boolean
    lineDrafts: readonly ConfirmationLineDraft[]
    coverage: readonly CoverageByLine[]
    supplyOptions: readonly ProcurementSupplyOption[]
    supplierOptions: readonly SupplierComboboxItem[] | undefined
    offeringOptionsForSku: (skuId: string) => readonly SelectionOption[]
    capabilityOptionsForOffering: (
        offeringRevisionId: string,
        fulfillmentMode: FulfillmentMode,
    ) => readonly SelectionOption[]
    fulfillmentOptionsForOffering: (
        offeringRevisionId: string,
    ) => readonly SelectionOption[]
    formalPending: boolean
    onApplyRecommendation: () => void
    onUpdateLine: (
        lineKey: string,
        patch: Partial<ConfirmationLineDraft>,
    ) => void
    onAddSplitLine: (submissionLineId: string) => void
    onRemoveLine: (lineKey: string) => void
    saveMessage: string | null
    dirty: boolean
}

export function LegacyProcurementPlanEditor({
    task,
    recommendation,
    recommendationPending,
    recommendationFailed,
    lineDrafts,
    coverage,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    formalPending,
    onApplyRecommendation,
    onUpdateLine,
    onAddSplitLine,
    onRemoveLine,
    saveMessage,
    dirty,
}: LegacyProcurementPlanEditorProps) {
    return (
        <Card className="hidden min-w-0" size="sm">
            <CardHeader className="border-b">
                <div className="flex flex-wrap items-center gap-2">
                    <CardTitle>
                        <h2
                            tabIndex={-1}
                            className="outline-none"
                            aria-live="polite"
                        >
                            最低成本采购方案
                        </h2>
                    </CardTitle>
                    <BusinessStatusBadge
                        context="list"
                        label={recommendation?.ready ? "可执行" : "待补齐"}
                        tone={recommendation?.ready ? "success" : "warning"}
                    />
                    <Badge variant="secondary">系统推荐 · 可人工调整</Badge>
                </div>
                <CardDescription>
                    按供应商当前有效供给、能力、起订量、可供量、采购价及费用组合；审批时系统重新校验。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-5">
                <RecommendationStatusAlert
                    isPending={recommendationPending}
                    isError={recommendationFailed}
                    recommendation={recommendation}
                />

                <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">
                        推荐策略：
                        {recommendation?.policyVersion ?? "正在读取"}
                    </p>
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            formalPending ||
                            recommendationPending ||
                            !recommendation?.ready
                        }
                        onClick={onApplyRecommendation}
                    >
                        重新载入最低成本方案
                    </Button>
                </div>

                <Separator />

                <section aria-labelledby="confirm-lines-title">
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                        <h3
                            id="confirm-lines-title"
                            className="text-sm font-semibold"
                        >
                            采购明细
                        </h3>
                        <span className="text-xs text-muted-foreground">
                            同一商品可向多个供应商采购；每条销售明细分别核对数量
                        </span>
                    </div>

                    <div className="space-y-5">
                        {task.salesSubmission.lines.map((subLine) => {
                            const lines = lineDrafts.filter(
                                (l) =>
                                    l.submissionLineId ===
                                    subLine.submissionLineId,
                            )
                            const cov = coverage.find(
                                (c) =>
                                    c.submissionLineId ===
                                    subLine.submissionLineId,
                            )
                            return (
                                <LegacyPlanSubmissionSection
                                    key={subLine.submissionLineId}
                                    subLine={subLine}
                                    lines={lines}
                                    cov={cov}
                                    formalPending={formalPending}
                                    supplyOptions={supplyOptions}
                                    supplierOptions={supplierOptions}
                                    offeringOptionsForSku={
                                        offeringOptionsForSku
                                    }
                                    capabilityOptionsForOffering={
                                        capabilityOptionsForOffering
                                    }
                                    fulfillmentOptionsForOffering={
                                        fulfillmentOptionsForOffering
                                    }
                                    onUpdateLine={onUpdateLine}
                                    onAddSplitLine={onAddSplitLine}
                                    onRemoveLine={onRemoveLine}
                                />
                            )
                        })}
                    </div>
                </section>

                {saveMessage ? (
                    <p className="text-xs text-muted-foreground" role="status">
                        {saveMessage}
                        {dirty ? " · 之后有未保存修改" : null}
                    </p>
                ) : dirty ? (
                    <p
                        className="text-xs text-warning-soft-foreground"
                        role="status"
                    >
                        有未保存的确认分行修改（⌘S 保存）
                    </p>
                ) : null}
            </CardContent>
        </Card>
    )
}
