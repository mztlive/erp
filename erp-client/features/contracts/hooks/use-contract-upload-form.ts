"use client"

import * as React from "react"
import { z } from "zod"
import { useSelector } from "@tanstack/react-form"

import { useAppForm } from "@/components/form"
import { paymentTermLabel } from "@/lib/business-options"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { contractPdfError } from "@/features/contracts/lib/pdf"
import { useUploadContractPdfMutation } from "@/features/contracts/hooks/queries"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import { useCustomerCenterQuery } from "@/features/customers/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"

export const uploadSchema = z
    .object({
        pdfFile: z.custom<File | null>(),
        contractNo: z.string().trim().min(1, "请填写合同编号"),
        customerId: z.string().trim().min(1, "请选择客户"),
        customerName: z.string().trim().min(2, "请选择客户"),
        settlementPartyId: z.string().trim().min(1, "请选择结算主体"),
        settlementPartyName: z.string().trim().min(2, "请选择结算主体"),
        paymentTerms: z.string().trim().min(1, "请选择付款条件"),
        signedAt: z.string().min(1, "请填写签订日期"),
        validFrom: z.string().min(1, "请填写有效期起"),
        validTo: z.string().min(1, "请填写有效期止"),
    })
    .superRefine((value, context) => {
        const fileError = contractPdfError(value.pdfFile)
        if (fileError) {
            context.addIssue({
                code: "custom",
                path: ["pdfFile"],
                message: fileError,
            })
        }
        if (
            value.validFrom &&
            value.validTo &&
            value.validTo < value.validFrom
        ) {
            context.addIssue({
                code: "custom",
                path: ["validTo"],
                message: "有效期止不能早于有效期起",
            })
        }
    })

export function uploadErrorMessage(error: unknown): string {
    const message = getErrorMessage(error, "上传失败，请使用原任务号重试。")
    if (message === "CONTRACT_NO_EXISTS") {
        return "该合同编号已存在，请打开已有合同核对；重复编号不能新建合同。"
    }
    if (message === "CONTRACT_VALIDITY_INVALID") {
        return "有效期止不能早于有效期起。"
    }
    return message
}

export type UseContractUploadFormOptions = {
    open: boolean
    onOpenChange: (open: boolean) => void
    /** 预选客户（客户中心 / 建单页带入） */
    initialCustomerId?: string
    /** 归档成功后回调；父层负责刷新列表、选中合同或展示结果 */
    onSuccess?: (result: UploadContractPdfResult) => void
}

/**
 * 上传归档表单状态：表单实例、脏状态、打开时种子（日期默认今天/明年同日）、
 * 客户预选与提交（提交走 useUploadContractPdfMutation）。
 */
export function useContractUploadForm({
    open,
    onOpenChange,
    initialCustomerId = "",
    onSuccess,
}: UseContractUploadFormOptions) {
    const customerQuery = useCustomerCenterQuery(initialCustomerId)
    const accountProfile = useAccountProfileQuery()
    const canReadAllCustomers = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )
    const uploadMutation = useUploadContractPdfMutation()
    const [discardOpen, setDiscardOpen] = React.useState(false)

    const form = useAppForm({
        defaultValues: {
            pdfFile: null as File | null,
            contractNo: "",
            customerId: initialCustomerId,
            customerName: "",
            settlementPartyId: "",
            settlementPartyName: "",
            paymentTerms: "CONTRACT",
            signedAt: "",
            validFrom: "",
            validTo: "",
        },
        validators: { onChange: uploadSchema },
        onSubmit: async ({ value }) => {
            if (!value.pdfFile) return
            const result = await uploadMutation.mutateAsync({
                pdfFile: value.pdfFile,
                contractNo: value.contractNo.trim(),
                customerId:
                    value.customerId.trim() || initialCustomerId || undefined,
                customerName: value.customerName.trim(),
                settlementPartyId: value.settlementPartyId.trim() || undefined,
                settlementPartyName: value.settlementPartyName.trim(),
                paymentTerms:
                    paymentTermLabel(value.paymentTerms) ||
                    value.paymentTerms.trim(),
                signedAt: value.signedAt,
                validFrom: value.validFrom,
                validTo: value.validTo,
                idempotencyKey: `upload-${Date.now().toString(36)}`,
            })
            form.reset()
            uploadMutation.reset()
            onOpenChange(false)
            onSuccess?.(result)
        },
    })

    const dirty = useSelector(form.store, (state) => state.isDirty)

    React.useEffect(() => {
        if (!dirty) return
        const onBeforeUnload = (e: BeforeUnloadEvent) => {
            e.preventDefault()
            e.returnValue = "当前输入尚未提交，刷新后将丢失。"
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [dirty])

    React.useEffect(() => {
        if (!open) {
            uploadMutation.reset()
            return
        }
        form.reset()
        // 日期默认今天 / 明年同日，避免硬编码日期随时间失真。
        const today = new Date()
        const pad = (n: number) => String(n).padStart(2, "0")
        const todayText = `${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`
        const nextYear = new Date(today)
        nextYear.setFullYear(today.getFullYear() + 1)
        const nextYearText = `${nextYear.getFullYear()}-${pad(nextYear.getMonth() + 1)}-${pad(nextYear.getDate())}`
        form.setFieldValue("signedAt", todayText)
        form.setFieldValue("validFrom", todayText)
        form.setFieldValue("validTo", nextYearText)
        if (initialCustomerId) {
            form.setFieldValue("customerId", initialCustomerId)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- only seed when dialog opens
    }, [open, initialCustomerId])

    React.useEffect(() => {
        if (!open) return
        const customer = customerQuery.data
        if (!customer) return
        // 幂等补写客户名：StrictMode（dev）双跑 effect 时 open 效果会二次 reset，
        // 以「值为空」为守卫而非 ref，保证名称最终就绪；用户改选客户由 onItemChange 维护。
        const values = form.state.values
        if (values.customerId !== customer.customerId) {
            form.setFieldValue("customerId", customer.customerId)
        }
        if (!values.customerName) {
            form.setFieldValue(
                "customerName",
                customer.currentRevision.legalName,
            )
        }
    }, [customerQuery.data, form, open])

    return {
        form,
        dirty,
        uploadMutation,
        canReadAllCustomers,
        discardOpen,
        setDiscardOpen,
    }
}
