"use client"

import { OptionCombobox, surfaceInsetClassName } from "@/components/business"
import { CompanySearchCombobox } from "@/features/companies/company-search-combobox"
import {
    parsePeriodicTerm,
    periodicPaymentTerm,
    periodicSettlement,
} from "@/lib/supplier-payment-terms"
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
    const cycle = periodicSettlement(values.settlement)
    const term = parsePeriodicTerm(values.paymentTerm)
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
                            id="master-data-supplier-commercial-settlement-combobox"
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
                            options={
                                values.settlement === "pay_after_use"
                                    ? [
                                          ...SETTLEMENT_MODE_OPTIONS,
                                          {
                                              value: "pay_after_use",
                                              label: "货到后付（历史）",
                                          },
                                      ]
                                    : SETTLEMENT_MODE_OPTIONS
                            }
                            allowClear
                            placeholder="请选择结算方式"
                            className="w-full"
                            disabled={!canEdit}
                        />
                    </FieldShell>
                    <FieldShell>
                        <Label htmlFor="master-data-supplier-commercial-payment-days">
                            {cycle
                                ? "结算后付款天数"
                                : masterDataCopy.fPaymentTerm}
                        </Label>
                        {cycle ? (
                            <Input
                                id="master-data-supplier-commercial-payment-days"
                                type="number"
                                min={0}
                                max={366}
                                value={
                                    term
                                        ? String(term.days)
                                        : (values.paymentTerm
                                              .split("_")
                                              .at(-1) ?? "")
                                }
                                onChange={(e) =>
                                    setFieldValue(
                                        "paymentTerm",
                                        periodicPaymentTerm(
                                            values.settlement,
                                            e.target.value,
                                        ),
                                    )
                                }
                                disabled={!canEdit}
                            />
                        ) : (
                            <OptionCombobox
                                id="master-data-supplier-commercial-payment-term-combobox"
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
                                placeholder="请选择具体付款条件"
                                className="w-full"
                                disabled={!canEdit || !values.settlement}
                            />
                        )}
                        <p className="text-xs text-muted-foreground">
                            {cycle
                                ? `按最晚预计交付日所属自然周期结算，期末后 ${term?.days ?? "—"} 个自然日计划付款。`
                                : "先款与现结按采购最终审批日；历史货到账期按最晚预计交付日计算。"}
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
                        <CompanySearchCombobox
                            id="master-data-supplier-commercial-signing-entity-combobox"
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
                        <CompanySearchCombobox
                            id="master-data-supplier-commercial-payment-entity-combobox"
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
                            id="master-data-supplier-commercial-rating-combobox"
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
