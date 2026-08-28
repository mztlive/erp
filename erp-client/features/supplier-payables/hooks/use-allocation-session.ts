"use client"

import * as React from "react"

import { useStore } from "@tanstack/react-form"

import { useAppForm } from "@/components/form"
import {
    useAllocationSessionQuery,
    useResolveUnknownMutation,
    useSaveAllocationDraftMutation,
    useSubmitInvoiceMutation,
    useSubmitPaymentMutation,
} from "@/features/supplier-payables/hooks/queries"
import { buildAllocationIssues } from "@/features/supplier-payables/lib/allocation-validation"
import { beginFreshAllocationAttempt } from "@/features/supplier-payables/api/shared"
import {
    BANK_RECEIPT_PENDING_REFERENCE,
    cents,
    fromCents,
    invoiceSchema,
    paymentSchema,
    todayInput,
    withLockedPaymentAmount,
} from "@/features/supplier-payables/lib/allocation-model"
import type {
    AllocationTrack,
    FormalSubmitResult,
} from "@/features/supplier-payables/types"

export type AllocationSessionParams = {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
    paymentWorkItemId?: string
    expectedPaymentTaskVersion?: string
    paymentPayableAccountId?: string
    paymentRecipientBankAccountId?: string
    paymentRecipientBankAccountVersion?: number
}

export type AllocationSessionOptions = {
    onCompleted?: (result: FormalSubmitResult) => void
    /** 成功结果不进入页内 FormalActionResult，由 onCompleted 收口。 */
    consumeSucceededResult?: boolean
    /** 会话加载后把服务端/客户端会话主键回写页面状态，保持跨刷新会话身份稳定。 */
    onDraftSessionIdChange?: (draftSessionId: string) => void
}

const HYDRATE_FIELD_OPTIONS = {
    dontUpdateMeta: true,
    dontValidate: true,
} as const

