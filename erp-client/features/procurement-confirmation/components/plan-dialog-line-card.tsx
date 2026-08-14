"use client"

import { OptionCombobox } from "@/components/business"
import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { money } from "@/features/procurement-confirmation/lib/format"
import {
    findOffering,
    singleCapabilityForMode,
} from "@/features/procurement-confirmation/lib/offering"
import type {
    ConfirmationLineDraft,
    FulfillmentMode,
} from "@/features/procurement-confirmation/types"

type SelectionOption = {
    value: string
    label: string
}

type PlanLineCardProps = {
    line: ConfirmationLineDraft
    skuId: string
    offering: ProcurementSupplyOption | undefined
    usesBulkPrice: boolean
    canRemove: boolean
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
    removeLine: (lineKey: string) => void
}

export function PlanLineCard({
    line,
    skuId,
    offering,
    usesBulkPrice,
    canRemove,
    formalPending,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    updatePlanLine,
    removeLine,
}: PlanLineCardProps) {
    return (
        <div
            key={line.lineKey}
            className="grid min-w-0 gap-3 rounded-md border border-border p-3 md:grid-cols-2 lg:grid-cols-4"
        >
            <div className="min-w-0 space-y-1.5 md:col-span-2 lg:col-span-2">
                <Label>供应商报价</Label>
                <OptionCombobox
                    value={line.offeringRevisionId || undefined}
                    onValueChange={(revisionId) => {
                        const nextOffering = findOffering(
                            supplyOptions,
                            revisionId,
                        )
                        const supplier = supplierOptions?.find(
                            (option) =>
                                option.supplierId === nextOffering?.supplierId,
                        )
                        const capability = singleCapabilityForMode(
                            nextOffering,
                            line.fulfillmentMode,
                        )
                        updatePlanLine(line.lineKey, {
                            supplierId: nextOffering?.supplierId ?? "",
                            supplierName: supplier?.supplierName ?? "",
                            offeringRevisionId:
                                nextOffering?.offeringRevisionId ?? "",
                            inputTaxRate: nextOffering?.inputTaxRate ?? "",
                            capabilityRevisionId: capability?.revisionId ?? "",
                            capabilitySummary:
                                capability?.label ?? "请选择供应资质",
                            qualificationStatus: capability
                                ? "VALID"
                                : "INVALID",
                        })
                    }}
                    options={offeringOptionsForSku(skuId)}
                    disabled={formalPending}
                    placeholder="选择供应商报价"
                    className="w-full min-w-0"
                    inputClassName="h-9 min-h-9"
                />
            </div>

            <div className="min-w-0 space-y-1.5">
                <Label>采购数量</Label>
                <Input
                    className="num h-9"
                    inputMode="decimal"
                    value={line.confirmedQuantity}
                    onChange={(event) =>
                        updatePlanLine(line.lineKey, {
                            confirmedQuantity: event.target.value,
                        })
                    }
                    disabled={formalPending}
                />
            </div>

            <div className="min-w-0 space-y-1.5">
                <Label>含税单价</Label>
                <div className="flex h-9 min-w-0 items-center justify-between gap-2 rounded-md border border-border bg-muted/30 px-3 text-sm">
                    <span className="num shrink-0 font-medium">
                        {line.latestCostGross
                            ? money.format(Number(line.latestCostGross))
                            : "—"}
                    </span>
                    <span className="truncate text-xs text-muted-foreground">
                        {offering
                            ? usesBulkPrice
                                ? "集采价"
                                : "一件代发价"
                            : "选择供应商后计算"}
                    </span>
                </div>
            </div>

            <div className="min-w-0 space-y-1.5">
                <Label>交付方式</Label>
                <OptionCombobox
                    value={line.fulfillmentMode}
                    onValueChange={(value) => {
                        if (!value) return
                        const fulfillmentMode = value as FulfillmentMode
                        const capability = singleCapabilityForMode(
                            offering,
                            fulfillmentMode,
                        )
                        updatePlanLine(line.lineKey, {
                            fulfillmentMode,
                            capabilityRevisionId:
                                capability?.revisionId ?? "",
                            capabilitySummary:
                                capability?.label ?? "请选择供应资质",
                            qualificationStatus: capability
                                ? "VALID"
                                : "INVALID",
                        })
                    }}
                    options={fulfillmentOptionsForOffering(
                        line.offeringRevisionId,
                    )}
                    allowClear={false}
                    disabled={formalPending}
                    placeholder="选择交付方式"
                    className="w-full min-w-0"
                    inputClassName="h-9 min-h-9"
                />
            </div>

            <div className="min-w-0 space-y-1.5">
                <Label>供应商交期</Label>
                <DatePicker
                    value={line.expectedDeliveryDate || undefined}
                    onValueChange={(value) =>
                        updatePlanLine(line.lineKey, {
                            expectedDeliveryDate: value ?? "",
                        })
                    }
                    disabled={formalPending}
                    className="h-9 w-full min-w-0"
                />
            </div>

            <div className="min-w-0 space-y-1.5">
                <Label>供应资质</Label>
                <OptionCombobox
                    value={line.capabilityRevisionId || undefined}
                    onValueChange={(revisionId) => {
                        const capability = capabilityOptionsForOffering(
                            line.offeringRevisionId,
                            line.fulfillmentMode,
                        ).find((option) => option.value === revisionId)
                        updatePlanLine(line.lineKey, {
                            capabilityRevisionId: revisionId ?? "",
                            capabilitySummary: capability?.label ?? "",
                            qualificationStatus: revisionId
                                ? "VALID"
                                : "INVALID",
                        })
                    }}
                    options={capabilityOptionsForOffering(
                        line.offeringRevisionId,
                        line.fulfillmentMode,
                    )}
                    disabled={formalPending}
                    placeholder="选择供应资质"
                    className="w-full min-w-0"
                    inputClassName="h-9 min-h-9"
                />
            </div>

            <div className="flex min-w-0 items-end justify-end">
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={formalPending || !canRemove}
                    onClick={() => removeLine(line.lineKey)}
                >
                    删除
                </Button>
            </div>
        </div>
    )
}
