import { z } from "zod"
import type { Company } from "@/features/companies/api"
import type { ImportCustomer } from "../api/import-identities"

export const IMPORT_REVIEW_STEPS = [
    "签约双方",
    "合同条款",
    "日期与范围",
] as const
export type ImportReviewStep = 0 | 1 | 2
const STEP_FIELDS = [
    [
        "customerId",
        "companyId",
        "customer_name",
        "customer_credit_code",
        "company_name",
        "company_credit_code",
        "customerMode",
    ],
    ["contract_no", "payment_terms", "invoice_type", "tax_point"],
    ["signed_at", "valid_from", "valid_to", "business_scope"],
] as const

export const importNewCustomerCreditCodeSchema = z
    .string()
    .trim()
    .min(1, "请填写统一社会信用代码")
    .regex(/^[A-Za-z0-9]{18}$/, "统一社会信用代码须为 18 位字母数字")

export function importReviewFieldStep(field: string): ImportReviewStep {
    const step = STEP_FIELDS.findIndex((fields) =>
        (fields as readonly string[]).includes(field),
    )
    return step < 0 ? 0 : (step as ImportReviewStep)
}

export function importReviewSchema({
    canCreate,
    expectedCustomerId,
    customer,
    company,
}: {
    canCreate: boolean
    expectedCustomerId?: string
    customer?: ImportCustomer
    company?: Company
}) {
    const required = (label: string) =>
        z.string().trim().min(1, `请填写${label}`)
    return z
        .object({
            contract_no: required("合同编号"),
            customer_name: required("对方签约名称"),
            customer_credit_code: z.string(),
            company_name: required("我方签约名称"),
            company_credit_code: z.string(),
            payment_terms: required("付款条件"),
            invoice_type: required("开票要求"),
            tax_point: required("税率"),
            signed_at: required("签订日期"),
            valid_from: required("生效日期"),
            valid_to: required("有效期止"),
            business_scope: z.string().trim(),
            customerId: z.string(),
            companyId: z.string().min(1, "请选择系统中的我方签约主体"),
            customerMode: z.enum(["existing", "create"]),
        })
        .superRefine((value, ctx) => {
            if (value.customerMode === "create") {
                const creditCode = importNewCustomerCreditCodeSchema.safeParse(
                    value.customer_credit_code,
                )
                if (!creditCode.success)
                    ctx.addIssue({
                        code: "custom",
                        path: ["customer_credit_code"],
                        message: creditCode.error.issues[0].message,
                    })
            }
            if (
                value.customerMode === "create" &&
                (!canCreate || expectedCustomerId)
            )
                ctx.addIssue({
                    code: "custom",
                    path: ["customerMode"],
                    message: "当前操作不能建立客户档案，请选择已有客户",
                })
            if (value.customerMode === "existing" && !value.customerId)
                ctx.addIssue({
                    code: "custom",
                    path: ["customerId"],
                    message: "请选择合同签约方对应的客户档案",
                })
            if (
                value.customerId &&
                customer &&
                (customer.statusTone !== "success" ||
                    value.customer_name.trim() !== customer.legalName.trim() ||
                    value.customer_credit_code.trim().toUpperCase() !==
                        customer.creditCode.trim().toUpperCase())
            )
                ctx.addIssue({
                    code: "custom",
                    path: ["customerId"],
                    message:
                        "签约信息与所选客户不一致，请在客户区域核对并确认档案信息。",
                })
            if (
                value.companyId &&
                company &&
                (company.status !== "active" ||
                    value.company_name.trim() !== company.legal_name.trim() ||
                    value.company_credit_code.trim().toUpperCase() !==
                        (company.unified_credit_code ?? "")
                            .trim()
                            .toUpperCase())
            )
                ctx.addIssue({
                    code: "custom",
                    path: ["companyId"],
                    message:
                        "签约信息与所选我方主体不一致，请在我方主体区域核对并确认档案信息。",
                })
        })
}
