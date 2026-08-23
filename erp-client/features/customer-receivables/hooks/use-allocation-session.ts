"use client"

import * as React from "react"

import { useStore } from "@tanstack/react-form"

import { type ResultState } from "@/components/business/feedback"
import { type ValidationIssue } from "@/components/business"
import { useAppForm } from "@/components/form"
import { getErrorMessage } from "@/lib/api/errors"
import { resultText } from "@/lib/ui-text"
import {
    factDefaultValues,
    factFormSchema,
    factFromValues,
} from "@/features/customer-receivables/lib/fact-form"
import {
    money,
    parseAmt,
} from "@/features/customer-receivables/lib/allocation-math"
import {
    useEnsureCustomerReceiptDraftMutation,
    usePostAllocationMutation,
    useResolvePostUnknownMutation,
    useSaveAllocationDraftMutation,
} from "@/features/customer-receivables/hooks/queries"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { readCustomerReceiptApprovalResponsibility } from "@/features/customer-receivables/lib/customer-receipt-approval"
import type {
    AllocationDraftLine,
    AllocationSessionView,
    AllocationTarget,
    PostAllocationResult,
} from "@/features/customer-receivables/types"

/**
 * 核销工作区控制器：草稿分配行、记录表单、校验问题、保存/提交/结果回填。
 * 组件层只消费返回值渲染，不持有业务状态。
 * 发票模式不读取、不回填审批绑定，也不创建回款草稿。
 */
