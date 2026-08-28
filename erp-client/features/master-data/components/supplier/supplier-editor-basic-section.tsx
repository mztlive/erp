"use client"

import * as React from "react"

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

export function SupplierEditorBasicSection({
    values,
    setFieldValue,
    canEdit,
    phoneSensitive,
    addressSensitive,
    canRevealSensitive,
    refreshSensitiveToken,
    editedSensitiveRef,
}: SupplierEditorSectionProps & {
    phoneSensitive: SupplierSensitiveInfo | undefined
    addressSensitive: SupplierSensitiveInfo | undefined
    canRevealSensitive: boolean
    refreshSensitiveToken: SupplierRefreshRevealToken
    editedSensitiveRef: React.MutableRefObject<
        Set<"contactPhone" | "address" | "bankAccount">
    >
}) {
    return (
        <SectionPanel
            title="基本信息"
            description="名称与企业主体必填；联系方式便于采购对接。"
        >
            <div className="grid gap-4 sm:grid-cols-2">
                <FieldShell>
                    <Label htmlFor="supplier-name">
                        名称<span className="text-destructive">*</span>
                    </Label>
                    <Input
                        id="supplier-name"
                        value={values.name}
                        onChange={(e) =>
                            setFieldValue("name", e.target.value)
                        }
                        placeholder="供应商名称"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="supplier-company">
                        {masterDataCopy.fCompany}
                        <span className="text-destructive">*</span>
                    </Label>
                    <Input
                        id="supplier-company"
                        value={values.company}
                        onChange={(e) =>
                            setFieldValue("company", e.target.value)
                        }
                        placeholder="企业主体全称"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="supplier-contact-name">
                        {masterDataCopy.fContactName}
                    </Label>
                    <Input
                        id="supplier-contact-name"
                        value={values.contactName}
                        onChange={(e) =>
                            setFieldValue("contactName", e.target.value)
                        }
                        placeholder="联系人姓名"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="supplier-credit-code">
                        {masterDataCopy.fCreditCode}
                    </Label>
                    <Input
                        id="supplier-credit-code"
                        value={values.creditCode}
                        onChange={(event) =>
                            setFieldValue("creditCode", event.target.value)
                        }
                        placeholder="18 位统一社会信用代码"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <SensitiveEditableField
                        label={masterDataCopy.fContactPhone}
                        id="supplier-contact-phone"
                        value={values.contactPhone}
                        maskedValue={phoneSensitive?.maskedValue}
                        revealToken={phoneSensitive?.revealToken}
                        onChange={(next) => {
                            editedSensitiveRef.current.add("contactPhone")
                            setFieldValue("contactPhone", next)
                        }}
                        disabled={!canEdit}
                        canReveal={canRevealSensitive}
                        getRevealToken={() =>
                            refreshSensitiveToken(["联系电话", "联系人"])
                        }
                        placeholder="手机号或固定电话"
                    />
                </FieldShell>
                <FieldShell className="sm:col-span-2">
                    <SensitiveEditableField
                        label={masterDataCopy.fAddress}
                        id="supplier-address"
                        value={values.address}
                        maskedValue={addressSensitive?.maskedValue}
                        revealToken={addressSensitive?.revealToken}
                        onChange={(next) => {
                            editedSensitiveRef.current.add("address")
                            setFieldValue("address", next)
                        }}
                        placeholder="注册或经营地址"
                        disabled={!canEdit}
                        canReveal={canRevealSensitive}
                        getRevealToken={() =>
                            refreshSensitiveToken(["经营地址"])
                        }
                    />
                </FieldShell>
            </div>
        </SectionPanel>
    )
}
