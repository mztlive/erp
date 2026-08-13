import { currentResourceFieldValues } from "@/features/master-data/lib/resource-fields"
import type { MasterDataCenterView } from "@/features/master-data/types"

export type SupplierEditorFormValues = Readonly<{
    name: string
    company: string
    creditCode: string
    contactName: string
    contactPhone: string
    address: string
    capability: string
    settlement: string
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
    bankName: string
    bankAccount: string
    invoiceType: string
    invoiceTaxRate: string
    initialScore: string
    supplierRating: string
    currentScore: string
    changeReason: string
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
        return "联系人姓名和联系电话必须同时填写；修改联系人姓名前请先短时查看联系电话"
    }
    const hasBankName = Boolean(values.bankName.trim())
    const hasBankAccount = Boolean(values.bankAccount.trim())
    const preservesStoredBankAccount =
        hasBankName &&
        !hasBankAccount &&
        context.hasStoredBankAccount === true &&
        values.bankName.trim() === context.originalBankName?.trim()
    if (hasBankName !== hasBankAccount && !preservesStoredBankAccount) {
        return "开户银行和银行账号必须同时填写；修改开户银行前请先短时查看银行账号"
    }
    if (!values.signingEntity.trim()) return "请选择公司签约主体"
    if (!values.paymentEntity.trim()) return "请选择公司付款主体"
    for (const [label, score] of [
        ["合作期初评分", values.initialScore],
        ["合作中评分", values.currentScore],
    ] as const) {
        if (score.trim() && !/^(100|[1-9]?\d)$/.test(score.trim())) {
            return `${label}必须是 0–100 的整数`
        }
    }
    const taxRate = values.invoiceTaxRate.trim()
    if (taxRate && !/^(0|[1-9]\d?)$/.test(taxRate)) {
        return "发票税点必须是 0–99 的整数"
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
    return {
        name: data.name,
        company: fields.company ?? "",
        creditCode: fields.creditCode ?? "",
        contactName: fields.contactName ?? "",
        contactPhone: fields.contactPhone ?? "",
        address: fields.address ?? "",
        capability: fields.capability ?? "",
        settlement: fields.settlement ?? "",
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
        bankName: fields.bankName ?? "",
        bankAccount: fields.bankAccount ?? "",
        invoiceType: fields.invoiceType ?? "",
        invoiceTaxRate: fields.invoiceTaxRate ?? "",
        initialScore: fields.initialScore ?? "",
        supplierRating: fields.supplierRating ?? "",
        currentScore: fields.currentScore ?? "",
        changeReason: "",
    }
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
        bankName: "",
        bankAccount: "",
        invoiceType: "",
        invoiceTaxRate: "",
        initialScore: "",
        supplierRating: "",
        currentScore: "",
        changeReason: isCreate ? "新建供应商" : "",
    }
}