export function useAllocationSession({
    session,
    onClose,
    onPosted,
}: {
    session: AllocationSessionView
    onClose: () => void
    onPosted: () => void
}) {
    const saveMutation = useSaveAllocationDraftMutation()
    const postMutation = usePostAllocationMutation()
    const resolveMutation = useResolvePostUnknownMutation()
    const ensureReceiptMutation = useEnsureCustomerReceiptDraftMutation()

    const [allocations, setAllocations] = React.useState<AllocationDraftLine[]>(
        () => session.allocations.map((a) => ({ ...a })),
    )
    const [editVersion, setEditVersion] = React.useState(session.editVersion)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [result, setResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [draftSavedAt, setDraftSavedAt] = React.useState(session.savedAt)
    const [postedLocally, setPostedLocally] = React.useState(false)
    const [leaveConfirmOpen, setLeaveConfirmOpen] = React.useState(false)
    const [pendingRemove, setPendingRemove] = React.useState<string | null>(
        null,
    )
    const idempotencyRef = React.useRef<string | null>(null)
    const baselineRef = React.useRef("")
    const [receiptApproval, setReceiptApproval] = React.useState<
        DocumentApprovalView | undefined
    >(() => (session.mode === "receipt" ? session.approval : undefined))

    const isReceipt = session.mode === "receipt"
    const existing = Boolean(session.existingFactId)

    const form = useAppForm({
        defaultValues: factDefaultValues(session.fact, isReceipt),
        validators: {
            onChange: factFormSchema,
        },
        onSubmit: async () => {
            if (isReceipt) {
                const prepared = await prepareReceiptDraft()
                if (!prepared) return
            }
            setConfirmOpen(true)
        },
    })

    // 订阅表单 store：记录金额/拟分配/校验与提交按钮随输入实时更新
    const formValues = useStore(form.store, (s) => s.values)

    const snapshot = () =>
        JSON.stringify({ values: form.state.values, allocations })

    React.useEffect(() => {
        setAllocations(session.allocations.map((a) => ({ ...a })))
        setEditVersion(session.editVersion)
        setDraftSavedAt(session.savedAt)
        setPostedLocally(false)
        setReceiptApproval(
            session.mode === "receipt" ? session.approval : undefined,
        )
        baselineRef.current = snapshot()
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [session.draftSessionId, session.editVersion, session.savedAt])

    React.useEffect(() => {
        const onBeforeUnload = (event: BeforeUnloadEvent) => {
            if (session.status === "posted" || postedLocally) return
            if (snapshot() === baselineRef.current) return
            event.preventDefault()
            event.returnValue = ""
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    })

    function requestClose() {
        if (session.status === "posted" || postedLocally) {
            onClose()
            return
        }
        if (snapshot() !== baselineRef.current) {
            setLeaveConfirmOpen(true)
            return
        }
        onClose()
    }

    // 发票 gross 变化时按 13% 预填不含税/税额（可手动覆盖）
    const invoiceGross = isReceipt
        ? ""
        : String(formValues.grossAmount ?? "")
    React.useEffect(() => {
        if (isReceipt || !invoiceGross) return
        const net = String(form.state.values.netAmount ?? "").trim()
        const tax = String(form.state.values.taxAmount ?? "").trim()
        if (net || tax) return
        const gross = Number(invoiceGross)
        if (!Number.isFinite(gross) || gross <= 0) return
        form.setFieldValue("netAmount", (gross / 1.13).toFixed(2))
        form.setFieldValue("taxAmount", (gross - gross / 1.13).toFixed(2))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [invoiceGross, isReceipt])

    const factAmountStr = isReceipt
        ? String(formValues.amount ?? "")
        : String(formValues.grossAmount ?? "")
    const proposedAllocated = allocations.reduce(
        (s, a) => s + parseAmt(a.amount),
        0,
    )
    const proposedUnallocated = Math.max(
        0,
        parseAmt(factAmountStr) - proposedAllocated,
    )

    const issues: ValidationIssue[] = []
    if (!session.leaseValid) {
        issues.push({
            id: "lease",
            label: "处理状态",
            message: "权限已收回或处理已失效，禁止提交",
        })
    }
    for (const line of allocations) {
        if (parseAmt(line.amount) < 0) {
            issues.push({
                id: `neg-${line.lineKey}`,
                label: line.label,
                message: "分配金额不能为负",
            })
        }
        if (parseAmt(line.amount) - parseAmt(line.openAmount) > 1e-9) {
            issues.push({
                id: `over-${line.lineKey}`,
                label: line.label,
                message: `拟分配不可超过开放余额 ${line.openAmount}`,
            })
        }
    }
    if (proposedAllocated - parseAmt(factAmountStr) > 1e-9) {
        issues.push({
            id: "over-fact",
            label: "拟分配合计",
            message: "拟分配合计超过记录金额",
        })
    }
    if (
        isReceipt &&
        allocations.filter((line) => parseAmt(line.amount) > 0).length === 0
    ) {
        issues.push({
            id: "need-alloc",
            label: "核销分配",
            message: "提交审批至少需要一条核销分配",
        })
    }

    const canSubmit =
        session.leaseValid &&
        issues.length === 0 &&
        parseAmt(factAmountStr) > 0 &&
        session.status === "draft"

    function addFromPool(target: AllocationTarget) {
        if (allocations.some((a) => a.targetId === target.targetId)) return
        if (target.counterpartyPartyId !== session.counterpartyPartyId) {
            setActionError("跨主体目标不能分配，请选择同主体目标。")
            return
        }
        const alreadyAllocated = allocations.reduce(
            (sum, line) => sum + parseAmt(line.amount),
            0,
        )
        const remaining = Math.max(0, parseAmt(factAmountStr) - alreadyAllocated)
        const fill =
            remaining > 0
                ? money(Math.min(parseAmt(target.openAmount), remaining))
                : ""
        setAllocations((prev) => [
            ...prev,
            {
                lineKey: `line_${target.targetId}_${Date.now()}`,
                targetId: target.targetId,
                targetKind: target.targetKind,
                label: target.label,
                salesOrderNo: target.salesOrderNo,
                openAmount: target.openAmount,
                amount: fill,
                baselineVersion: target.baselineVersion,
            },
        ])
    }

    function updateAmount(lineKey: string, amount: string) {
        setAllocations((prev) =>
            prev.map((a) => (a.lineKey === lineKey ? { ...a, amount } : a)),
        )
    }

    function removeLine(lineKey: string) {
        setAllocations((prev) => prev.filter((a) => a.lineKey !== lineKey))
    }

    function fillLineAmount(target: AllocationDraftLine) {
        const others = allocations
            .filter((a) => a.lineKey !== target.lineKey)
            .reduce((s, a) => s + parseAmt(a.amount), 0)
        const remaining = Math.max(0, parseAmt(factAmountStr) - others)
        const fill = Math.min(parseAmt(target.openAmount), remaining)
        setAllocations((prev) =>
            prev.map((a) =>
                a.lineKey === target.lineKey
                    ? { ...a, amount: money(fill) }
                    : a,
            ),
        )
    }

    async function doSaveDraft() {
        setActionError(null)
        try {
            const fact = factFromValues(form.state.values, isReceipt)
            const next = await saveMutation.mutateAsync({
                draftSessionId: session.draftSessionId,
                fact,
                allocations,
                editVersion,
            })
            setEditVersion(next.editVersion)
            setDraftSavedAt(next.savedAt)
            if (isReceipt && next.approval) setReceiptApproval(next.approval)
            baselineRef.current = snapshot()
        } catch (err) {
            setActionError(getErrorMessage(err, "保存草稿失败"))
        }
    }

    /**
     * 提交确认前先创建回款草稿，只读展示服务端绑定。
     *
     * @returns 创建或刷新成功为 true。
     */
    async function prepareReceiptDraft(): Promise<boolean> {
        setActionError(null)
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w11-submit-${session.draftSessionId}-${Date.now()}`
        }
        try {
            const fact = factFromValues(form.state.values, isReceipt)
            const saved = await saveMutation.mutateAsync({
                draftSessionId: session.draftSessionId,
                fact,
                allocations,
                editVersion,
            })
            setEditVersion(saved.editVersion)
            setDraftSavedAt(saved.savedAt)
            const ensured = await ensureReceiptMutation.mutateAsync({
                draftSessionId: session.draftSessionId,
                editVersion: saved.editVersion,
                idempotencyKey: idempotencyRef.current,
            })
            if (ensured.status !== "succeeded") {
                setActionError(ensured.message)
                return false
            }
            setReceiptApproval(ensured.session.approval)
            return true
        } catch (err) {
            setActionError(getErrorMessage(err, "创建回款草稿失败"))
            return false
        }
    }

    async function doPost() {
        setActionError(null)
        if (!idempotencyRef.current) {
            idempotencyRef.current = `w11-submit-${session.draftSessionId}-${Date.now()}`
        }
        // 先保存草稿同步服务端
        try {
            const fact = factFromValues(form.state.values, isReceipt)
            const saved = await saveMutation.mutateAsync({
                draftSessionId: session.draftSessionId,
                fact,
                allocations,
                editVersion,
            })
            setEditVersion(saved.editVersion)

            const res = await postMutation.mutateAsync({
                draftSessionId: session.draftSessionId,
                editVersion: saved.editVersion,
                idempotencyKey: idempotencyRef.current,
            })
            applyPostResult(res)
        } catch (err) {
            setActionError(getErrorMessage(err, "提交失败"))
            setConfirmOpen(false)
        }
    }

    function applyPostResult(res: PostAllocationResult) {
        if (res.status === "succeeded") {
            setPostedLocally(true)
            if (isReceipt && res.approval) setReceiptApproval(res.approval)
            const responsibility = readCustomerReceiptApprovalResponsibility(
                isReceipt ? res.approval : undefined,
            )
            setResult({
                status: "succeeded",
                title: isReceipt ? "回款已提交审批" : "销项发票已登记并分配",
                description: isReceipt
                    ? `已进入审批。单号 ${res.factNo}。全部节点通过后过账核销。`
                    : `已生效单号 ${res.factNo}。未分配余额 ${res.unallocatedAmount}。`,
                reference: res.operationId,
                facts: [
                    {
                        label: isReceipt ? "回款单号" : "发票号码",
                        value: res.factNo,
                    },
                    { label: "净已分配", value: res.allocatedTotal },
                    { label: "未分配余额", value: res.unallocatedAmount },
                    { label: "往来主体", value: session.counterpartyPartyName },
                    ...(isReceipt && responsibility.nextResponsible
                        ? [
                              {
                                  label: "当前审批人",
                                  value: responsibility.nextResponsible,
                              },
                          ]
                        : []),
                ],
                returnTo: res.returnTo,
            })
            setConfirmOpen(false)
            onPosted()
            return
        }
        if (res.status === "unknown") {
            setResult({
                status: "unknown",
                title: resultText.unknown,
                description: res.message,
                reference: res.operationId,
                pendingKey: res.idempotencyKey,
            })
            setConfirmOpen(false)
            return
        }
        if (res.code === "DUPLICATE_INVOICE") {
            setResult({
                status: "failed",
                title: "发票号码重复",
                description: res.message,
                reference: res.existingInvoiceNo,
                facts: res.existingInvoiceId
                    ? [
                          {
                              label: "已有发票",
                              value: res.existingInvoiceNo ?? "",
                          },
                      ]
                    : undefined,
            })
            setConfirmOpen(false)
            return
        }
        if (res.code === "BALANCE_CONFLICT" || res.code === "OVER_ALLOCATE") {
            setActionError(res.message)
            if (res.refreshedTargets) {
                setAllocations((prev) =>
                    prev.map((line) => {
                        const hit = res.refreshedTargets?.find(
                            (t) => t.targetId === line.targetId,
                        )
                        return hit
                            ? { ...line, openAmount: hit.openAmount }
                            : line
                    }),
                )
            }
            setConfirmOpen(false)
            return
        }
        setActionError(res.message)
        setConfirmOpen(false)
    }

    async function resolveUnknown() {
        if (!result?.pendingKey) return
        const res = await resolveMutation.mutateAsync(result.pendingKey)
        if (res) applyPostResult(res)
    }

    const locked = existing || session.status === "posted" || postedLocally

    return {
        form,
        isReceipt,
        existing,
        locked,
        receiptApproval,
        allocations,
        editVersion,
        draftSavedAt,
        postedLocally,
        result,
        actionError,
        confirmOpen,
        setConfirmOpen,
        leaveConfirmOpen,
        setLeaveConfirmOpen,
        pendingRemove,
        setPendingRemove,
        issues,
        canSubmit,
        factAmountStr,
        proposedAllocated,
        proposedUnallocated,
        addFromPool,
        updateAmount,
        removeLine,
        fillLineAmount,
        requestClose,
        doSaveDraft,
        doPost,
        resolveUnknown,
        saveMutation,
        postMutation,
        resolveMutation,
        ensureReceiptMutation,
    }
}
