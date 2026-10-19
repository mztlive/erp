"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { useStore } from "@tanstack/react-form"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePaymentCandidatesQuery } from "../hooks/queries"
import { useDraftForm } from "../hooks/use-draft-form"
import {
    draftGroupIssues,
    prefillRequestAmounts,
    regroup,
    totalOf,
} from "../lib/draft"
import type { PaymentCandidate, PaymentRequest } from "../types"
import { RequestDraftWorkspace } from "./request-draft-workspace"
import { RequestOrderPicker } from "./request-order-picker"

export function RequestDraftDialog({
    open,
    onOpenChange,
    purchaseOrderId,
    fixedPurchaseOrderId,
    initialPayableAccountIds,
    request,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    purchaseOrderId?: string
    /** 采购详情发起时限定唯一来源，不能切回选单或添加其他采购单。 */
    fixedPurchaseOrderId?: string
    initialPayableAccountIds?: readonly string[]
    request?: PaymentRequest
}) {
    const query = usePaymentCandidatesQuery(open)
    const [locked, setLocked] = useState(false)
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!locked) onOpenChange(next)
            }}
        >
            <DialogContent
                id="ppr-create-dialog"
                closeButtonId="ppr-create-dialog-close"
                showCloseButton={!locked}
                className={cn(
                    "flex flex-col gap-0 overflow-clip p-0",
                    fixedPurchaseOrderId
                        ? "max-h-[75dvh] sm:max-w-4xl"
                        : "max-h-[94dvh] sm:max-w-6xl",
                )}
            >
                {query.isPending || query.isError ? (
                    <div className="space-y-4 p-6">
                        <DialogHeader>
                            <DialogTitle>申请采购付款</DialogTitle>
                            <DialogDescription>
                                {fixedPurchaseOrderId
                                    ? "为当前采购单填写付款金额、收款账户和付款日期。"
                                    : "从已生效采购单中选择本次付款来源。"}
                            </DialogDescription>
                        </DialogHeader>
                        {query.isPending ? (
                            <p className="py-8 text-muted-foreground">
                                正在读取可申请采购单…
                            </p>
                        ) : (
                            <>
                                <p role="alert">
                                    {getErrorMessage(
                                        query.error,
                                        "采购单读取失败",
                                    )}
                                </p>
                                <Button
                                    id="ppr-create-retry"
                                    onClick={() => void query.refetch()}
                                >
                                    重试
                                </Button>
                            </>
                        )}
                    </div>
                ) : (
                    open && (
                        <RequestDraftForm
                            candidates={
                                fixedPurchaseOrderId
                                    ? query.data.filter(
                                          (candidate) =>
                                              candidate.purchase_order_id ===
                                              fixedPurchaseOrderId,
                                      )
                                    : query.data
                            }
                            request={request}
                            purchaseOrderId={
                                fixedPurchaseOrderId ?? purchaseOrderId
                            }
                            fixedOrder={Boolean(fixedPurchaseOrderId)}
                            initialPayableAccountIds={
                                fixedPurchaseOrderId
                                    ? undefined
                                    : initialPayableAccountIds
                            }
                            onClose={() => onOpenChange(false)}
                            onLockChange={setLocked}
                        />
                    )
                )}
            </DialogContent>
        </Dialog>
    )
}

