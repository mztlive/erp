"use client"

import * as React from "react"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
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
import type { SupplierSensitiveFieldKey } from "@/features/master-data/lib/supplier-editor-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SupplierEditorBasicSection({
    values,
    setFieldValue,
    canEdit,
    phoneSensitive,
    addressSensitive,
    canRevealSensitive,
    refreshSensitiveToken,
    editedSensitive,
    markSensitiveEdited,
}: SupplierEditorSectionProps & {
    phoneSensitive: SupplierSensitiveInfo | undefined
    addressSensitive: SupplierSensitiveInfo | undefined
    canRevealSensitive: boolean
    refreshSensitiveToken: SupplierRefreshRevealToken
    editedSensitive: ReadonlySet<SupplierSensitiveFieldKey>
    markSensitiveEdited: (field: SupplierSensitiveFieldKey) => void
}) {
    return (
        <SectionPanel id="supplier-section-basic" title="基本信息">
            <div className="grid gap-4 sm:grid-cols-2">
                <FieldShell>
                    <Label htmlFor="master-data-supplier-basic-name">
                        名称
                        <RequiredMark />
                    </Label>
                    <Input
                        id="master-data-supplier-basic-name"
                        aria-required="true"
                        value={values.name}
                        onChange={(e) => setFieldValue("name", e.target.value)}
                        placeholder="供应商名称"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="master-data-supplier-basic-company">
                        {masterDataCopy.fCompany}
                        <RequiredMark />
                    </Label>
                    <Input
                        id="master-data-supplier-basic-company"
                        aria-required="true"
                        value={values.company}
                        onChange={(e) =>
                            setFieldValue("company", e.target.value)
                        }
                        placeholder="企业主体全称"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="master-data-supplier-basic-contact-name">
                        {masterDataCopy.fContactName}
                    </Label>
                    <Input
                        id="master-data-supplier-basic-contact-name"
                        value={values.contactName}
                        onChange={(e) =>
                            setFieldValue("contactName", e.target.value)
                        }
                        placeholder="联系人姓名"
                        disabled={!canEdit}
                    />
                </FieldShell>
                <FieldShell>
                    <Label htmlFor="master-data-supplier-basic-credit-code">
                        {masterDataCopy.fCreditCode}
                    </Label>
                    <Input
                        id="master-data-supplier-basic-credit-code"
                        className="num"
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
                        id="master-data-supplier-basic-contact-phone"
                        value={values.contactPhone}
                        maskedValue={phoneSensitive?.maskedValue}
                        revealToken={phoneSensitive?.revealToken}
                        edited={editedSensitive.has("contactPhone")}
                        numeric
                        onChange={(next) => {
                            markSensitiveEdited("contactPhone")
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
                        id="master-data-supplier-basic-address"
                        value={values.address}
                        maskedValue={addressSensitive?.maskedValue}
                        revealToken={addressSensitive?.revealToken}
                        edited={editedSensitive.has("address")}
                        onChange={(next) => {
                            markSensitiveEdited("address")
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
