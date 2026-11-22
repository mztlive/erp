import { masterDataCopy } from "@/features/master-data/lib/copy"
import { parseSupplierTaxRates } from "@/lib/supplier-tax-rates"
import { currentResourceFieldValues } from "@/features/master-data/lib/resource-fields"
import { settlementCode } from "@/features/master-data/api/presentation"
import { reconciliationCycle } from "@/lib/supplier-payment-terms"
import type { MasterDataCenterView } from "@/features/master-data/types"
import {
    isSupplierPaymentTermCode,
    paymentTermCode,
    paymentTermMatchesSettlement,
} from "@/lib/business-options"

export type SupplierEditorFormValues = Readonly<{
    name: string
    company: string
    creditCode: string
    contactName: string
    contactPhone: string
    address: string
    capability: string
    settlement: string
    reconciliationCycle: string
    paymentTerm: string
    businessCategory: string
    signingEntity: string
    paymentEntity: string
    qualification: string
    contractNo: string
    contractValidFrom: string
    contractValidTo: string
    contractFile: string
    authorizationFile: string
    authorizationValidFrom: string
    authorizationValidTo: string
    foodLicense: string
    legalPersonIdCard: string
    taxNo: string
    bankRoutingNumber: string
    bankName: string
    bankAccount: string
    invoiceType: string
    invoiceTaxRate: string
    initialScore: string
    supplierRating: string
    currentScore: string
    changeReason: string
    maintainerUserId: string
    capabilityOwnerUserId: string
}>

export type SupplierFieldKey = keyof SupplierEditorFormValues

/** 保存前字段校验（不含变更原因；原因在右上角保存弹窗中填写）。 */
export type SupplierValidationContext = Readonly<{
    hasStoredContactPhone?: boolean
    originalContactName?: string
    hasStoredBankAccount?: boolean
    originalBankName?: string
}>

export function validateSupplierEditorFields(
    values: SupplierEditorFormValues,
    context: SupplierValidationContext = {},
): string | null {
    if (!values.maintainerUserId.trim()) return "请指定供应商维护人"
    if (values.capability.trim() && !values.capabilityOwnerUserId.trim()) {
        return "请指定供给能力负责人"
    }
    if (values.name.trim().length < 2) return "请填写供应商名称"
    if (values.company.trim().length < 1) return "请填写企业主体"
    if (
        values.creditCode.trim() &&
        !/^[0-9A-Z]{18}$/i.test(values.creditCode.trim())
    ) {
        return "统一社会信用代码必须是 18 位字母或数字"
    }
    const hasContactName = Boolean(values.contactName.trim())
    const hasContactPhone = Boolean(values.contactPhone.trim())
    const preservesStoredContact =
        hasContactName &&
        !hasContactPhone &&
        context.hasStoredContactPhone === true &&
        values.contactName.trim() === context.originalContactName?.trim()
    if (hasContactName !== hasContactPhone && !preservesStoredContact) {
        return "联系人姓名和联系电话必须同时填写；修改联系人姓名时需一并核对联系电话，请先点击“查看完整信息”后再修改"
    }
    if (
        values.bankRoutingNumber.trim() &&
        !/^\d{12}$/.test(values.bankRoutingNumber.trim())
    )
        return "联行号必须为 12 位数字"
    const hasBankName = Boolean(values.bankName.trim())
    const hasBankAccount = Boolean(values.bankAccount.trim())
    if (values.bankRoutingNumber.trim() && !hasBankName)
        return "请先维护开户银行与银行账号"
    const preservesStoredBankAccount =
        hasBankName &&
        !hasBankAccount &&
        context.hasStoredBankAccount === true &&
        values.bankName.trim() === context.originalBankName?.trim()
    if (hasBankName !== hasBankAccount && !preservesStoredBankAccount) {
        return "开户银行和银行账号必须同时填写；修改开户银行时需一并核对银行账号，请先点击“查看完整信息”后再修改"
    }
    if (!values.signingEntity.trim()) return "请选择公司签约主体"
    if (!values.paymentEntity.trim()) return "请选择公司付款主体"
    if (!settlementCode(values.settlement)) return "请选择结算方式"
    if (!reconciliationCycle(values.reconciliationCycle))
        return "请选择对账频率"
    if (!isSupplierPaymentTermCode(values.paymentTerm)) {
        return "请选择具体付款条件"
    }
    if (!paymentTermMatchesSettlement(values.paymentTerm, values.settlement)) {
        return "结算方式与付款条件不一致，请重新选择"
    }
    for (const [label, score] of [
        ["合作期初评分", values.initialScore],
        ["合作中评分", values.currentScore],
    ] as const) {
        if (score.trim() && !/^(100|[1-9]?\d)$/.test(score.trim())) {
            return `${label}必须是 0–100 的整数`
        }
    }
    try {
        parseSupplierTaxRates(values.invoiceTaxRate)
    } catch {
        return "请输入有效的常用进项税率，如 9%、13%"
    }
    if (
        values.contractValidFrom &&
        values.contractValidTo &&
        values.contractValidTo <= values.contractValidFrom
    ) {
        return "合同有效期止必须晚于有效期起"
    }
    if (
        values.authorizationValidFrom &&
        values.authorizationValidTo &&
        values.authorizationValidTo <= values.authorizationValidFrom
    ) {
        return "授权书有效期止必须晚于有效期起"
    }
    return null
}

