"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { z } from "zod"
import { invoiceFileReference } from "@/lib/invoice-files"
import { toast } from "@/components/ui/toast"
import type { AllocationSessionState } from "./use-allocation-session"
import { useWorkspaceFormGuard } from "@/lib/workspace-form-guard"

const snapshotSchema = z.object({
    invoice: z.object({
        allocationTaxes: z.record(z.string(), z.string()),
        invoiceCode: z.string(),
        invoiceNo: z.string(),
        invoiceDate: z.string(),
        grossAmount: z.string(),
        netAmount: z.string(),
        taxAmount: z.string(),
    }),
    selected: z.array(z.string()),
    amounts: z.record(z.string(), z.string()),
})
const draftSchema = snapshotSchema.extend({
    savedAt: z.string(),
    invoiceFileNames: z.array(z.string()).default([]),
})

/** 登记草稿按账号、供应商、原票和会话隔离；恢复时重新核对当前应付池。 */
export function useInvoiceRegistrationDraft(
    state: AllocationSessionState,
    userId: string,
) {
    const { session } = state
    const key = session
        ? `erp.invoice-registration.v1:${[userId, session.supplierId, session.existingInvoiceId ?? "new", session.draftSessionId].map(encodeURIComponent).join(":")}`
        : null
    const restored = useRef<string | null>(null)
    const [hint, setHint] = useState<string | null>(null)
    const [baseline, setBaseline] = useState<string | null>(null)
    const { invoiceFiles, ...invoice } = state.invoiceForm.state.values
    const snapshot = {
        invoice,
        selected: [...state.selected],
        amounts: state.amounts,
    }
    const signature = JSON.stringify({
        ...snapshot,
        invoiceFileNames: invoiceFiles.map(invoiceFileReference),
    })
    const completed = state.result?.status === "succeeded"
    const guard = useMemo(
        () => ({
            state: {
                isDefaultValue:
                    completed || baseline === null || baseline === signature,
                isSubmitting: state.isSubmitting,
            },
            store: { subscribe: () => ({ unsubscribe() {} }) },
        }),
        [baseline, completed, signature, state.isSubmitting],
    )
    useWorkspaceFormGuard(guard)

    useEffect(() => {
        if (!key || restored.current === key) return
        restored.current = key
        try {
            const raw = localStorage.getItem(key)
            if (!raw) {
                setBaseline(signature)
                return
            }
            const saved = draftSchema.parse(JSON.parse(raw))
            const available = new Set(
                state.pool.map((row) => row.payableAccountId),
            )
            const selected = saved.selected.filter((id) => available.has(id))
            const next = {
                invoice: { ...saved.invoice, invoiceFiles: [] },
                selected,
                amounts: saved.amounts,
            }
            state.restoreInvoiceDraft(next)
            setBaseline(
                JSON.stringify({
                    ...next,
                    invoice: saved.invoice,
                    invoiceFileNames: [],
                }),
            )
            setHint(
                [
                    selected.length === saved.selected.length
                        ? "已恢复本机草稿，请核对当前可收票金额"
                        : "已恢复本机草稿；部分单据已不可核销，请重新核对",
                    saved.invoiceFileNames.length
                        ? `原选 ${saved.invoiceFileNames.length} 个发票文件未保存，请重新选择`
                        : null,
                ]
                    .filter(Boolean)
                    .join("；"),
            )
        } catch {
            setHint("本机草稿读取失败，请重新填写或重试")
            toast.add({
                type: "error",
                title: "本机草稿读取失败，请重新填写或重试",
            })
        }
    }, [key, signature, state])

    useEffect(() => {
        if (!key || !completed) return
        state.invoiceForm.reset(state.invoiceForm.state.values)
        try {
            localStorage.removeItem(key)
        } catch {
            toast.add({ type: "error", title: "发票已登记，本机草稿清理失败" })
        }
        // 仅在本会话提交成功时清理一次。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key, completed])

    function save() {
        if (!key || !userId || state.isSubmitting) return
        try {
            const savedAt = new Date().toISOString()
            localStorage.setItem(
                key,
                JSON.stringify({
                    ...snapshot,
                    savedAt,
                    invoiceFileNames: invoiceFiles.map(invoiceFileReference),
                }),
            )
            state.invoiceForm.reset(state.invoiceForm.state.values)
            setBaseline(signature)
            setHint(
                `草稿已保存到本机 ${new Date(savedAt).toLocaleTimeString("zh-CN")}${invoiceFiles.length ? "；发票文件未保存，恢复时需重新选择" : ""}`,
            )
        } catch {
            toast.add({
                type: "error",
                title: "草稿保存失败，请检查浏览器存储空间后重试",
            })
        }
    }
    return {
        save,
        hint:
            hint && baseline !== null && signature !== baseline
                ? `${hint} · 有未保存的修改`
                : hint,
    }
}
