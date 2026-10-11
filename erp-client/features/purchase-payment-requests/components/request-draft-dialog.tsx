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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePaymentCandidatesQuery } from "../hooks/queries"
import { useDraftForm } from "../hooks/use-draft-form"
import {
    draftSchema,
    prefillRequestAmounts,
    regroup,
    totalOf,
} from "../lib/draft"
import type { PaymentCandidate, PaymentRequest } from "../types"
import { RequestDraftGroup } from "./request-draft-group"
import { RequestOrderPicker } from "./request-order-picker"

export function RequestDraftDialog({
    open,
    onOpenChange,
    purchaseOrderId,
    request,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    purchaseOrderId?: string
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
                className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-clip p-0 sm:max-w-6xl"
            >
                {query.isPending || query.isError ? (
                    <div className="space-y-4 p-6">
                        <DialogHeader>
                            <DialogTitle>申请采购付款</DialogTitle>
                            <DialogDescription>
                                从已生效采购单中选择本次付款来源。
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
                            candidates={query.data}
                            request={request}
                            purchaseOrderId={purchaseOrderId}
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
    purchaseOrderId,
    request,
    onClose,
    onLockChange,
}: {
    candidates: PaymentCandidate[]
    purchaseOrderId?: string
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
    const [step, setStep] = useState(purchaseOrderId || request ? 2 : 1)
    const [saved, setSaved] = useState<PaymentRequest[] | null>(null)
    const { form, unknown, error, recover, pending } = useDraftForm(
        candidates,
        purchaseOrderId,
        request,
        (rows) => {
            setSaved(rows)
            onLockChange(false)
        },
    )
    const [openGroup, setOpenGroup] = useState<string | null>(
        form.state.values.groups[0]?.key ?? null,
    )
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
        if (!groups.some((group) => group.key === openGroup))
            setOpenGroup(groups[0]?.key ?? null)
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
            className="flex min-h-0 flex-col overflow-hidden"
            onSubmit={(event) => {
                event.preventDefault()
                if (step === 1) {
                    if (form.state.values.selectedIds.length) moveNext()
                    return
                }
                const values = form.state.values
                const validation = draftSchema(candidates).safeParse(values)
                if (!validation.success) {
                    const invalid = values.groups.find((group, index) =>
                        validation.error.issues.some(
                            (issue) =>
                                (issue.path[0] === "groups" &&
                                    issue.path[1] === index) ||
                                (issue.path[0] === "amounts" &&
                                    group.ids.includes(String(issue.path[1]))),
                        ),
                    )
                    if (invalid) setOpenGroup(invalid.key)
                }
                void form.handleSubmit().catch(() => undefined)
            }}
        >
            <DialogHeader className="shrink-0 border-b px-6 py-5 pr-16">
                <DialogTitle>
                    {request
                        ? "修改付款申请"
                        : step === 1
                          ? "选择本次付款的采购单"
                          : "填写付款申请"}
                </DialogTitle>
                <DialogDescription>
                    {step === 1
                        ? "支持跨供应商选择采购单，下一步按付款关系分别生成申请。"
                        : request
                          ? "修改金额与付款安排，保存后可在申请详情提交审核。"
                          : "逐份填写金额与付款安排，提交后直接生成正式申请单并进入财务审核。"}
                </DialogDescription>
                <div
                    className="mt-3 flex items-center gap-3 text-xs"
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
                    <span aria-hidden="true" className="h-px w-10 bg-border" />
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
            </DialogHeader>
            <form.Subscribe
                selector={(state) =>
                    [state.values.groups, state.errors] as const
                }
            >
                {([groups, errors]) => (
                    <>
                        <div className="min-h-0 flex-1 overflow-y-auto">
                            <fieldset
                                disabled={busy || unknown}
                                className="min-w-0 space-y-4 px-6 py-5 disabled:opacity-70"
                            >
                                {step === 1 ? (
                                    <RequestOrderPicker
                                        form={form}
                                        candidates={candidates}
                                        editing={!!request}
                                    />
                                ) : (
                                    <>
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <p className="text-sm text-muted-foreground">
                                                已选{" "}
                                                {groups.reduce(
                                                    (count, group) =>
                                                        count +
                                                        group.ids.length,
                                                    0,
                                                )}{" "}
                                                张采购单，分为 {groups.length}{" "}
                                                份申请
                                            </p>
                                            <Button
                                                id="ppr-create-reselect"
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                onClick={() => setStep(1)}
                                            >
                                                重新选择采购单
                                            </Button>
                                        </div>
                                        {!groups.length && (
                                            <p
                                                role="alert"
                                                className="rounded-lg border p-6 text-sm"
                                            >
                                                当前采购来源不可申请，请返回上一步重新选择。
                                            </p>
                                        )}
                                        {groups.map((group, index) => (
                                            <RequestDraftGroup
                                                key={group.key}
                                                form={form}
                                                group={group}
                                                index={index}
                                                candidates={candidates}
                                                editing={!!request}
                                                open={openGroup === group.key}
                                                onOpenChange={(open) =>
                                                    setOpenGroup(
                                                        open ? group.key : null,
                                                    )
                                                }
                                            />
                                        ))}

                                        {!!errors.length && (
                                            <p
                                                role="alert"
                                                className="text-sm text-destructive"
                                            >
                                                请核对各组必填项和逐单申请金额。
                                            </p>
                                        )}
                                    </>
                                )}
                            </fieldset>
                        </div>
                        <div className="shrink-0 space-y-3 border-t bg-background px-6 py-4">
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
                            <div className="flex flex-wrap items-center justify-between gap-4">
                                <form.Subscribe
                                    selector={(state) => state.values}
                                >
                                    {(values) => (
                                        <div>
                                            <p className="text-xs text-muted-foreground">
                                                {step === 1
                                                    ? `已选 ${values.selectedIds.length} 张采购单`
                                                    : request
                                                      ? "将保存申请修改"
                                                      : `将提交 ${values.groups.length} 份付款申请`}
                                            </p>
                                            {step === 2 && (
                                                <>
                                                    <MoneyValue
                                                        value={totalOf(
                                                            values.groups.flatMap(
                                                                (group) =>
                                                                    group.ids.map(
                                                                        (id) =>
                                                                            values
                                                                                .amounts[
                                                                                id
                                                                            ] ??
                                                                            "",
                                                                    ),
                                                            ),
                                                        )}
                                                        size="section"
                                                    />
                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                        {request
                                                            ? "保存后需提交财务审核"
                                                            : "提交后占用申请余额，各份申请独立审核、跟踪付款"}
                                                    </p>
                                                </>
                                            )}
                                        </div>
                                    )}
                                </form.Subscribe>
                                <div className="flex gap-2">
                                    <Button
                                        id="ppr-create-cancel"
                                        type="button"
                                        variant="outline"
                                        disabled={busy || unknown}
                                        onClick={
                                            step === 1
                                                ? onClose
                                                : () => setStep(1)
                                        }
                                    >
                                        {step === 1 ? "取消" : "上一步"}
                                    </Button>
                                    {step === 1 ? (
                                        <form.Subscribe
                                            selector={(state) =>
                                                state.values.selectedIds.length
                                            }
                                        >
                                            {(count) => (
                                                <Button
                                                    id="ppr-create-next"
                                                    type="button"
                                                    disabled={!count}
                                                    onClick={moveNext}
                                                >
                                                    下一步：填写申请
                                                </Button>
                                            )}
                                        </form.Subscribe>
                                    ) : (
                                        <Button
                                            id="ppr-create-submit"
                                            type={unknown ? "button" : "submit"}
                                            onClick={
                                                unknown
                                                    ? () => {
                                                          void recover().catch(
                                                              () => undefined,
                                                          )
                                                      }
                                                    : undefined
                                            }
                                            disabled={busy || !groups.length}
                                        >
                                            {busy
                                                ? request
                                                    ? "正在保存…"
                                                    : "正在提交…"
                                                : unknown
                                                  ? "查询本次提交结果"
                                                  : request
                                                    ? "保存申请草稿"
                                                    : `提交 ${groups.length} 份付款申请`}
                                        </Button>
                                    )}
                                </div>
                            </div>
                        </div>
                    </>
                )}
            </form.Subscribe>
        </form>
    )
}
