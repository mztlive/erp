"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"

import type { SupplierEditor } from "@/features/master-data/hooks/use-supplier-editor"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    validateSupplierEditorFields,
    type SupplierFieldKey,
} from "@/features/master-data/lib/supplier-editor-model"

/**
 * 供应商编辑器右上角保存流程：字段校验 → 变更原因弹窗 → 提交。
 * 从 `SupplierEditorForm` 的 Subscribe 渲染回调中抽出，改为直接订阅表单值。
 */
export function useSupplierSaveFlow(editor: SupplierEditor) {
    const {
        isCreate,
        detailQuery,
        data,
        form,
        setFormError,
        setSaveReasonOpen,
        setReasonDraft,
        setReasonError,
        pendingChangeReasonRef,
        sensitiveByLabel,
        initialFormValues,
        reasonDraft,
    } = editor

    const values = useSelector(form.store, (state) => state.values)

    const setFieldValue = React.useCallback(
        (key: SupplierFieldKey, next: string) => {
            form.setFieldValue(key, next)
        },
        [form],
    )

    /** 右上角保存：先校验字段，再弹窗填写变更原因。 */
    const requestSave = React.useCallback(
        (event?: React.FormEvent) => {
            event?.preventDefault()
            const validation = validateSupplierEditorFields(values, {
                hasStoredContactPhone: data?.sensitiveFields.some(
                    (field) =>
                        field.label === "联系电话" || field.label === "联系人",
                ),
                originalContactName: initialFormValues.contactName,
                hasStoredBankAccount: data?.sensitiveFields.some(
                    (field) => field.label === "银行账号",
                ),
                originalBankName: initialFormValues.bankName,
            })
            if (validation) {
                setFormError(validation)
                return
            }
            setFormError(null)
            setReasonDraft(
                isCreate
                    ? values.changeReason.trim() || "新建供应商"
                    : values.changeReason,
            )
            setReasonError(null)
            setSaveReasonOpen(true)
        },
        [
            data?.sensitiveFields,
            initialFormValues.bankName,
            initialFormValues.contactName,
            isCreate,
            setFormError,
            setReasonDraft,
            setReasonError,
            setSaveReasonOpen,
            values,
        ],
    )

    const confirmSaveWithReason = React.useCallback(() => {
        const reason = reasonDraft.trim()
        if (reason.length < 2) {
            setReasonError("请填写本次保存的变更原因")
            return
        }
        setReasonError(null)
        pendingChangeReasonRef.current = reason
        form.setFieldValue("changeReason", reason)
        setSaveReasonOpen(false)
        void form.handleSubmit()
    }, [
        form,
        pendingChangeReasonRef,
        reasonDraft,
        setReasonError,
        setSaveReasonOpen,
    ])

    const phoneSensitive =
        sensitiveByLabel.get("联系电话") ?? sensitiveByLabel.get("联系人")
    const addressSensitive = sensitiveByLabel.get("经营地址")
    const bankSensitive = sensitiveByLabel.get("银行账号")

    const refreshSensitiveToken = React.useCallback(
        async (labels: readonly string[]): Promise<string | undefined> => {
            const refreshed = await detailQuery.refetch()
            return refreshed.data?.sensitiveFields.find((field) =>
                labels.includes(field.label),
            )?.revealToken
        },
        [detailQuery],
    )

    const summaryRows: Array<{ label: string; value: string }> =
        React.useMemo(
            () => [
                {
                    label: masterDataCopy.fContactName,
                    value: values.contactName.trim() || "—",
                },
                {
                    label: masterDataCopy.fSettlement,
                    value: values.settlement || "—",
                },
                {
                    label: masterDataCopy.fSupplierRating,
                    value: values.supplierRating || "—",
                },
                {
                    label: masterDataCopy.fCapability,
                    value: values.capability || "—",
                },
            ],
            [
                values.capability,
                values.contactName,
                values.settlement,
                values.supplierRating,
            ],
        )

    return {
        values,
        setFieldValue,
        requestSave,
        confirmSaveWithReason,
        phoneSensitive,
        addressSensitive,
        bankSensitive,
        refreshSensitiveToken,
        summaryRows,
    }
}
