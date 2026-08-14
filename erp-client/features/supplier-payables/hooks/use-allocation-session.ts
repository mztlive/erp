"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import {
    useAllocationSessionQuery,
    useResolveUnknownMutation,
    useSaveAllocationDraftMutation,
    useSubmitInvoiceMutation,
    useSubmitPaymentMutation,
} from "@/features/supplier-payables/hooks/queries"
import { buildAllocationIssues } from "@/features/supplier-payables/lib/allocation-validation"
import {
    cents,
    fromCents,
    invoiceSchema,
    paymentSchema,
    todayInput,
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
    existingPaymentId?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
}

export type AllocationSessionOptions = {
    onCompleted?: (result: FormalSubmitResult) => void
}

/** 核销工作区全部状态与派生数据：会话查询、两张记录表单、勾选/金额状态、校验问题与提交。 */
export function useAllocationSession(
    {
        track,
        supplierId,
        draftSessionId,
        purchaseOrderId,
        returnTo,
        fromWorkspace,
        existingPaymentId,
        existingInvoiceId,
        preselectPayableAccountId,
    }: AllocationSessionParams,
    { onCompleted }: AllocationSessionOptions = {},
) {
    const sessionQuery = useAllocationSessionQuery({
        track,
        supplierId,
        draftSessionId,
        purchaseOrderId,
        returnTo,
        fromWorkspace,
        existingPaymentId,
        existingInvoiceId,
        preselectPayableAccountId,
    })
    const submitPayment = useSubmitPaymentMutation()
    const submitInvoice = useSubmitInvoiceMutation()
    const saveDraft = useSaveAllocationDraftMutation()
    const resolveUnknown = useResolveUnknownMutation()

    const session = sessionQuery.data
    const policy = session?.payablePriorityPolicy

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

    const preselectKey = session?.preselectedPayableAccountIds.join("|")

    React.useEffect(() => {
        if (!session) return
        const next = new Set(session.preselectedPayableAccountIds)
        setSelected(next)
        const am: Record<string, string> = {}
        let prefillSum = 0
        for (const id of next) {
            const item = session.pool.find((p) => p.payableAccountId === id)
            if (!item) continue
            const open =
                track === "payment" ? item.openTotal : item.openInvoiceableTotal
            am[id] = open
            prefillSum += cents(open)
        }
        setAmounts((prev) => ({ ...am, ...prev }))
        // 预选目标时同步预填记录金额，避免重复输入（继续核销场景除外）
        if (!session.existingPaymentId && !session.existingInvoiceId) {
            const prefill = fromCents(prefillSum)
            if (track === "payment") {
                paymentForm.setFieldValue("amount", prefill)
            } else {
                invoiceForm.setFieldValue("grossAmount", prefill)
            }
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 会话与预选变化时同步
    }, [session?.draftSessionId, preselectKey])

    React.useEffect(() => {
        if (!session?.existingUnallocated) return
        if (track === "payment") {
            paymentForm.setFieldValue("amount", session.existingUnallocated)
        } else {
            invoiceForm.setFieldValue(
                "grossAmount",
                session.existingUnallocated,
            )
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
        session?.existingUnallocated,
        session?.existingPaymentId,
        session?.existingInvoiceId,
    ])

    const factAmount =
        track === "payment"
            ? paymentForm.state.values.amount
            : invoiceForm.state.values.grossAmount

    const allocatedHint = React.useMemo(() => {
        let c = 0
        for (const id of selected) {
            c += cents(amounts[id] ?? "0")
        }
        return fromCents(c)
    }, [selected, amounts])

    const unallocatedHint = React.useMemo(() => {
        return fromCents(
            Math.max(0, cents(factAmount || "0") - cents(allocatedHint)),
        )
    }, [factAmount, allocatedHint])

    const mixedSources = React.useMemo(() => {
        const types = new Set(
            session?.pool
                .filter((p) => selected.has(p.payableAccountId))
                .map((p) => p.sourceType) ?? [],
        )
        return types.size > 1
    }, [session?.pool, selected])

    const policyBlocksAuto =
        mixedSources &&
        (!policy ||
            policy.state !== "AVAILABLE" ||
            !policy.mixedAutoAllocationAllowed)

    const issues = buildAllocationIssues({
        track,
        selected,
        amounts,
        pool: session?.pool,
        allocatedHint,
        factAmount,
        existingPaymentId: session?.existingPaymentId,
        existingInvoiceId: session?.existingInvoiceId,
        existingUnallocated: session?.existingUnallocated,
        existingAmount: session?.existingAmount,
    })

    const canSubmit = issues.length === 0 && !result

    function toggleItem(
        payableAccountId: string,
        checked: boolean | "indeterminate",
        open: string,
    ) {
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
        const ids = session?.pool.map((p) => p.payableAccountId) ?? []
        const allSelected =
            ids.length > 0 && ids.every((id) => selected.has(id))
        setSelected(new Set(allSelected ? [] : ids))
        if (!allSelected && session) {
            setAmounts((m) => {
                const next = { ...m }
                for (const p of session.pool) {
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
            for (const p of session.pool) {
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
                amounts,
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
        if (session?.existingPaymentId || session?.existingInvoiceId) {
            setConfirmOpen(true)
            return
        }
        if (track === "payment") void paymentForm.handleSubmit()
        else void invoiceForm.handleSubmit()
    }

    async function doSubmit() {
        if (!session) return
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w12_${track}_${session.draftSessionId}_${Date.now()}`
        }
        const targets = [...selected].map((payableAccountId) => {
            const item = session.pool.find(
                (p) => p.payableAccountId === payableAccountId,
            )!
            return {
                payableAccountId,
                payableEntryId: item.primaryEntryId,
                amount: amounts[payableAccountId] || "0",
                entryLockVersion: item.entryLockVersion,
                accountLockVersion: item.accountLockVersion,
            }
        })

        const explicitSelection = true // UI always collects explicit picks

        let res: FormalSubmitResult
        if (track === "payment") {
            const v = paymentForm.state.values
            res = await submitPayment.mutateAsync({
                draftSessionId: session.draftSessionId,
                supplierId,
                paidAt: v.paidAt,
                amount: session.existingPaymentId
                    ? (session.existingAmount ?? v.amount)
                    : v.amount,
                bankReference: v.bankReference,
                note: v.note,
                targets,
                payablePriorityPolicyId: policy?.payablePriorityPolicyId,
                payablePriorityPolicyVersion:
                    policy?.payablePriorityPolicyVersion,
                explicitSelection,
                existingPaymentId: session.existingPaymentId,
                idempotencyKey: idempotencyRef.current,
            })
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
        setResult({ ...res, returnTo: returnTo ?? session.returnTo })
        if (res.status === "succeeded") {
            onCompleted?.(res)
        }
    }

    async function handleResolveUnknown(): Promise<boolean> {
        if (!idempotencyRef.current) return false
        const r = await resolveUnknown.mutateAsync(idempotencyRef.current)
        if (r) {
            setResult({ ...r, returnTo: returnTo ?? session?.returnTo })
            if (r.status === "succeeded") onCompleted?.(r)
            return true
        }
        return false
    }

    return {
        sessionQuery,
        session,
        policy,
        amounts,
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
    }
}