function RequestDraftForm({
    candidates: allCandidates,
    fixedOrder,
    purchaseOrderId,
    initialPayableAccountIds,
    request,
    onClose,
    onLockChange,
}: {
    candidates: PaymentCandidate[]
    fixedOrder: boolean
    purchaseOrderId?: string
    initialPayableAccountIds?: readonly string[]
    request?: PaymentRequest
    onClose: () => void
    onLockChange: (value: boolean) => void
}) {
    const candidates = request
        ? allCandidates.filter(
              (item) =>
                  item.supplier_id === request.supplier_id &&
                  item.our_company_id === request.our_company_id,
          )
        : allCandidates
    const [step, setStep] = useState(
        purchaseOrderId || request || initialPayableAccountIds?.length ? 2 : 1,
    )
    const [saved, setSaved] = useState<PaymentRequest[] | null>(null)
    const { form, unknown, error, recover, pending } = useDraftForm(
        candidates,
        purchaseOrderId,
        request,
        (rows) => {
            setSaved(rows)
            onLockChange(false)
        },
        initialPayableAccountIds,
    )
    const [activeKey, setActiveKey] = useState<string | undefined>(() => {
        const values = form.state.values
        return (
            draftGroupIssues(values, candidates).keys().next().value ??
            values.groups[0]?.key
        )
    })
    const selectGroup = (key: string) => {
        setActiveKey(key)
        requestAnimationFrame(() => {
            document.getElementById("ppr-create-editor")?.scrollTo({ top: 0 })
            document
                .getElementById(`ppr-group-${toAutomationIdSegment(key)}-title`)
                ?.focus({ preventScroll: true })
        })
    }
    const submitting = useStore(form.store, (state) => state.isSubmitting)
    const busy = submitting || pending
    useEffect(() => {
        onLockChange(busy || unknown)
    }, [busy, unknown, onLockChange])
    useEffect(() => () => onLockChange(false), [onLockChange])
    const moveNext = () => {
        const values = form.state.values
        const groups = regroup(values.selectedIds, candidates, values.groups)
        form.setFieldValue("groups", groups)
        form.setFieldValue(
            "amounts",
            prefillRequestAmounts(
                values.selectedIds,
                candidates,
                values.amounts,
            ),
        )
        const nextValues = form.state.values
        setActiveKey(
            draftGroupIssues(nextValues, candidates).keys().next().value ??
                groups[0]?.key,
        )
        setStep(2)
    }
    if (saved)
        return (
            <>
                <DialogHeader className="border-b p-6">
                    <DialogTitle>
                        {request
                            ? "已保存申请修改"
                            : `已提交 ${saved.length} 份付款申请`}
                    </DialogTitle>
                    <DialogDescription>
                        {request
                            ? "修改已保存，请打开申请核对后提交财务审核。"
                            : fixedOrder
                              ? "本采购单的付款申请已提交财务审核，关联申请记录已更新。"
                              : "付款申请已提交财务审核，并占用相应采购余额。可打开申请查看审核及付款进度。"}
                    </DialogDescription>
                </DialogHeader>
                <div
                    id="ppr-create-results"
                    className="min-h-0 space-y-3 overflow-y-auto p-6"
                >
                    {saved.map((row) => (
                        <div
                            key={row.id}
                            className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-4"
                        >
                            <div>
                                <p className="num font-medium">
                                    {row.request_no}
                                </p>
                                <p className="mt-1 text-sm text-muted-foreground">
                                    {row.supplier_name} · {row.our_company_name}
                                </p>
                                <p className="mt-2 text-sm">
                                    {row.lines.length} 张采购单 ·{" "}
                                    <MoneyValue value={row.total_amount} />
                                </p>
                            </div>
                            {!fixedOrder ? (
                                <Button
                                    id={`ppr-result-${toAutomationIdSegment(row.id)}`}
                                    variant="outline"
                                    render={
                                        <Link
                                            href={`/procurement/payment-requests/${encodeURIComponent(row.id)}`}
                                        />
                                    }
                                    onClick={onClose}
                                >
                                    打开申请
                                </Button>
                            ) : null}
                        </div>
                    ))}
                </div>
                <div className="flex justify-end border-t p-4">
                    <Button id="ppr-create-done" onClick={onClose}>
                        完成
                    </Button>
                </div>
            </>
        )
    return (
        <form
            id="ppr-create-form"
            noValidate
            className={cn(
                "flex min-h-0 flex-col overflow-hidden",
                fixedOrder ? "h-[75dvh]" : "h-[94dvh]",
            )}
            onSubmit={(event) => {
                event.preventDefault()
                if (busy || unknown) return
                if (step === 1) {
                    if (form.state.values.selectedIds.length) moveNext()
                    return
                }
                const invalid = draftGroupIssues(form.state.values, candidates)
                    .keys()
                    .next().value
                if (invalid) {
                    selectGroup(invalid)
                    return
                }
                void form.handleSubmit().catch(() => undefined)
            }}
        >
            <DialogHeader className="shrink-0 border-b px-6 py-4 pr-16">
                <DialogTitle>
                    {request
                        ? "修改付款申请"
                        : step === 1
                          ? "选择本次付款的采购单"
                          : fixedOrder
                            ? "申请采购付款"
                            : "批量申请付款"}
                </DialogTitle>
                <DialogDescription>
                    {step === 1
                        ? "支持跨供应商选择采购单，下一步按付款关系分别生成申请。"
                        : request
                          ? "修改金额与付款安排，保存后可在申请详情提交审核。"
                          : fixedOrder
                            ? "仅为当前采购单申请付款，请核对金额、收款账户和付款日期。"
                            : "请逐份核对金额、收款账户和付款日期，提交后分别进入财务审核。"}
                </DialogDescription>
                {!fixedOrder ? (
                    <div
                        className="mt-2 flex items-center gap-3 text-xs"
                        aria-label={`当前第 ${step} 步`}
                    >
                        <span
                            className={
                                step === 1
                                    ? "font-medium text-foreground"
                                    : "text-muted-foreground"
                            }
                        >
                            1 选择采购单
                        </span>
                        <span
                            aria-hidden="true"
                            className="h-px w-10 bg-border"
                        />
                        <span
                            className={
                                step === 2
                                    ? "font-medium text-foreground"
                                    : "text-muted-foreground"
                            }
                        >
                            2 填写申请
                        </span>
                    </div>
                ) : null}
            </DialogHeader>
            <form.Subscribe
                selector={(state) => [state.values, state.errors] as const}
            >
                {([values, errors]) => {
                    const { groups } = values
                    const issues = draftGroupIssues(values, candidates)
                    return (
                        <>
                            <fieldset
                                disabled={busy || unknown}
                                className="flex min-h-0 min-w-0 flex-1 flex-col disabled:opacity-70"
                            >
                                <legend className="sr-only">
                                    {step === 1 ? "选择采购单" : "填写付款申请"}
                                </legend>
                                {step === 1 ? (
                                    <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                                        <RequestOrderPicker
                                            form={form}
                                            candidates={candidates}
                                            editing={!!request}
                                        />
                                    </div>
                                ) : (
                                    <RequestDraftWorkspace
                                        form={form}
                                        values={values}
                                        candidates={candidates}
                                        editing={!!request}
                                        fixedOrder={fixedOrder}
                                        activeKey={activeKey}
                                        issues={issues}
                                        onSelect={selectGroup}
                                    />
                                )}
                            </fieldset>
                            <div className="shrink-0 space-y-3 border-t bg-background px-6 py-4">
                                {step === 2 && !!errors.length && (
                                    <p
                                        role="alert"
                                        className="text-sm text-destructive"
                                    >
                                        {fixedOrder
                                            ? "请核对必填项和申请金额；当前采购单不可申请时，请关闭弹窗后刷新详情。"
                                            : "请核对必填项和逐单申请金额；采购来源不可用或超过 50 份时，请返回上一步调整。"}
                                    </p>
                                )}
                                {error && (
                                    <p
                                        role="alert"
                                        className="text-sm text-destructive"
                                    >
                                        {getErrorMessage(
                                            error,
                                            request
                                                ? "保存失败，请核对后重试"
                                                : "提交失败，请核对后重试",
                                        )}
                                    </p>
                                )}
                                {unknown && (
                                    <p role="alert" className="text-sm">
                                        提交结果暂无法确认。请点击“查询本次提交结果”，系统将恢复原次结果，不会重复生成申请或占用余额。
                                    </p>
                                )}
                                <div className="flex flex-wrap items-center justify-between gap-3">
                                    <div>
                                        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                                            <p className="text-xs text-muted-foreground">
                                                {step === 1
                                                    ? `已选 ${values.selectedIds.length} 张采购单`
                                                    : "已填申请金额合计"}
                                            </p>
                                            {step === 2 && (
                                                <MoneyValue
                                                    value={totalOf(
                                                        groups.flatMap(
                                                            (group) =>
                                                                group.ids.map(
                                                                    (id) =>
                                                                        values
                                                                            .amounts[
                                                                            id
                                                                        ] ?? "",
                                                                ),
                                                        ),
                                                    )}
                                                    size="section"
                                                />
                                            )}
                                        </div>
                                        {step === 2 && (
                                            <p
                                                role="status"
                                                className="mt-1 text-xs text-muted-foreground"
                                            >
                                                {!groups.length
                                                    ? fixedOrder
                                                        ? "当前采购单暂无可申请付款的来源"
                                                        : "请返回上一步选择采购单"
                                                    : issues.size
                                                      ? `${groups.length} 份申请中，${issues.size} 份待补充`
                                                      : request
                                                        ? "填写完整，保存后需提交财务审核"
                                                        : `共 ${groups.length} 份申请，已填完整`}
                                            </p>
                                        )}
                                    </div>
                                    <div className="flex flex-wrap gap-2">
                                        <Button
                                            id="ppr-create-cancel"
                                            type="button"
                                            variant="outline"
                                            disabled={busy || unknown}
                                            onClick={
                                                step === 1 || fixedOrder
                                                    ? onClose
                                                    : () => setStep(1)
                                            }
                                        >
                                            {step === 1 || fixedOrder
                                                ? "取消"
                                                : "上一步"}
                                        </Button>
                                        {step === 1 ? (
                                            <Button
                                                id="ppr-create-next"
                                                type="button"
                                                disabled={
                                                    busy ||
                                                    unknown ||
                                                    !values.selectedIds.length
                                                }
                                                onClick={moveNext}
                                            >
                                                下一步：填写申请
                                            </Button>
                                        ) : (
                                            <Button
                                                id="ppr-create-submit"
                                                type={
                                                    unknown
                                                        ? "button"
                                                        : "submit"
                                                }
                                                onClick={
                                                    unknown
                                                        ? () => {
                                                              void recover().catch(
                                                                  () =>
                                                                      undefined,
                                                              )
                                                          }
                                                        : undefined
                                                }
                                                disabled={
                                                    busy || !groups.length
                                                }
                                            >
                                                {busy
                                                    ? request
                                                        ? "正在保存…"
                                                        : "正在提交…"
                                                    : unknown
                                                      ? "查询本次提交结果"
                                                      : issues.size
                                                        ? `继续填写（剩 ${issues.size} 份）`
                                                        : request
                                                          ? "保存申请草稿"
                                                          : fixedOrder
                                                            ? "提交付款申请"
                                                            : `提交 ${groups.length} 份付款申请`}
                                            </Button>
                                        )}
                                    </div>
                                </div>
                            </div>
                        </>
                    )
                }}
            </form.Subscribe>
        </form>
    )
}
