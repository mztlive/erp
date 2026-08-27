"use client"

import { OptionCombobox, surfaceInsetClassName } from "@/components/business"
import { SettlementPartySearchCombobox } from "@/features/entity-selectors"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    CapabilityCheckboxGroup,
    FieldShell,
    SectionPanel,
} from "@/features/master-data/components/supplier/supplier-editor-fields"
import type { SupplierEditorSectionProps } from "@/features/master-data/components/supplier/supplier-editor-section-props"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    SETTLEMENT_MODE_OPTIONS,
    SUPPLIER_PAYMENT_TERM_OPTIONS,
    SUPPLIER_RATING_OPTIONS,
} from "@/features/master-data/lib/resource-fields"
import { supplierPaymentTermOptionsFor } from "@/lib/business-options"
import { cn } from "@/lib/utils"

export function SupplierEditorCommercialSection({
    values,
    setFieldValue,
    canEdit,
}: SupplierEditorSectionProps) {
    const paymentTermOptions = supplierPaymentTermOptionsFor(values.settlement)
    return (
        <SectionPanel
            title="商务合作"
            description="能力、结算与主体用于采购选用；评估分便于后续优选。"
        >
            <div className="space-y-4">
                <FieldShell>
                    <Label>{masterDataCopy.fCapability}</Label>
                    <CapabilityCheckboxGroup
                        value={values.capability}
                        onChange={(next) => setFieldValue("capability", next)}
                        disabled={!canEdit}
                    />
                </FieldShell>

                <div className="grid gap-4 sm:grid-cols-2">
                    <FieldShell>
                        <Label>{masterDataCopy.fSettlement}</Label>
                        <OptionCombobox
                            value={values.settlement || null}
                            onValueChange={(value) => {
                                const settlement = value ?? ""
                                setFieldValue("settlement", settlement)
                                const allowed =
                                    supplierPaymentTermOptionsFor(settlement)
                                if (
                                    !allowed.some(
                                        (option) =>
                                            option.value === values.paymentTerm,
                                    )
                                ) {
                                    setFieldValue(
                                        "paymentTerm",
                                        allowed.length === 1
                                            ? allowed[0].value
                                            : "",
                                    )
                                }
                            }}
                            options={SETTLEMENT_MODE_OPTIONS}
                            allowClear
                            placeholder="请选择结算方式"
                            className="w-full"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                    <FieldShell>
                        <Label>{masterDataCopy.fPaymentTerm}</Label>
                        <OptionCombobox
                            value={values.paymentTerm || null}
                            onValueChange={(value) =>
                                setFieldValue("paymentTerm", value ?? "")
                            }
                            options={
                                values.settlement
                                    ? paymentTermOptions
                                    : SUPPLIER_PAYMENT_TERM_OPTIONS
                            }
                            allowClear
                            placeholder={
                                values.settlement
                                    ? "请选择具体付款条件"
                                    : "请先选择结算方式"
                            }
                            className="w-full"
                            disabled={!canEdit || !values.settlement}
                        />
                        <p className="text-xs text-muted-foreground">
                            先款与现结按采购最终审批日；货到账期按采购明细最晚预计交付日计算。
                        </p>
                    </FieldShell>
                    <FieldShell>
                        <Label htmlFor="supplier-business-category">
                            {masterDataCopy.fBusinessCategory}
                        </Label>
                        <Input
                            id="supplier-business-category"
                            value={values.businessCategory}
                            onChange={(e) =>
                                setFieldValue(
                                    "businessCategory",
                                    e.target.value,
                                )
                            }
                            placeholder="如：礼盒、茶叶、卡券"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                    <FieldShell>
                        <Label>{masterDataCopy.fSigningEntity}</Label>
                        <SettlementPartySearchCombobox
                            value={values.signingEntity || undefined}
                            onValueChange={(value) =>
                                setFieldValue("signingEntity", value ?? "")
                            }
                            placeholder="选择与供应商签约的公司主体"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                    <FieldShell>
                        <Label>{masterDataCopy.fPaymentEntity}</Label>
                        <SettlementPartySearchCombobox
                            value={values.paymentEntity || undefined}
                            onValueChange={(value) =>
                                setFieldValue("paymentEntity", value ?? "")
                            }
                            placeholder="选择向供应商付款的公司主体"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                </div>

                <div
                    className={cn(
                        surfaceInsetClassName,
                        "grid gap-4 p-4 sm:grid-cols-3",
                    )}
                >
                    <FieldShell>
                        <Label htmlFor="supplier-initial-score">
                            {masterDataCopy.fInitialScore}
                        </Label>
                        <Input
                            id="supplier-initial-score"
                            value={values.initialScore}
                            onChange={(e) =>
                                setFieldValue("initialScore", e.target.value)
                            }
                            placeholder="如：85"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                    <FieldShell>
                        <Label>{masterDataCopy.fSupplierRating}</Label>
                        <OptionCombobox
                            value={values.supplierRating || null}
                            onValueChange={(v) =>
                                setFieldValue("supplierRating", v ?? "")
                            }
                            options={SUPPLIER_RATING_OPTIONS.map((o) => ({
                                value: o,
                                label: o,
                            }))}
                            allowClear
                            placeholder="请选择评级"
                            className="w-full"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                    <FieldShell>
                        <Label htmlFor="supplier-current-score">
                            {masterDataCopy.fCurrentScore}
                        </Label>
                        <Input
                            id="supplier-current-score"
                            value={values.currentScore}
                            onChange={(e) =>
                                setFieldValue("currentScore", e.target.value)
                            }
                            placeholder="如：88"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                </div>
            </div>
        </SectionPanel>
    )
}