export function hydrateSupplierEditor(
    data: MasterDataCenterView,
): SupplierEditorFormValues {
    const fields = currentResourceFieldValues(data)
    const resolvedPaymentTerm = paymentTermCode(fields.paymentTerm ?? "")
    return {
        name: data.name,
        company: fields.company ?? "",
        creditCode: fields.creditCode ?? "",
        contactName: fields.contactName ?? "",
        contactPhone: fields.contactPhone ?? "",
        address: fields.address ?? "",
        capability: fields.capability ?? "",
        settlement: settlementCode(fields.settlement) ?? "",
        reconciliationCycle:
            reconciliationCycle(fields.reconciliationCycle ?? "") ?? "",
        paymentTerm:
            resolvedPaymentTerm &&
            isSupplierPaymentTermCode(resolvedPaymentTerm)
                ? resolvedPaymentTerm
                : "",
        businessCategory: fields.businessCategory ?? "",
        signingEntity: fields.signingEntity ?? "",
        paymentEntity: fields.paymentEntity ?? "",
        qualification: fields.qualification ?? "",
        contractNo: fields.contractNo ?? "",
        contractValidFrom: fields.contractValidFrom ?? "",
        contractValidTo: fields.contractValidTo ?? "",
        contractFile: fields.contractFile ?? "",
        authorizationFile: fields.authorizationFile ?? "",
        authorizationValidFrom: fields.authorizationValidFrom ?? "",
        authorizationValidTo: fields.authorizationValidTo ?? "",
        foodLicense: fields.foodLicense ?? "",
        legalPersonIdCard: fields.legalPersonIdCard ?? "",
        taxNo: fields.taxNo ?? "",
        bankRoutingNumber: fields.bankRoutingNumber ?? "",
        bankName: fields.bankName ?? "",
        bankAccount: fields.bankAccount ?? "",
        invoiceType: fields.invoiceType ?? "",
        invoiceTaxRate: fields.invoiceTaxRate ?? "",
        initialScore: fields.initialScore ?? "",
        supplierRating: fields.supplierRating ?? "",
        currentScore: fields.currentScore ?? "",
        changeReason: "",
        maintainerUserId:
            labeledValue(data.currentRevision.fields, "维护人") ||
            labeledValue(data.resourceFacts, "维护人") ||
            fields.maintainerUserId ||
            "",
        capabilityOwnerUserId:
            labeledValue(data.currentRevision.fields, "能力负责人") ||
            labeledValue(data.resourceFacts, "能力负责人") ||
            fields.capabilityOwnerUserId ||
            "",
    }
}

function labeledValue(
    facts: ReadonlyArray<{ label: string; value: string }> | undefined,
    label: string,
): string {
    const value =
        facts?.find((item) => item.label === label)?.value?.trim() ?? ""
    if (!value || value === "—" || value === "****") return ""
    return value
}

export type SupplierSensitiveFieldKey =
    | "contactPhone"
    | "address"
    | "bankAccount"

export type SupplierProfileSectionKey =
    | "basic"
    | "commercial"
    | "contract"
    | "invoice"

export function supplierSectionForValidationError(
    message: string,
): SupplierProfileSectionKey {
    if (
        message.includes("签约主体") ||
        message.includes("付款主体") ||
        message.includes("结算") ||
        message.includes("对账") ||
        message.includes("付款条件") ||
        message.includes("评分") ||
        message.includes("供给能力")
    ) {
        return "commercial"
    }
    if (message.includes("合同") || message.includes("授权书")) {
        return "contract"
    }
    if (
        message.includes("联行号") ||
        message.includes("开户银行") ||
        message.includes("银行账号") ||
        message.includes("税率")
    ) {
        return "invoice"
    }
    return "basic"
}