/** 核销工作区全部状态与派生数据：会话查询、两张记录表单、勾选/金额状态、校验问题与提交。 */
export function useAllocationSession(
    {
        track,
        supplierId,
        draftSessionId,
        purchaseOrderId,
        returnTo,
        fromWorkspace,
        existingInvoiceId,
        preselectPayableAccountId,
        paymentWorkItemId,
        expectedPaymentTaskVersion,
        paymentPayableAccountId,
        paymentRecipientBankAccountId,
        paymentRecipientBankAccountVersion,
    }: AllocationSessionParams,
    {
        onCompleted,
        consumeSucceededResult = false,
        onDraftSessionIdChange,
    }: AllocationSessionOptions = {},
) {
    const sessionQuery = useAllocationSessionQuery({
        track,
        supplierId,
        draftSessionId,
        purchaseOrderId,
        returnTo,
        fromWorkspace,
        existingInvoiceId,
        preselectPayableAccountId,
    })
    const submitPayment = useSubmitPaymentMutation()
    const submitInvoice = useSubmitInvoiceMutation()
    const saveDraft = useSaveAllocationDraftMutation()
    const resolveUnknown = useResolveUnknownMutation()

    const session = sessionQuery.data
    const policy = session?.payablePriorityPolicy
    const pool = React.useMemo(
        () =>
            track === "payment" && paymentPayableAccountId
                ? (session?.pool.filter(
                      (item) =>
                          item.payableAccountId === paymentPayableAccountId,
                  ) ?? [])
                : (session?.pool ?? []),
        [paymentPayableAccountId, session?.pool, track],
    )

    const [amounts, setAmounts] = React.useState<Record<string, string>>({})
    const [selected, setSelected] = React.useState<Set<string>>(new Set())
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [result, setResult] = React.useState<FormalSubmitResult | null>(null)
    const [draftHint, setDraftHint] = React.useState<string | null>(null)
    const idempotencyRef = React.useRef<string | null>(null)

    const paymentForm = useAppForm({
        defaultValues: {
            paidAt: todayInput(),
            amount:
                session?.existingUnallocated ?? session?.existingAmount ?? "",
            bankReference: "",
            bankReceiptAssetId: "",
            bankReceipt: null as File | null,
            note: "",
        },
        validators: { onChange: paymentSchema },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const invoiceForm = useAppForm({
        defaultValues: {
            invoiceCode: "",
            invoiceNo: "",
            invoiceDate: new Date().toISOString().slice(0, 10),
            grossAmount:
                session?.existingUnallocated ?? session?.existingAmount ?? "",
            netAmount: "",
            taxAmount: "",
        },
        validators: { onChange: invoiceSchema },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const preselectKey = [
        ...(session?.preselectedPayableAccountIds ?? []),
        paymentPayableAccountId ?? "",
    ].join("|")

    // 订阅表单 store：记录金额/校验问题/提交按钮随输入实时更新
    const paymentValues = useStore(paymentForm.store, (s) => s.values)
    const invoiceValues = useStore(invoiceForm.store, (s) => s.values)

    // 会话身份回写：fetchAllocationSession 在未带 draftSessionId 时会现场生成新会话主键，
    // 若页面不把它写回状态/URL，任何查询失效后的重取都会换新主键，预选效果重跑并清空用户勾选。
    React.useEffect(() => {
        if (!session?.draftSessionId) return
        if (session.draftSessionId !== draftSessionId) {
            onDraftSessionIdChange?.(session.draftSessionId)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅会话主键变化时同步
    }, [session?.draftSessionId, draftSessionId])

    React.useEffect(() => {
        if (!session) return
        const next = new Set(
            track === "payment" && paymentPayableAccountId
                ? pool.map((item) => item.payableAccountId)
                : session.preselectedPayableAccountIds,
        )
        setSelected(next)
        const am: Record<string, string> = {}
        let prefillSum = 0
        for (const id of next) {
            const item = pool.find((p) => p.payableAccountId === id)
            if (!item) continue
            const open =
                track === "payment" ? item.openTotal : item.openInvoiceableTotal
            am[id] = open
            prefillSum += cents(open)
        }
        setAmounts((prev) => ({ ...am, ...prev }))
        // 预选目标时同步预填记录金额，避免重复输入（继续核销场景除外）
        if (!session.existingInvoiceId) {
            const prefill = fromCents(prefillSum)
            if (track === "payment") {
                paymentForm.setFieldValue(
                    "amount",
                    prefill,
                    HYDRATE_FIELD_OPTIONS,
                )
            } else {
                invoiceForm.setFieldValue(
                    "grossAmount",
                    prefill,
                    HYDRATE_FIELD_OPTIONS,
                )
            }
        }
        if (track === "payment") {
            paymentForm.setFieldValue(
                "bankReceiptAssetId",
                "",
                HYDRATE_FIELD_OPTIONS,
            )
            paymentForm.setFieldValue(
                "bankReceipt",
                null,
                HYDRATE_FIELD_OPTIONS,
            )
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 会话与预选变化时同步
    }, [session?.draftSessionId, preselectKey])

    React.useEffect(() => {
        if (!session?.existingUnallocated) return
        if (track === "payment") {
            paymentForm.setFieldValue(
                "amount",
                session.existingUnallocated,
                HYDRATE_FIELD_OPTIONS,
            )
        } else {
            invoiceForm.setFieldValue(
                "grossAmount",
                session.existingUnallocated,
                HYDRATE_FIELD_OPTIONS,
            )
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [session?.existingUnallocated, session?.existingInvoiceId])

    const factAmount =
        track === "payment" ? paymentValues.amount : invoiceValues.grossAmount

    const effectiveAmounts = React.useMemo(() => {
        if (track !== "payment" || !paymentPayableAccountId) {
            return amounts
        }
        return withLockedPaymentAmount(
            amounts,
            paymentPayableAccountId,
            paymentValues.amount,
        )
    }, [amounts, paymentPayableAccountId, paymentValues.amount, track])

    const allocatedHint = React.useMemo(() => {
        let c = 0
        for (const id of selected) {
            c += cents(effectiveAmounts[id] ?? "0")
        }
        return fromCents(c)
    }, [effectiveAmounts, selected])

    const unallocatedHint = React.useMemo(() => {
        return fromCents(
            Math.max(0, cents(factAmount || "0") - cents(allocatedHint)),
        )
    }, [factAmount, allocatedHint])

    const mixedSources = React.useMemo(() => {
        const types = new Set(
            pool
                .filter((p) => selected.has(p.payableAccountId))
                .map((p) => p.sourceType) ?? [],
        )
        return types.size > 1
    }, [pool, selected])

    const policyBlocksAuto =
        mixedSources &&
        (!policy ||
            policy.state !== "AVAILABLE" ||
            !policy.mixedAutoAllocationAllowed)

    const issues = buildAllocationIssues({
        track,
        selected,
        amounts: effectiveAmounts,
        pool,
        allocatedHint,
        factAmount,
        existingInvoiceId: session?.existingInvoiceId,
        existingUnallocated: session?.existingUnallocated,
        existingAmount: session?.existingAmount,
    })

    const hasPaymentTask =
        track !== "payment" ||
        Boolean(
            paymentWorkItemId &&
            expectedPaymentTaskVersion &&
            paymentPayableAccountId &&
            paymentRecipientBankAccountId &&
            paymentRecipientBankAccountVersion,
        )
    const hasPaymentReceipt =
        track !== "payment" ||
        Boolean(
            paymentValues.bankReceipt ||
            paymentValues.bankReceiptAssetId.trim(),
        )
    const canSubmit =
        issues.length === 0 && !result && hasPaymentTask && hasPaymentReceipt

    function toggleItem(
        payableAccountId: string,
        checked: boolean | "indeterminate",
        open: string,
    ) {
        if (
            track === "payment" &&
            payableAccountId !== paymentPayableAccountId
        ) {
            return
        }
        setSelected((prev) => {
            const next = new Set(prev)
            if (checked) next.add(payableAccountId)
            else next.delete(payableAccountId)
            return next
        })
        if (checked && !amounts[payableAccountId]) {
            setAmounts((m) => ({ ...m, [payableAccountId]: open }))
        }
    }

    function setAmountFor(payableAccountId: string, value: string) {
        setAmounts((m) => ({ ...m, [payableAccountId]: value }))
    }

    function toggleSelectAll() {
        const ids = pool.map((p) => p.payableAccountId)
        const allSelected =
            ids.length > 0 && ids.every((id) => selected.has(id))
        setSelected(new Set(allSelected ? [] : ids))
        if (!allSelected && session) {
            setAmounts((m) => {
                const next = { ...m }
                for (const p of pool) {
                    next[p.payableAccountId] =
                        track === "payment"
                            ? p.openTotal
                            : p.openInvoiceableTotal
                }
                return next
            })
        }
    }

    function fillAllSelected() {
        if (!session) return
        setAmounts((m) => {
            const next = { ...m }
            for (const p of pool) {
                if (selected.has(p.payableAccountId)) {
                    next[p.payableAccountId] =
                        track === "payment"
                            ? p.openTotal
                            : p.openInvoiceableTotal
                }
            }
            return next
        })
    }

    async function handleSaveDraft() {
        if (!session) return
        const saved = await saveDraft.mutateAsync({
            draftSessionId: session.draftSessionId,
            track,
            supplierId,
            formSnapshot: {
                amounts: effectiveAmounts,
                selected: [...selected],
                payment: paymentForm.state.values,
                invoice: invoiceForm.state.values,
            },
        })
        setDraftHint(
            `草稿已保存 ${new Date(saved.savedAt).toLocaleTimeString("zh-CN")}`,
        )
    }

    function requestSubmit() {
        if (track === "payment") {
            void paymentForm.handleSubmit()
            return
        }
        if (session?.existingInvoiceId) {
            setConfirmOpen(true)
            return
        }
        void invoiceForm.handleSubmit()
    }

    async function doSubmit() {
        if (!session) return
        if (
            track === "payment" &&
            (!paymentWorkItemId ||
                !expectedPaymentTaskVersion ||
                !paymentPayableAccountId ||
                !paymentRecipientBankAccountId ||
                !paymentRecipientBankAccountVersion)
        ) {
            return
        }
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w12_${track}_${session.draftSessionId}_${Date.now()}`
        }
        const targets = [...selected].map((payableAccountId) => {
            const item = pool.find(
                (p) => p.payableAccountId === payableAccountId,
            )!
            return {
                payableAccountId,
                payableEntryId: item.primaryEntryId,
                amount: effectiveAmounts[payableAccountId] || "0",
                entryLockVersion: item.entryLockVersion,
                accountLockVersion: item.accountLockVersion,
            }
        })

        const explicitSelection = true // UI always collects explicit picks

        let res: FormalSubmitResult
        if (track === "payment") {
            const v = paymentForm.state.values
            res = await submitPayment.mutateAsync({
                workItemId: paymentWorkItemId!,
                expectedTaskVersion: expectedPaymentTaskVersion!,
                expectedPayeeBankAccountId: paymentRecipientBankAccountId!,
                expectedPayeeBankAccountVersion:
                    paymentRecipientBankAccountVersion!,
                draftSessionId: session.draftSessionId,
                supplierId,
                paidAt: v.paidAt,
                amount: v.amount,
                bankReference: v.bankReference,
                bankReceiptAssetId: v.bankReceipt
                    ? BANK_RECEIPT_PENDING_REFERENCE
                    : v.bankReceiptAssetId,
                bankReceiptFile: v.bankReceipt,
                note: v.note,
                targets,
                payablePriorityPolicyId: policy?.payablePriorityPolicyId,
                payablePriorityPolicyVersion:
                    policy?.payablePriorityPolicyVersion,
                explicitSelection,
                idempotencyKey: idempotencyRef.current,
            })
            if (res.status === "succeeded") {
                res = {
                    ...res,
                    title: "付款已登记",
                    description: `付款单号 ${res.documentNo ?? res.reference ?? ""} 已过账并核销。`,
                    facts: [
                        {
                            label: "付款单号",
                            value: res.documentNo ?? res.reference ?? "",
                        },
                        {
                            label: "净已分配",
                            value: res.allocatedTotal ?? "0.00",
                        },
                        {
                            label: "未分配余额",
                            value: res.unallocatedAmount ?? "0.00",
                        },
                        { label: "供应商", value: session.supplierName },
                    ],
                }
            }
        } else {
            const v = invoiceForm.state.values
            res = await submitInvoice.mutateAsync({
                draftSessionId: session.draftSessionId,
                supplierId,
                invoiceCode: v.invoiceCode,
                invoiceNo: v.invoiceNo,
                invoiceDate: v.invoiceDate,
                grossAmount: session.existingInvoiceId
                    ? (session.existingAmount ?? v.grossAmount)
                    : v.grossAmount,
                netAmount: v.netAmount,
                taxAmount: v.taxAmount,
                invoiceKind: "BLUE",
                targets,
                payablePriorityPolicyId: policy?.payablePriorityPolicyId,
                payablePriorityPolicyVersion:
                    policy?.payablePriorityPolicyVersion,
                explicitSelection,
                existingInvoiceId: session.existingInvoiceId,
                idempotencyKey: idempotencyRef.current,
            })
        }

        setConfirmOpen(false)
        if (res.status === "succeeded" && consumeSucceededResult) {
            onCompleted?.(res)
            return
        }
        setResult({ ...res, returnTo: returnTo ?? session.returnTo })
        if (res.status === "succeeded") {
            onCompleted?.(res)
        }
    }

    async function handleResolveUnknown(): Promise<boolean> {
        if (!idempotencyRef.current) return false
        const r = await resolveUnknown.mutateAsync(idempotencyRef.current)
        if (r) {
            if (r.status === "succeeded" && consumeSucceededResult) {
                onCompleted?.(r)
                return true
            }
            setResult({ ...r, returnTo: returnTo ?? session?.returnTo })
            if (r.status === "succeeded") onCompleted?.(r)
            return true
        }
        return false
    }

    /** 结束上一笔提交并为分次付款建立新的会话与幂等身份。 */
    function startFreshAttempt(): string {
        const nextDraftSessionId = beginFreshAllocationAttempt(
            session?.draftSessionId,
            idempotencyRef.current,
        )
        idempotencyRef.current = null
        paymentForm.reset()
        invoiceForm.reset()
        submitPayment.reset()
        submitInvoice.reset()
        saveDraft.reset()
        resolveUnknown.reset()
        setAmounts({})
        setSelected(new Set())
        setConfirmOpen(false)
        setResult(null)
        setDraftHint(null)
        return nextDraftSessionId
    }

    return {
        sessionQuery,
        session,
        policy,
        pool,
        amounts: effectiveAmounts,
        selected,
        confirmOpen,
        setConfirmOpen,
        result,
        draftHint,
        paymentForm,
        invoiceForm,
        factAmount,
        allocatedHint,
        unallocatedHint,
        mixedSources,
        policyBlocksAuto,
        issues,
        canSubmit,
        isSubmitting: submitPayment.isPending || submitInvoice.isPending,
        isSavingDraft: saveDraft.isPending,
        hasSubmitKey: idempotencyRef.current !== null,
        toggleItem,
        setAmountFor,
        toggleSelectAll,
        fillAllSelected,
        handleSaveDraft,
        requestSubmit,
        doSubmit,
        handleResolveUnknown,
        startFreshAttempt,
    }
}

/** 核销工作区状态：会话查询、表单、勾选金额、校验问题与提交动作。 */
export type AllocationSessionState = ReturnType<typeof useAllocationSession>
