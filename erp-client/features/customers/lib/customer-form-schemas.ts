import { z } from "zod"

const contactRowSchema = z
    .object({
        existingId: z.string().optional(),
        name: z.string().trim().min(1, "请填写联系人姓名"),
        title: z.string(),
        phone: z.string(),
        telephone: z.string(),
        email: z.string(),
        isDefault: z.boolean(),
    })
    .superRefine((value, context) => {
        if (!value.existingId && !value.phone.trim()) {
            context.addIssue({
                code: "custom",
                path: ["phone"],
                message: "请填写手机号",
            })
        }
    })

const addressRowSchema = z
    .object({
        existingId: z.string().optional(),
        addressType: z.string().trim().min(1, "请选择地址类型"),
        contactName: z.string(),
        address: z.string(),
        isDefault: z.boolean(),
    })
    .superRefine((value, context) => {
        if (!value.existingId && !value.address.trim()) {
            context.addIssue({
                code: "custom",
                path: ["address"],
                message: "请填写地址",
            })
        }
    })

const bankAccountRowSchema = z
    .object({
        existingId: z.string().optional(),
        accountName: z.string().trim().min(1, "请填写户名"),
        bankName: z.string().trim().min(1, "请填写银行名称"),
        branchName: z.string(),
        accountNumber: z.string(),
        isDefault: z.boolean(),
    })
    .superRefine((value, context) => {
        if (!value.existingId && !value.accountNumber.trim()) {
            context.addIssue({
                code: "custom",
                path: ["accountNumber"],
                message: "请填写银行账号",
            })
        }
    })

const unifiedCreditCodeSchema = z
    .string()
    .trim()
    .min(1, "请填写统一社会信用代码")
    .regex(/^[0-9A-Za-z]{18}$/, "统一社会信用代码必须是 18 位字母或数字")

const createSchema = z.object({
    legalName: z.string().trim().min(2, "请填写法定名称"),
    shortName: z.string(),
    unifiedCreditCode: unifiedCreditCodeSchema,
    defaultPaymentTerm: z.string(),
    status: z.enum(["active", "disabled"]),
    changeReason: z.string(),
    contacts: z.array(contactRowSchema),
    addresses: z.array(addressRowSchema),
    bankAccounts: z.array(bankAccountRowSchema),
})

const editSchema = z.object({
    legalName: z.string().trim().min(2, "请填写法定名称"),
    shortName: z.string(),
    unifiedCreditCode: unifiedCreditCodeSchema,
    defaultPaymentTerm: z.string(),
    status: z.enum(["active", "disabled"]),
    changeReason: z.string().trim().min(2, "请填写修订原因"),
    contacts: z.array(contactRowSchema),
    addresses: z.array(addressRowSchema),
    bankAccounts: z.array(bankAccountRowSchema),
})

export { createSchema, editSchema }