const SUPPLIER_CHANGE_FIELD_LABELS: ReadonlyArray<
    readonly [SupplierFieldKey, string]
> = [
    ["name", "名称"],
    ["company", masterDataCopy.fCompany],
    ["creditCode", masterDataCopy.fCreditCode],
    ["contactName", masterDataCopy.fContactName],
    ["contactPhone", masterDataCopy.fContactPhone],
    ["address", masterDataCopy.fAddress],
    ["capability", masterDataCopy.fCapability],
    ["settlement", masterDataCopy.fSettlement],
    ["reconciliationCycle", masterDataCopy.fReconciliationCycle],
    ["paymentTerm", masterDataCopy.fPaymentTerm],
    ["businessCategory", masterDataCopy.fBusinessCategory],
    ["signingEntity", masterDataCopy.fSigningEntity],
    ["paymentEntity", masterDataCopy.fPaymentEntity],
    ["initialScore", masterDataCopy.fInitialScore],
    ["supplierRating", masterDataCopy.fSupplierRating],
    ["currentScore", masterDataCopy.fCurrentScore],
    ["contractNo", masterDataCopy.fContractNo],
    ["contractValidFrom", masterDataCopy.fContractValidFrom],
    ["contractValidTo", masterDataCopy.fContractValidTo],
    ["contractFile", masterDataCopy.fContractFile],
    ["authorizationValidFrom", masterDataCopy.fAuthorizationValidFrom],
    ["authorizationValidTo", masterDataCopy.fAuthorizationValidTo],
    ["authorizationFile", masterDataCopy.fAuthorizationFile],
    ["qualification", masterDataCopy.fQualification],
    ["foodLicense", masterDataCopy.fFoodLicense],
    ["legalPersonIdCard", masterDataCopy.fLegalPersonIdCard],
    ["taxNo", masterDataCopy.fTaxNo],
    ["bankName", masterDataCopy.fBankName],
    ["bankRoutingNumber", "联行号"],
    ["bankAccount", masterDataCopy.fBankAccount],
    ["invoiceType", masterDataCopy.fInvoiceType],
    ["invoiceTaxRate", masterDataCopy.fInvoiceTaxRate],
]

/** 仅比较可维护的业务字段（含敏感字段清空/修改），供保存浮条与保存确认弹窗复用。 */
export function supplierChangeSummary(
    previous: SupplierEditorFormValues,
    next: SupplierEditorFormValues,
    context?: {
        editedSensitive?: ReadonlySet<SupplierSensitiveFieldKey>
        hasStoredSensitive?: Partial<Record<SupplierSensitiveFieldKey, boolean>>
    },
): string[] {
    const changes: string[] = []
    for (const [key, label] of SUPPLIER_CHANGE_FIELD_LABELS) {
        if (next[key] !== previous[key]) {
            changes.push(label)
            continue
        }
        if (
            (key === "contactPhone" ||
                key === "address" ||
                key === "bankAccount") &&
            Boolean(context?.hasStoredSensitive?.[key]) &&
            context?.editedSensitive?.has(key) &&
            !next[key].trim()
        ) {
            changes.push(label)
        }
    }
    return changes
}

export function createSupplierEditorDefaults(
    isCreate: boolean,
): SupplierEditorFormValues {
    return {
        name: "",
        company: "",
        creditCode: "",
        contactName: "",
        contactPhone: "",
        address: "",
        capability: "",
        settlement: "",
        reconciliationCycle: "none",
        paymentTerm: "",
        businessCategory: "",
        signingEntity: "",
        paymentEntity: "",
        qualification: "",
        contractNo: "",
        contractValidFrom: "",
        contractValidTo: "",
        contractFile: "",
        authorizationFile: "",
        authorizationValidFrom: "",
        authorizationValidTo: "",
        foodLicense: "",
        legalPersonIdCard: "",
        taxNo: "",
        bankRoutingNumber: "",
        bankName: "",
        bankAccount: "",
        invoiceType: "",
        invoiceTaxRate: "",
        initialScore: "",
        supplierRating: "",
        currentScore: "",
        changeReason: isCreate ? "新建供应商" : "",
        maintainerUserId: "",
        capabilityOwnerUserId: "",
    }
}
