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

export const uploadErrorMessage = (error: unknown): string =>
    getErrorMessage(error, "上传失败，请使用原任务号重试。")

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

    const selectedCustomerId = useSelector(
        form.store,
        (state) => state.values.customerId,
    )
    const customerQuery = useCustomerCenterQuery(
        selectedCustomerId || initialCustomerId,
    )
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
        const today = new Date()
        const pad = (n: number) => String(n).padStart(2, "0")
        const todayText = `${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`
        const nextYear = new Date(today)
        nextYear.setFullYear(today.getFullYear() + 1)
        const nextYearText = `${nextYear.getFullYear()}-${pad(nextYear.getMonth() + 1)}-${pad(nextYear.getDate())}`
        form.reset(
            {
                pdfFile: null,
                contractNo: "",
                customerId: initialCustomerId,
                customerName: "",
                settlementPartyId: "",
                settlementPartyName: "",
                paymentTerms: "CONTRACT",
                signedAt: todayText,
                validFrom: todayText,
                validTo: nextYearText,
            },
            // useAppForm 下一次 layout effect 仍会同步声明处的空表单默认值。
            // 保留该默认值，避免未 touch 的开窗日期种子被 formApi.update 回滚为空。
            { keepDefaultValues: true },
        )
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
        if (!values.settlementPartyId && customer.partyId) {
            form.setFieldValue("settlementPartyId", customer.partyId)
            form.setFieldValue(
                "settlementPartyName",
                customer.currentRevision.legalName,
            )
        }
        if (
            values.paymentTerms === "CONTRACT" &&
            customer.currentRevision.defaultPaymentTerm
        ) {
            form.setFieldValue(
                "paymentTerms",
                customer.currentRevision.defaultPaymentTerm,
            )
        }
    }, [customerQuery.data, form, open])

    return {
        form,
        dirty,
        uploadMutation,
        canReadAllCustomers,
        customerPartyId: customerQuery.data?.partyId ?? "",
        discardOpen,
        setDiscardOpen,
    }
}
