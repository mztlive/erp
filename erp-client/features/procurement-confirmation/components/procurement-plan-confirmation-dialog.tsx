"use client"

import { CheckIcon, CircleCheckIcon, TriangleAlertIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { BusinessFailureState, ValidationSummary } from "@/components/business"
import type { ValidationIssue } from "@/components/business/feedback"
import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { PlanSummaryCards } from "@/features/procurement-confirmation/components/plan-dialog-summary-cards"
import {
    PlanSubmissionSection,
    type PlanCoverage,
} from "@/features/procurement-confirmation/components/plan-dialog-submission-section"
import { money } from "@/features/procurement-confirmation/lib/format"
import type {
    ConfirmationLineDraft,
    FulfillmentMode,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
} from "@/features/procurement-confirmation/types"

type SelectionOption = {
    value: string
    label: string
}

type ProcurementPlanConfirmationDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    task: ProcurementConfirmationTask
    recommendation: ProcurementRecommendation | undefined
    recommendationPending: boolean
    recommendationFailed: boolean
    recommendationError: unknown
    onRetryRecommendation: () => void
    currentPlanSummary: {
        purchaseGross: number
        grossMargin: number
        orderCount: number
    }
    dirty: boolean
    lineDrafts: readonly ConfirmationLineDraft[]
    coverage: readonly PlanCoverage[]
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
    updatePlanLine: (
        lineKey: string,
        patch: Partial<ConfirmationLineDraft>,
    ) => void
    addSplitLine: (submissionLineId: string) => void
    removeLine: (lineKey: string) => void
    clientBlocking: readonly ValidationIssue[]
    allCovered: boolean
    formalPending: boolean
    isSubmitting: boolean
    actionError: string | null
    onApprove: () => Promise<void>
}

export function ProcurementPlanConfirmationDialog({
    open,
    onOpenChange,
    task,
    recommendation,
    recommendationPending,
    recommendationFailed,
    recommendationError,
    onRetryRecommendation,
    currentPlanSummary,
    dirty,
    lineDrafts,
    coverage,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    updatePlanLine,
    addSplitLine,
    removeLine,
    clientBlocking,
    allCovered,
    formalPending,
    isSubmitting,
    actionError,
    onApprove,
}: ProcurementPlanConfirmationDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-h-[88vh] w-[calc(100vw-2rem)] overflow-x-hidden overflow-y-auto sm:max-w-6xl">
                <DialogHeader>
                    <DialogTitle>确认采购方案</DialogTitle>
                    <DialogDescription>
                        销售单 {task.salesSubmission.salesOrderNo}
                        。系统按采购数量匹配报价档位，并在可供范围内组合预计成本最低的采购方案。
                    </DialogDescription>
                </DialogHeader>

                {recommendationPending ? (
                    <div className="rounded-lg border border-border bg-muted/30 p-6 text-center text-sm text-muted-foreground">
                        正在核对供应商报价、起订量和可供数量…
                    </div>
                ) : recommendationFailed ? (
                    <BusinessFailureState
                        title="采购方案计算失败"
                        error={recommendationError}
                        onRetry={() => onRetryRecommendation()}
                    />
                ) : recommendation?.ready ? (
                    <div className="space-y-4">
                        <Alert variant="success">
                            <CircleCheckIcon aria-hidden="true" />
                            <AlertTitle>
                                当前方案预计生成 {currentPlanSummary.orderCount}{" "}
                                张采购单
                            </AlertTitle>
                            <AlertDescription>
                                当前采购含税{" "}
                                {money.format(currentPlanSummary.purchaseGross)}
                                ，预计毛利{" "}
                                {money.format(currentPlanSummary.grossMargin)}。
                            </AlertDescription>
                        </Alert>

                        <PlanSummaryCards
                            salesGross={recommendation.salesGross}
                            purchaseGross={currentPlanSummary.purchaseGross}
                            grossMargin={currentPlanSummary.grossMargin}
                        />

                        <section
                            className="space-y-3"
                            aria-labelledby="editable-plan-title"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div>
                                    <h3
                                        id="editable-plan-title"
                                        className="font-semibold"
                                    >
                                        当前采购安排
                                    </h3>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        可以调整供应商、数量、交付方式和交期；采购价会按该供应商承接的合计数量自动重算。
                                    </p>
                                </div>
                                <Badge
                                    variant={dirty ? "secondary" : "outline"}
                                >
                                    {dirty ? "已人工调整" : "系统初始方案"}
                                </Badge>
                            </div>

                            {task.salesSubmission.lines.map((subLine) => {
                                const planLines = lineDrafts.filter(
                                    (line) =>
                                        line.submissionLineId ===
                                        subLine.submissionLineId,
                                )
                                const lineCoverage = coverage.find(
                                    (item) =>
                                        item.submissionLineId ===
                                        subLine.submissionLineId,
                                )
                                return (
                                    <PlanSubmissionSection
                                        key={subLine.submissionLineId}
                                        subLine={subLine}
                                        planLines={planLines}
                                        lineCoverage={lineCoverage}
                                        allLineDrafts={lineDrafts}
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
                                        updatePlanLine={updatePlanLine}
                                        addSplitLine={addSplitLine}
                                        removeLine={removeLine}
                                    />
                                )
                            })}

                            {clientBlocking.length > 0 ? (
                                <ValidationSummary
                                    title="生成采购单前需要补齐"
                                    issues={clientBlocking}
                                />
                            ) : null}
                        </section>

                        {recommendation.warnings.length > 0 ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>确认前请核对</AlertTitle>
                                <AlertDescription>
                                    {recommendation.warnings
                                        .map((item) => item.message)
                                        .join("；")}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </div>
                ) : (
                    <Alert variant="destructive">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>暂时无法形成完整采购方案</AlertTitle>
                        <AlertDescription>
                            {recommendation?.blockingIssues
                                .map((item) => item.message)
                                .join("；") || "当前供应商报价或可供数量不足。"}
                        </AlertDescription>
                    </Alert>
                )}

                {actionError ? (
                    <Alert variant="destructive">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>生成采购单未完成</AlertTitle>
                        <AlertDescription>{actionError}</AlertDescription>
                    </Alert>
                ) : null}

                <DialogFooter>
                    <DialogClose
                        render={<Button type="button" variant="outline" />}
                    >
                        返回销售单
                    </DialogClose>
                    <Button
                        type="button"
                        disabled={
                            !recommendation?.ready ||
                            !allCovered ||
                            recommendationPending ||
                            isSubmitting
                        }
                        onClick={() => void onApprove()}
                    >
                        <CheckIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {isSubmitting ? "正在生成采购单…" : "生成采购单"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
