"use client"

import { PlusIcon } from "lucide-react"

import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { LegacyPlanLineRow } from "@/features/procurement-confirmation/components/legacy-plan-line-row"
import type {
    ConfirmationLineDraft,
    CoverageByLine,
    FulfillmentMode,
    SubmissionLineView,
} from "@/features/procurement-confirmation/types"
import { money } from "@/features/procurement-confirmation/lib/format"

type SelectionOption = {
    value: string
    label: string
}

type LegacyPlanSubmissionSectionProps = {
    subLine: SubmissionLineView
    lines: readonly ConfirmationLineDraft[]
    cov: CoverageByLine | undefined
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
    onUpdateLine: (
        lineKey: string,
        patch: Partial<ConfirmationLineDraft>,
    ) => void
    onAddSplitLine: (submissionLineId: string) => void
    onRemoveLine: (lineKey: string) => void
}

export function LegacyPlanSubmissionSection({
    subLine,
    lines,
    cov,
    formalPending,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    onUpdateLine,
    onAddSplitLine,
    onRemoveLine,
}: LegacyPlanSubmissionSectionProps) {
    return (
        <div
            key={subLine.submissionLineId}
            id={`submission-line-${subLine.submissionLineId}`}
            className="rounded-xl border border-border"
            tabIndex={-1}
        >
            <div className="flex flex-wrap items-start justify-between gap-2 border-b border-border bg-muted/40 px-3 py-2">
                <div>
                    <p className="text-sm font-medium">{subLine.itemName}</p>
                    <p className="text-xs text-muted-foreground">
                        承诺{" "}
                        <span className="num">
                            {subLine.committedQuantity} {subLine.unit}
                        </span>{" "}
                        · 客户期望 {subLine.requestedDeliveryDate}
                        {subLine.referenceSupplier
                            ? ` · 参考 ${subLine.referenceSupplier} / ${money.format(Number(subLine.referenceCost))}`
                            : null}
                    </p>
                </div>
                <div className="text-right text-xs" aria-live="polite">
                    <Badge
                        variant={cov?.complete ? "secondary" : "destructive"}
                    >
                        覆盖 {cov?.confirmed}/{cov?.required} {subLine.unit}
                        {cov && !cov.complete
                            ? ` · 缺口 ${cov.gap} ${subLine.unit}`
                            : " · 完整"}
                    </Badge>
                </div>
            </div>

            <div className="overflow-x-auto">
                <table className="w-full min-w-[40rem] text-sm">
                    <caption className="sr-only">
                        {subLine.itemName} 采购明细
                    </caption>
                    <thead>
                        <tr className="border-b border-border text-left text-xs text-muted-foreground">
                            <th className="px-3 py-2 font-medium">供应商</th>
                            <th className="px-3 py-2 font-medium num">
                                确认数量
                            </th>
                            <th className="px-3 py-2 font-medium num">
                                含税成本
                            </th>
                            <th className="hidden px-3 py-2 font-medium num md:table-cell">
                                进项税率
                            </th>
                            <th className="hidden px-3 py-2 font-medium sm:table-cell">
                                预计交期
                            </th>
                            <th className="px-3 py-2 font-medium">交付方式</th>
                            <th className="hidden px-3 py-2 font-medium lg:table-cell">
                                供应资质
                            </th>
                            <th className="px-3 py-2 font-medium text-right">
                                操作
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {lines.map((line) => (
                            <LegacyPlanLineRow
                                key={line.lineKey}
                                line={line}
                                skuId={subLine.itemSku}
                                canRemove={lines.length > 1}
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
                                onUpdateLine={onUpdateLine}
                                onRemoveLine={onRemoveLine}
                            />
                        ))}
                    </tbody>
                </table>
            </div>

            <div className="border-t border-border px-3 py-2">
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={formalPending}
                    onClick={() => onAddSplitLine(subLine.submissionLineId)}
                >
                    <PlusIcon data-icon="inline-start" aria-hidden="true" />
                    拆分供应商
                </Button>
            </div>
        </div>
    )
}
