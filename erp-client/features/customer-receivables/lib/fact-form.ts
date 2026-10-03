"use client"

import { z } from "zod"

import type { AllocationSessionView } from "@/features/customer-receivables/types"
import { invoiceFilesError } from "@/features/customer-receivables/lib/invoice-files"

export const factFormSchema = z.object({
    receivedAt: z.string(),
    amount: z.string(),
    bankReference: z.string(),
    invoiceCode: z.string(),
    invoiceNo: z.string(),
    invoiceDate: z.string(),
    grossAmount: z.string(),
    netAmount: z.string(),
    taxAmount: z.string(),
    invoiceFiles: z
        .array(
            z.custom<File>(
                (value) => typeof File !== "undefined" && value instanceof File,
                "发票附件文件无效",
            ),
        )
        .superRefine((files, context) => {
            const message = invoiceFilesError(files)
            if (message) context.addIssue({ code: "custom", message })
        }),
})

export type FactFormValues = z.infer<typeof factFormSchema>

export function factDefaultValues(
    fact: AllocationSessionView["fact"],
    isReceipt: boolean,
): FactFormValues {
    if (isReceipt) {
        return {
            receivedAt: fact.receivedAt ?? "",
            amount: fact.amount ?? "",
            bankReference: fact.bankReference ?? "",
            invoiceCode: "",
            invoiceNo: "",
            invoiceDate: "",
            grossAmount: "",
            netAmount: "",
            taxAmount: "",
            invoiceFiles: [],
        }
    }
    return {
        receivedAt: "",
        amount: "",
        bankReference: "",
        invoiceCode: fact.invoiceCode ?? "",
        invoiceNo: fact.invoiceNo ?? "",
        invoiceDate: fact.invoiceDate ?? "",
        grossAmount: fact.grossAmount ?? "",
        netAmount: fact.netAmount ?? "",
        taxAmount: fact.taxAmount ?? "",
        invoiceFiles: [...(fact.invoiceFiles ?? [])],
    }
}

/** 表单值 → 会话 fact（与保存/提交共用同一映射）。 */
export function factFromValues(
    values: FactFormValues,
    isReceipt: boolean,
): AllocationSessionView["fact"] {
    if (isReceipt) {
        return {
            receivedAt: values.receivedAt,
            amount: values.amount,
            bankReference: values.bankReference,
        }
    }
    return {
        invoiceCode: values.invoiceCode,
        invoiceNo: values.invoiceNo,
        invoiceDate: values.invoiceDate,
        grossAmount: values.grossAmount,
        netAmount: values.netAmount,
        taxAmount: values.taxAmount,
        invoiceFiles: [...values.invoiceFiles],
        invoiceKind: "blue",
    }
}
