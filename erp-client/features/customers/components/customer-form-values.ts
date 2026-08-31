"use client"

import type { AppFieldExtendedReactFormApi } from "@tanstack/react-form"
import type {
    CreateCustomerInput,
    CustomerCenterView,
    SaveCustomerDetailsInput,
} from "@/features/customers/types"

export type ContactRow = {
    existingId?: string
    draftId?: string
    name: string
    title: string
    phone: string
    telephone: string
    email: string
    isDefault: boolean
}

export type AddressRow = {
    existingId?: string
    draftId?: string
    addressType: string
    contactName: string
    address: string
    isDefault: boolean
}

export type BankAccountRow = {
    existingId?: string
    draftId?: string
    accountName: string
    bankName: string
    branchName: string
    accountNumber: string
    isDefault: boolean
}

export type FormValues = {
    legalName: string
    shortName: string
    unifiedCreditCode: string
    defaultPaymentTerm: string
    status: "active" | "disabled"
    changeReason: string
    contacts: ContactRow[]
    addresses: AddressRow[]
    bankAccounts: BankAccountRow[]
}

/** 表单实例类型；拆分子表单组件时用 `form.AppField` 渲染绑定字段。 */
export type CustomerFormApi = AppFieldExtendedReactFormApi<
    FormValues,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    {
        TextField: typeof import("@/components/form").TextField
        TextareaField: typeof import("@/components/form").TextareaField
        SelectField: typeof import("@/components/form").SelectField
        PdfUploadField: typeof import("@/components/form").PdfUploadField
        DateField: typeof import("@/components/form").DateField
        DateTimeField: typeof import("@/components/form").DateTimeField
    },
    {
        SubmitButton: typeof import("@/components/form").SubmitButton
    }
>

export function newIdempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

let customerFormDraftSequence = 0

export function createCustomerFormDraftId(prefix: string): string {
    customerFormDraftSequence += 1
    return `${prefix}-${customerFormDraftSequence}`
}

/**
 * 编辑态敏感字段：后端不回传明文/reveal token，预填留空，避免把掩码写回。
 * 有 token 时也仅作占位（reveal 接口未落地）。
 */
export function editableValue(
    token: string | undefined,
    masked: string,
): string {
    if (token) return ""
    if (!masked || masked === "—" || masked.includes("*")) return ""
    return masked
}

export function buildDefaults(
    mode: "create" | "edit",
    customer: CustomerCenterView | undefined,
): FormValues {
    if (mode === "create") {
        return {
            legalName: "",
            shortName: "",
            unifiedCreditCode: "",
            defaultPaymentTerm: "POSTPAY_NET30",
            status: "active",
            changeReason: "",
            contacts: [],
            addresses: [],
            bankAccounts: [],
        }
    }
    return {
        legalName: customer!.currentRevision.legalName,
        shortName: customer!.currentRevision.shortName ?? "",
        unifiedCreditCode: customer!.currentRevision.unifiedCreditCode ?? "",
        defaultPaymentTerm: customer!.currentRevision.defaultPaymentTerm ?? "",
        status: customer!.status,
        changeReason: "",
        contacts: customer!.contacts.map((c) => ({
            existingId: c.id,
            name: c.name,
            title: c.title ?? "",
            phone: editableValue(c.phoneRevealToken, c.phoneMasked),
            telephone: c.telephone ?? "",
            email: c.email ?? "",
            isDefault: c.isDefault,
        })),
        addresses: customer!.addresses.map((a) => ({
            existingId: a.id,
            addressType: a.addressType,
            contactName: a.contactName ?? "",
            address: editableValue(a.addressRevealToken, a.addressMasked),
            isDefault: a.isDefault,
        })),
        bankAccounts: customer!.bankAccounts.map((b) => ({
            existingId: b.id,
            accountName: b.accountName,
            bankName: b.bankName,
            branchName: b.branchName ?? "",
            accountNumber: editableValue(b.accountRevealToken, b.accountMasked),
            isDefault: b.isDefault,
        })),
    }
}

/**
 * 表单值 → 根命令输入：行级 trim、可选字段收口，已存在行保留掩码值时不回写。
 */
export function buildFormSubmission(
    mode: "create" | "edit",
    value: FormValues,
    customer: CustomerCenterView | undefined,
    options: {
        canWriteContacts: boolean
        canWriteAddresses: boolean
        canWriteBanks: boolean
        idempotencyKey: string
    },
): CreateCustomerInput | SaveCustomerDetailsInput {
    const contacts = value.contacts.map((row) => ({
        existingId: row.existingId,
        name: row.name.trim(),
        title: row.title.trim() || undefined,
        phone:
            row.existingId && row.phone.includes("*")
                ? undefined
                : row.phone.trim() || undefined,
        telephone: row.telephone.trim() || undefined,
        email: row.email.trim() || undefined,
        isDefault: row.isDefault,
    }))
    const addresses = value.addresses.map((row) => ({
        existingId: row.existingId,
        addressType: row.addressType.trim(),
        contactName: row.contactName.trim() || undefined,
        address:
            row.existingId && row.address.includes("*")
                ? undefined
                : row.address.trim() || undefined,
        isDefault: row.isDefault,
    }))
    const bankAccounts = value.bankAccounts.map((row) => ({
        existingId: row.existingId,
        accountName: row.accountName.trim(),
        bankName: row.bankName.trim(),
        branchName: row.branchName.trim() || undefined,
        accountNumber:
            row.existingId && row.accountNumber.includes("*")
                ? undefined
                : row.accountNumber.trim() || undefined,
        isDefault: row.isDefault,
    }))

    if (mode === "create") {
        return {
            legalName: value.legalName.trim(),
            shortName: value.shortName.trim() || undefined,
            unifiedCreditCode: value.unifiedCreditCode.trim(),
            defaultPaymentTerm: value.defaultPaymentTerm.trim() || undefined,
            status: value.status,
            contacts: options.canWriteContacts ? contacts : undefined,
            addresses: options.canWriteAddresses ? addresses : undefined,
            bankAccounts: options.canWriteBanks ? bankAccounts : undefined,
            idempotencyKey: options.idempotencyKey,
        }
    }
    return {
        customerId: customer!.customerId,
        expectedLockVersion: customer!.lockVersion,
        expectedPartyVersion: customer!.partyLockVersion,
        baseRevisionId: customer!.currentRevision.revisionId,
        legalName: value.legalName.trim(),
        shortName: value.shortName.trim(),
        unifiedCreditCode: value.unifiedCreditCode.trim(),
        defaultPaymentTerm: value.defaultPaymentTerm.trim(),
        status: value.status,
        changeReason: value.changeReason.trim(),
        contacts: options.canWriteContacts ? contacts : undefined,
        addresses: options.canWriteAddresses ? addresses : undefined,
        bankAccounts: options.canWriteBanks ? bankAccounts : undefined,
        idempotencyKey: options.idempotencyKey,
    }
}
