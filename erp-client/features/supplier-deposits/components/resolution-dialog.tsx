"use client"

import { useRef, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { format } from "date-fns"
import { z } from "zod"
import { MoneyValue } from "@/components/business"
import { applyApiFormError, useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { isUncertainDepositCommandError } from "@/features/supplier-deposits/lib/command-error"
import { compareDecimal, formatCurrencyFixed } from "@/lib/fixed-decimal"
import { useDepositCommand } from "../hooks/queries"
import {
    balancedReconciliations,
    reservationRemainder,
} from "../lib/resolution"
import type {
    AccountDetail,
    DepositCommand,
    DepositReconciliation,
    DepositReservation,
} from "../types"

export type DepositResolutionTarget =
    | { action: "resolve"; reconciliation: DepositReconciliation }
    | { action: "release"; reservation: DepositReservation }

const releaseReasons = [
    { value: "charges_settled", label: "全部费用已结清，剩余占用可释放" },
    {
        value: "failed_without_charge",
        label: "下单明确失败，已核实无扣款和冻结",
    },
    {
        value: "cancelled_and_returned",
        label: "订单已取消，原扣款已全部返还并冲回",
    },
] as const

function reconciliationLabel(record: DepositReconciliation) {
    const time = format(
        new Date(record.covered_until * 1000),
        "yyyy-MM-dd HH:mm",
    )
    return `截至 ${time} · 余额 ${formatCurrencyFixed(record.remote_balance, { maxScale: 2, minimumFractionDigits: 2 })} · ${record.evidence_reference}`
}

/** 在原账户记录旁提交核实依据；资料和金额资格由服务端再次核验。 */
export function DepositResolutionDialog({
    account,
    target,
    onOpenChange,
}: {
    account: AccountDetail
    target: DepositResolutionTarget
    onOpenChange: (open: boolean) => void
}) {
    const resolving = target.action === "resolve"
    const restoring =
        resolving &&
        !target.reconciliation.unresolved_transaction_ids.length &&
        compareDecimal(
            target.reconciliation.remote_balance,
            target.reconciliation.local_balance_at_cutoff,
            2,
        ) === 0
    const candidates = balancedReconciliations(
        account.reconciliations,
        resolving ? target.reconciliation : undefined,
    )
    const mutation = useDepositCommand()
    const upload = useMutation({
        mutationFn: uploadEvidenceFileAsset,
        retry: false,
    })
    const [operation] = useState(() => crypto.randomUUID())
    const [unknown, setUnknown] = useState(false)
    const frozen = useRef<DepositCommand | null>(null)
    const form = useAppForm({
        defaultValues: {
            reconciliation: "",
            reason: "",
            evidence: null as File | null,
        },
        validators: {
            onSubmit: z.object({
                reconciliation: z
                    .string()
                    .refine(
                        (value) =>
                            candidates.some((record) => record.id === value),
                        "请选择双方余额一致的对账记录",
                    ),
                reason: z
                    .string()
                    .refine(
                        (value) =>
                            resolving ||
                            releaseReasons.some(
                                (reason) => reason.value === value,
                            ),
                        "请选择已核实的释放原因",
                    ),
                evidence: z.custom<File | null>(
                    (value) => value instanceof File,
                    "请上传本次核实依据",
                ),
            }),
        },
        onSubmit: async ({ value }) => {
            try {
                if (!frozen.current) {
                    if (!value.evidence) throw new Error("请上传本次核实依据")
                    const evidence = await upload.mutateAsync(value.evidence)
                    frozen.current = {
                        accountId: account.account.id,
                        path: resolving
                            ? `/reconciliations/${encodeURIComponent(target.reconciliation.id)}/resolve`
                            : `/reservations/${encodeURIComponent(target.reservation.id)}/release`,
                        body: {
                            operation_key: operation,
                            expected_version: resolving
                                ? target.reconciliation.version
                                : target.reservation.version,
                            evidence_asset_id: evidence.id,
                            ...(resolving
                                ? {
                                      balanced_reconciliation_id:
                                          value.reconciliation,
                                  }
                                : {
                                      reconciliation_id: value.reconciliation,
                                      reason: value.reason,
                                  }),
                        },
                    }
                }
                await mutation.mutateAsync(frozen.current)
                onOpenChange(false)
            } catch (error) {
                const uncertain =
                    frozen.current !== null &&
                    isUncertainDepositCommandError(error)
                setUnknown(uncertain)
                if (!uncertain) frozen.current = null
                applyApiFormError(form, error)
            }
        },
    })
    const busy = mutation.isPending || upload.isPending
    const prefix = `supplier-deposits-${target.action}`
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!busy) onOpenChange(open)
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-close`}
                className="sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {resolving
                            ? restoring
                                ? "核实后恢复账户使用"
                                : "核实并结清对账差异"
                            : "核实并释放剩余占用"}
                    </DialogTitle>
                    <DialogDescription>
                        {resolving
                            ? "请提交覆盖原核对范围且双方余额一致的对账记录与核实附件；系统核对无待处理扣款后，将自动恢复账户使用。"
                            : "须先查清原订单、真实扣款、冻结及后续费用。核实后将释放剩余冻结金额，并同步调整对应付款申请额度。"}
                    </DialogDescription>
                </DialogHeader>
                <p className="text-body-compact">
                    {account.supplier_name} · {account.payer_party_name}
                </p>
                {resolving ? (
                    <div className="space-y-1 text-body-compact">
                        <p>
                            原对账范围：
                            {format(
                                new Date(
                                    target.reconciliation.covered_from * 1000,
                                ),
                                "yyyy-MM-dd HH:mm",
                            )}
                            {" 至 "}
                            {format(
                                new Date(
                                    target.reconciliation.covered_until * 1000,
                                ),
                                "yyyy-MM-dd HH:mm",
                            )}
                        </p>
                        <p className="text-muted-foreground">
                            原核对依据：
                            {target.reconciliation.evidence_reference}
                        </p>
                    </div>
                ) : (
                    <div className="space-y-1 text-body-compact">
                        <p>
                            原占用{" "}
                            <MoneyValue
                                value={target.reservation.amount}
                                display="exact"
                            />
                            {" · 已消耗 "}
                            <MoneyValue
                                value={target.reservation.consumed_amount}
                                display="exact"
                            />
                        </p>
                        <p>
                            当前剩余占用：
                            <MoneyValue
                                value={reservationRemainder(target.reservation)}
                                display="exact"
                            />
                        </p>
                    </div>
                )}
                {!candidates.length && (
                    <Alert>
                        <AlertDescription>
                            暂无符合范围的双方余额一致的对账记录。请先关闭窗口，登记覆盖原业务期间且无待核实交易的最新对账，再刷新账户办理。
                        </AlertDescription>
                    </Alert>
                )}
                {unknown && (
                    <Alert>
                        <AlertDescription>
                            本次办理结果尚未查明，资料已锁定。再次提交将查询或恢复原办理结果；也可关闭窗口后刷新账户核对。
                        </AlertDescription>
                    </Alert>
                )}
                <form
                    className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="reconciliation">
                        {(field) => (
                            <field.SelectField
                                id={`${prefix}-reconciliation`}
                                label="双方余额一致的对账记录"
                                required
                                disabled={busy || unknown || !candidates.length}
                                options={candidates.map((record) => ({
                                    value: record.id,
                                    label: reconciliationLabel(record),
                                }))}
                                description="仅列出双方余额一致且无未核实交易的记录，服务器仍会复核最新资金及原订单。"
                            />
                        )}
                    </form.AppField>
                    {!resolving && (
                        <form.AppField name="reason">
                            {(field) => (
                                <field.SelectField
                                    id={`${prefix}-reason`}
                                    label="已核实的释放原因"
                                    required
                                    disabled={busy || unknown}
                                    options={releaseReasons}
                                />
                            )}
                        </form.AppField>
                    )}
                    <form.AppField name="evidence">
                        {(field) => (
                            <field.EvidenceUploadField
                                id={`${prefix}-evidence`}
                                label={
                                    resolving
                                        ? "差异查清依据"
                                        : "无扣款风险核实凭证"
                                }
                                description={
                                    resolving
                                        ? "上传原差异逐项查清的核实材料，支持 PDF 或图片，单个文件不超过 5 MB。"
                                        : "上传无冻结、无后续扣款风险及所选原因的核实材料。取消受理或某次查不到订单不能单独作为释放依据。"
                                }
                                required
                                disabled={busy || unknown}
                            />
                        )}
                    </form.AppField>
                    <DialogFooter>
                        <Button
                            id={`${prefix}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={busy}
                            onClick={() => onOpenChange(false)}
                        >
                            关闭
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id={`${prefix}-submit`}
                                label={
                                    unknown
                                        ? "查询原办理结果"
                                        : resolving
                                          ? "确认核实并结案"
                                          : "确认核实并释放"
                                }
                                loading={busy}
                                disabled={!candidates.length}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
