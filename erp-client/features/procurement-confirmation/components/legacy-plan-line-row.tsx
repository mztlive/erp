"use client"

import { OptionCombobox } from "@/components/business"
import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import { supplyCostForQuantity } from "@/features/procurement-confirmation/lib/supply-cost"
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

type LegacyPlanLineRowProps = {
    line: ConfirmationLineDraft
    skuId: string
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
    onUpdateLine: (
        lineKey: string,
        patch: Partial<ConfirmationLineDraft>,
    ) => void
    onRemoveLine: (lineKey: string) => void
}

export function LegacyPlanLineRow({
    line,
    skuId,
    canRemove,
    formalPending,
    supplyOptions,
    supplierOptions,
    offeringOptionsForSku,
    capabilityOptionsForOffering,
    fulfillmentOptionsForOffering,
    onUpdateLine,
    onRemoveLine,
}: LegacyPlanLineRowProps) {
    return (
        <tr key={line.lineKey} className="border-b border-border last:border-0">
            <td className="px-3 py-2">
                <OptionCombobox
                    value={line.offeringRevisionId || undefined}
                    onValueChange={(revisionId) => {
                        const offering = findOffering(
                            supplyOptions,
                            revisionId,
                        )
                        const supplier = supplierOptions?.find(
                            (option) =>
                                option.supplierId === offering?.supplierId,
                        )
                        const onlyCapability = singleCapabilityForMode(
                            offering,
                            line.fulfillmentMode,
                        )
                        onUpdateLine(line.lineKey, {
                            supplierId: offering?.supplierId ?? "",
                            supplierName: supplier?.supplierName ?? "",
                            offeringRevisionId:
                                offering?.offeringRevisionId ?? "",
                            latestCostGross: offering
                                ? supplyCostForQuantity(
                                      offering,
                                      line.confirmedQuantity,
                                  )
                                : "",
                            inputTaxRate: offering?.inputTaxRate ?? "",
                            capabilityRevisionId:
                                onlyCapability?.revisionId ?? "",
                            capabilitySummary:
                                onlyCapability?.label ??
                                "请选择有效供应资质",
                            qualificationStatus: onlyCapability
                                ? "VALID"
                                : "INVALID",
                        })
                    }}
                    options={offeringOptionsForSku(skuId)}
                    disabled={formalPending}
                    placeholder="选择供应商报价"
                    className="min-w-[12rem]"
                />
            </td>
            <td className="px-3 py-2">
                <Input
                    className="num w-20"
                    inputMode="decimal"
                    value={line.confirmedQuantity}
                    onChange={(e) => {
                        const confirmedQuantity = e.target.value
                        const offering = findOffering(
                            supplyOptions,
                            line.offeringRevisionId,
                        )
                        onUpdateLine(line.lineKey, {
                            confirmedQuantity,
                            latestCostGross: offering
                                ? supplyCostForQuantity(
                                      offering,
                                      confirmedQuantity,
                                  )
                                : line.latestCostGross,
                        })
                    }}
                    disabled={formalPending}
                    aria-label={`${line.supplierName} 确认数量`}
                />
            </td>
            <td className="px-3 py-2">
                <Input
                    className="num w-24"
                    inputMode="decimal"
                    value={line.latestCostGross}
                    onChange={(e) =>
                        onUpdateLine(line.lineKey, {
                            latestCostGross: e.target.value,
                        })
                    }
                    disabled={formalPending}
                    aria-label="最新含税成本"
                />
            </td>
            <td className="hidden px-3 py-2 md:table-cell">
                <Input
                    className="num w-16"
                    inputMode="decimal"
                    value={line.inputTaxRate}
                    onChange={(e) =>
                        onUpdateLine(line.lineKey, {
                            inputTaxRate: e.target.value,
                        })
                    }
                    disabled={formalPending}
                    aria-label="进项税率"
                />
            </td>
            <td className="hidden px-3 py-2 sm:table-cell">
                <DatePicker
                    className="w-[9.5rem]"
                    value={line.expectedDeliveryDate || undefined}
                    onValueChange={(next) =>
                        onUpdateLine(line.lineKey, {
                            expectedDeliveryDate: next ?? "",
                        })
                    }
                    disabled={formalPending}
                />
            </td>
            <td className="px-3 py-2">
                <OptionCombobox
                    value={line.fulfillmentMode}
                    onValueChange={(value) => {
                        if (!value) return
                        const fulfillmentMode = value as FulfillmentMode
                        const offering = findOffering(
                            supplyOptions,
                            line.offeringRevisionId,
                        )
                        const onlyCapability = singleCapabilityForMode(
                            offering,
                            fulfillmentMode,
                        )
                        onUpdateLine(line.lineKey, {
                            fulfillmentMode,
                            capabilityRevisionId:
                                onlyCapability?.revisionId ?? "",
                            capabilitySummary:
                                onlyCapability?.label ??
                                "请选择有效供应资质",
                            qualificationStatus: onlyCapability
                                ? "VALID"
                                : "INVALID",
                        })
                    }}
                    options={fulfillmentOptionsForOffering(
                        line.offeringRevisionId,
                    )}
                    size="sm"
                    allowClear={false}
                    disabled={formalPending}
                    aria-label="交付方式"
                    placeholder="交付方式"
                    className="min-w-[8rem]"
                />
            </td>
            <td className="hidden px-3 py-2 lg:table-cell">
                <OptionCombobox
                    value={line.capabilityRevisionId || undefined}
                    onValueChange={(revisionId) => {
                        const capability = capabilityOptionsForOffering(
                            line.offeringRevisionId,
                            line.fulfillmentMode,
                        ).find((option) => option.value === revisionId)
                        onUpdateLine(line.lineKey, {
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
                    size="sm"
                    disabled={formalPending}
                    placeholder="选择供应资质"
                    className="min-w-[8rem]"
                />
            </td>
            <td className="px-3 py-2 text-right">
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={formalPending || !canRemove}
                    onClick={() => onRemoveLine(line.lineKey)}
                >
                    删除
                </Button>
            </td>
        </tr>
    )
}
