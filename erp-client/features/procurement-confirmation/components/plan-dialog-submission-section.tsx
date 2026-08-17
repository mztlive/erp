"use client"

import { PlusIcon } from "lucide-react"

import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { PlanLineCard } from "@/features/procurement-confirmation/components/plan-dialog-line-card"
import { findOffering } from "@/features/procurement-confirmation/lib/offering"
import type {
    ConfirmationLineDraft,
    FulfillmentMode,
    SubmissionLineView,
} from "@/features/procurement-confirmation/types"

type SelectionOption = {
    value: string
    label: string
}

export type PlanCoverage = {
    submissionLineId: string
    itemName: string
    confirmed: string
    required: string
    complete: boolean
    gap: string
}

type PlanSubmissionSectionProps = {
    subLine: SubmissionLineView
    planLines: readonly ConfirmationLineDraft[]
    lineCoverage: PlanCoverage | undefined
    allLineDrafts: readonly ConfirmationLineDraft[]
    formalPending: boolean
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
}

export function PlanSubmissionSection({
    subLine,
    planLines,
    lineCoverage,
    allLineDrafts,
    formalPending,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    updatePlanLine,
    addSplitLine,
    removeLine,
}: PlanSubmissionSectionProps) {
    return (
        <div
            key={subLine.submissionLineId}
            className="overflow-hidden rounded-lg border border-border"
        >
            <div className="flex flex-wrap items-start justify-between gap-2 bg-muted/40 px-4 py-3">
                <div>
                    <p className="font-medium">{subLine.itemName}</p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        需要采购 {subLine.committedQuantity} {subLine.unit} ·
                        客户交期 {subLine.requestedDeliveryDate}
                    </p>
                </div>
                <Badge
                    variant={
                        lineCoverage?.complete ? "secondary" : "destructive"
                    }
                >
                    已安排 {lineCoverage?.confirmed ?? "0"}/
                    {lineCoverage?.required ?? subLine.committedQuantity}{" "}
                    {subLine.unit}
                </Badge>
            </div>

            <div className="space-y-3 p-3">
                {planLines.map((line) => {
                    const offering = findOffering(
                        supplyOptions,
                        line.offeringRevisionId,
                    )
                    const allocatedQuantity = allLineDrafts
                        .filter(
                            (item) =>
                                item.offeringRevisionId ===
                                line.offeringRevisionId,
                        )
                        .reduce(
                            (total, item) =>
                                total + Number(item.confirmedQuantity || 0),
                            0,
                        )
                    const usesBulkPrice = offering
                        ? allocatedQuantity >=
                          Number(offering.bulkMinimumOrderQuantity)
                        : false
                    return (
                        <PlanLineCard
                            key={line.lineKey}
                            line={line}
                            skuId={subLine.itemSku}
                            offering={offering}
                            usesBulkPrice={usesBulkPrice}
                            canRemove={planLines.length > 1}
                            formalPending={formalPending}
                            supplyOptions={supplyOptions}
                            supplierOptions={supplierOptions}
                            offeringOptionsForSku={offeringOptionsForSku}
                            capabilityOptionsForOffering={
                                capabilityOptionsForOffering
                            }
                            fulfillmentOptionsForOffering={
                                fulfillmentOptionsForOffering
                            }
                            updatePlanLine={updatePlanLine}
                            removeLine={removeLine}
                        />
                    )
                })}

                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={formalPending}
                    onClick={() => addSplitLine(subLine.submissionLineId)}
                >
                    <PlusIcon data-icon="inline-start" aria-hidden="true" />
                    增加供应商
                </Button>
            </div>
        </div>
    )
}
