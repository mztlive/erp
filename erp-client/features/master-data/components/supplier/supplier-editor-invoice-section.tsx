"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    FieldShell,
    SectionPanel,
    SensitiveEditableField,
} from "@/features/master-data/components/supplier/supplier-editor-fields"
import type {
    SupplierEditorSectionProps,
    SupplierRefreshRevealToken,
    SupplierSensitiveInfo,
} from "@/features/master-data/components/supplier/supplier-editor-section-props"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { INVOICE_TYPE_OPTIONS } from "@/features/master-data/lib/resource-fields"
import type { SupplierSensitiveFieldKey } from "@/features/master-data/lib/supplier-editor-model"
import { SupplierTaxRatesField } from "./supplier-tax-rates-field"

export function SupplierEditorInvoiceSection({
    values,
    setFieldValue,
    canEdit,
    bankSensitive,
    canRevealSensitive,
    refreshSensitiveToken,
    editedSensitive,
    markSensitiveEdited,
}: SupplierEditorSectionProps & {
    bankSensitive: SupplierSensitiveInfo | undefined
    canRevealSensitive: boolean
    refreshSensitiveToken: SupplierRefreshRevealToken
    editedSensitive: ReadonlySet<SupplierSensitiveFieldKey>
    markSensitiveEdited: (field: SupplierSensitiveFieldKey) => void
}) {
    return (
        <SectionPanel
            id="supplier-section-invoice"
            title="开票信息"
            description="税号与银行信息用于采购开票与付款。"
        >
            <div className="grid gap-4 sm:grid-cols-2">
                <FieldShell>
                    <Label htmlFor="master-data-supplier-invoice-tax-no">
                        {masterDataCopy.fTaxNo}
                    </Label>
                    <Input
                        id="master-data-supplier-invoice-tax-no"
                        className="num"
                        value={values.taxNo}
                        onChange={(event) =>
                            setFieldValue("taxNo", event.target.value)
                        }
                        disabled={!canEdit}
                        placeholder="纳税人识别号"
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="master-data-supplier-invoice-bank-name">
                        {masterDataCopy.fBankName}
                    </Label>
                    <Input
                        id="master-data-supplier-invoice-bank-name"
                        value={values.bankName}
                        onChange={(event) =>
                            setFieldValue("bankName", event.target.value)
                        }
                        disabled={!canEdit}
                        placeholder="开户银行"
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="master-data-supplier-bank-routing-number">
                        联行号
                    </Label>
                    <Input
                        id="master-data-supplier-bank-routing-number"
                        className="num"
                        value={values.bankRoutingNumber}
                        onChange={(event) =>
                            setFieldValue(
                                "bankRoutingNumber",
                                event.target.value,
                            )
                        }
                        disabled={!canEdit}
                        placeholder="12 位联行号（选填）"
                        inputMode="numeric"
                        maxLength={12}
                    />
                </FieldShell>
                <FieldShell>
                    <SensitiveEditableField
                        label={masterDataCopy.fBankAccount}
                        id="master-data-supplier-invoice-bank-account"
                        value={values.bankAccount}
                        maskedValue={bankSensitive?.maskedValue}
                        revealToken={bankSensitive?.revealToken}
                        edited={editedSensitive.has("bankAccount")}
                        numeric
                        onChange={(next) => {
                            markSensitiveEdited("bankAccount")
                            setFieldValue("bankAccount", next)
                        }}
                        disabled={!canEdit}
                        canReveal={canRevealSensitive}
                        getRevealToken={() =>
                            refreshSensitiveToken(["银行账号"])
                        }
                        placeholder="银行账号"
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="master-data-supplier-invoice-type-combobox">
                        {masterDataCopy.fInvoiceType}
                    </Label>
                    <OptionCombobox
                        id="master-data-supplier-invoice-type-combobox"
                        value={values.invoiceType || null}
                        onValueChange={(v) =>
                            setFieldValue("invoiceType", v ?? "")
                        }
                        options={INVOICE_TYPE_OPTIONS.map((o) => ({
                            value: o,
                            label: o,
                        }))}
                        allowClear
                        placeholder="请选择发票类型"
                        className="w-full"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <SupplierTaxRatesField
                        id="master-data-supplier-invoice-tax-rate"
                        value={values.invoiceTaxRate}
                        onChange={(next) =>
                            setFieldValue("invoiceTaxRate", next)
                        }
                        disabled={!canEdit}
                    />
                </FieldShell>
            </div>
        </SectionPanel>
    )
}
