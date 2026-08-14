"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
    InputGroupText,
} from "@/components/ui/input-group"
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

export function SupplierEditorInvoiceSection({
    values,
    setFieldValue,
    canEdit,
    bankSensitive,
    canRevealSensitive,
    refreshSensitiveToken,
    editedSensitiveRef,
}: SupplierEditorSectionProps & {
    bankSensitive: SupplierSensitiveInfo | undefined
    canRevealSensitive: boolean
    refreshSensitiveToken: SupplierRefreshRevealToken
    editedSensitiveRef: React.MutableRefObject<
        Set<"contactPhone" | "address" | "bankAccount">
    >
}) {
    return (
        <SectionPanel
            title="开票信息"
            description="税号与银行信息用于采购开票与付款。"
        >
            <div className="grid gap-4 sm:grid-cols-2">
                <FieldShell>
                    <Label htmlFor="supplier-tax-no">
                        {masterDataCopy.fTaxNo}
                    </Label>
                    <Input
                        id="supplier-tax-no"
                        value={values.taxNo}
                        onChange={(event) =>
                            setFieldValue("taxNo", event.target.value)
                        }
                        disabled={!canEdit}
                        placeholder="纳税人识别号"
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="supplier-bank-name">
                        {masterDataCopy.fBankName}
                    </Label>
                    <Input
                        id="supplier-bank-name"
                        value={values.bankName}
                        onChange={(event) =>
                            setFieldValue("bankName", event.target.value)
                        }
                        disabled={!canEdit}
                        placeholder="开户银行"
                    />
                </FieldShell>
                <FieldShell>
                    <SensitiveEditableField
                        label={masterDataCopy.fBankAccount}
                        id="supplier-bank-account"
                        value={values.bankAccount}
                        maskedValue={bankSensitive?.maskedValue}
                        revealToken={bankSensitive?.revealToken}
                        onChange={(next) => {
                            editedSensitiveRef.current.add("bankAccount")
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
                    <Label>{masterDataCopy.fInvoiceType}</Label>
                    <OptionCombobox
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
                    <Label htmlFor="supplier-invoice-tax-rate">
                        {masterDataCopy.fInvoiceTaxRate}
                    </Label>
                    <InputGroup>
                        <InputGroupInput
                            id="supplier-invoice-tax-rate"
                            value={values.invoiceTaxRate}
                            inputMode="numeric"
                            onChange={(event) =>
                                setFieldValue(
                                    "invoiceTaxRate",
                                    event.target.value
                                        .replace(/\D/g, "")
                                        .slice(0, 2),
                                )
                            }
                            placeholder="如：13"
                            disabled={!canEdit}
                        />
                        <InputGroupAddon align="inline-end">
                            <InputGroupText>%</InputGroupText>
                        </InputGroupAddon>
                    </InputGroup>
                </FieldShell>
            </div>
        </SectionPanel>
    )
}
