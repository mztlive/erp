"use client"

import { useRef, useState } from "react"
import { format } from "date-fns"
import { useMutation } from "@tanstack/react-query"
import { z } from "zod"
import { useAppForm, applyApiFormError } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import { MoneyValue } from "@/components/business"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { isUncertainDepositCommandError } from "@/features/supplier-deposits/lib/command-error"
import { describeMoney } from "@/lib/money-display"
import { useDepositCommand } from "../hooks/queries"
import type { AccountDetail, RechargeRequest, DepositCommand } from "../types"

type PaymentFormValues = {
    bank: string
    paid_at: string
    bank_reference: string
    receipt: File | null
}

export function RechargePaymentDialog({
    account,
    request,
    onOpenChange,
}: {
    account: AccountDetail
    request: RechargeRequest
    onOpenChange: (open: boolean) => void
}) {
    const banks = account.payee_bank_accounts ?? []
    const mutation = useDepositCommand()
    const upload = useMutation({
        mutationFn: uploadEvidenceFileAsset,
        retry: false,
    })
    const [unknown, setUnknown] = useState(false)
    const [confirmValues, setConfirmValues] =
        useState<PaymentFormValues | null>(null)
    const frozen = useRef<DepositCommand | null>(null)

    const executePayment = async (value: PaymentFormValues) => {
        try {
            if (!frozen.current) {
                if (!value.receipt) throw new Error("请上传银行付款回单")
                const receipt = await upload.mutateAsync(value.receipt)
                frozen.current = {
                    accountId: account.account.id,
                    path: `/recharge-requests/${encodeURIComponent(request.id)}/pay`,
                    body: {
                        expected_version: request.version,
                        payee_bank_account_id: value.bank,
                        paid_at: Math.floor(Date.parse(value.paid_at) / 1000),
                        bank_reference: value.bank_reference.trim(),
                        bank_receipt_asset_id: receipt.id,
                    },
                }
            }
            await mutation.mutateAsync(frozen.current)
            toast.add({
                type: "success",
                title: "银行付款已登记，请继续核对供应商到账",
            })
            onOpenChange(false)
        } catch (error) {
            const uncertain =
                frozen.current !== null && isUncertainDepositCommandError(error)
            setUnknown(uncertain)
            if (!uncertain) {
                frozen.current = null
                setConfirmValues(null)
            }
            applyApiFormError(form, error)
        }
    }

    const form = useAppForm({
        defaultValues: {
            bank: "",
            paid_at: format(new Date(), "yyyy-MM-dd'T'HH:mm:ss"),
            bank_reference: "",
            receipt: null as File | null,
        },
        validators: {
            onSubmit: z.object({
                bank: z.string().min(1, "请选择供应商收款账户"),
                paid_at: z
                    .string()
                    .refine(
                        (value) => Number.isFinite(Date.parse(value)),
                        "请选择实际付款时间",
                    ),
                bank_reference: z
                    .string()
                    .trim()
                    .min(1, "请填写实际银行流水号"),
                receipt: z.custom<File | null>(
                    (value) => value instanceof File,
                    "请上传银行付款回单",
                ),
            }),
        },
        onSubmit: async ({ value }) => {
            if (!unknown && !confirmValues) {
                setConfirmValues({ ...value })
                return
            }
            await executePayment(value)
        },
    })

    const busy = mutation.isPending || upload.isPending
    const amountReadout = describeMoney(request.amount)?.readout
    const selectedBank = confirmValues
        ? banks.find((b) => b.id === confirmValues.bank)
        : undefined

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!busy) onOpenChange(open)
            }}
        >
            <DialogContent
                closeButtonId="supplier-deposits-payment-close"
                className="sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {confirmValues
                            ? "登记充值银行付款 · 二次核对"
                            : "登记充值银行付款"}
                    </DialogTitle>
                    <DialogDescription>
                        {confirmValues
                            ? "请核对付款金额、收款银行账户与流水号；确认后将登记为银行已付款。"
                            : "仅登记已经实际支付的充值。银行已付后仍须核对供应商到账，才增加可用余额。"}
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">
                        {account.supplier_name}
                    </span>
                    <span aria-hidden="true">·</span>
                    <span>{account.payer_party_name}</span>
                    {request.request_no ? (
                        <>
                            <span aria-hidden="true">·</span>
                            <span className="num">
                                充值单号 {request.request_no}
                            </span>
                        </>
                    ) : null}
                    <span aria-hidden="true">·</span>
                    <span className="inline-flex items-baseline gap-1">
                        <MoneyValue
                            value={request.amount}
                            display="exact"
                            className="font-semibold text-foreground"
                        />
                        {amountReadout ? (
                            <span>（{amountReadout}）</span>
                        ) : null}
                    </span>
                </div>
                {!banks.length && (
                    <Alert variant="warning">
                        <AlertDescription>
                            尚无可用的供应商收款账户，请先在供应商资料中补齐并核验后刷新账户。
                        </AlertDescription>
                    </Alert>
                )}
                {unknown && (
                    <Alert variant="warning">
                        <AlertDescription>
                            本次付款登记结果尚未查明。请先刷新核对原记录；再次提交使用原付款资料，避免重复登记。
                        </AlertDescription>
                    </Alert>
                )}

                {confirmValues ? (
                    <div className="space-y-4">
                        <Alert variant="warning">
                            <AlertTitle>请再次核对银行付款要素</AlertTitle>
                            <AlertDescription>
                                登记银行付款属于资金出账凭证记录，请确认回单与流水号准确无误。
                            </AlertDescription>
                        </Alert>
                        <dl className="grid gap-3 rounded-lg border border-border/70 bg-muted/30 p-4 text-body-compact sm:grid-cols-2">
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    付款金额
                                </dt>
                                <dd className="mt-1 space-y-0.5">
                                    <MoneyValue
                                        value={request.amount}
                                        display="exact"
                                        className="font-semibold"
                                    />
                                    {amountReadout ? (
                                        <div className="text-xs text-muted-foreground">
                                            {amountReadout}
                                        </div>
                                    ) : null}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    实际银行付款时间
                                </dt>
                                <dd className="num mt-1">
                                    {confirmValues.paid_at}
                                </dd>
                            </div>
                            <div className="sm:col-span-2">
                                <dt className="text-xs text-muted-foreground">
                                    供应商收款账户
                                </dt>
                                <dd className="mt-1">
                                    {selectedBank
                                        ? `${selectedBank.account_name} · ${selectedBank.bank_name} · ${selectedBank.account_number_masked}`
                                        : "—"}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    实际银行流水号
                                </dt>
                                <dd className="num mt-1">
                                    {confirmValues.bank_reference}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    银行付款回单
                                </dt>
                                <dd className="mt-1 break-all">
                                    {confirmValues.receipt?.name || "—"}
                                </dd>
                            </div>
                        </dl>
                        <DialogFooter>
                            <Button
                                id="supplier-deposits-payment-back"
                                type="button"
                                variant="outline"
                                disabled={busy || unknown}
                                onClick={() => setConfirmValues(null)}
                            >
                                返回修改
                            </Button>
                            <LoadingButton
                                id="supplier-deposits-payment-confirm"
                                type="button"
                                loading={busy}
                                disabled={busy}
                                onClick={() =>
                                    void executePayment(confirmValues)
                                }
                            >
                                {unknown
                                    ? "查询原付款登记结果"
                                    : "确认登记已实际付款"}
                            </LoadingButton>
                        </DialogFooter>
                    </div>
                ) : (
                    <form
                        className="flex flex-col gap-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="bank">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-deposits-payment-bank"
                                    label="供应商收款账户"
                                    required
                                    disabled={busy || unknown}
                                    options={banks.map((bank) => ({
                                        value: bank.id,
                                        label: `${bank.account_name} · ${bank.bank_name} · ${bank.account_number_masked}`,
                                    }))}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="paid_at">
                            {(field) => (
                                <field.DateTimeField
                                    id="supplier-deposits-payment-time"
                                    label="实际银行付款时间"
                                    required
                                    disabled={busy || unknown}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="bank_reference">
                            {(field) => (
                                <field.TextField
                                    id="supplier-deposits-payment-reference"
                                    label="实际银行流水号"
                                    required
                                    disabled={busy || unknown}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="receipt">
                            {(field) => (
                                <field.EvidenceUploadField
                                    id="supplier-deposits-payment-receipt"
                                    label="银行付款回单"
                                    required
                                    disabled={busy || unknown}
                                />
                            )}
                        </form.AppField>
                        <DialogFooter>
                            <Button
                                id="supplier-deposits-payment-cancel"
                                type="button"
                                variant="outline"
                                disabled={busy}
                                onClick={() => onOpenChange(false)}
                            >
                                关闭
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="supplier-deposits-payment-submit"
                                    label={
                                        unknown
                                            ? "查询原付款登记结果"
                                            : "下一步：核对付款信息"
                                    }
                                    loading={busy}
                                    disabled={!banks.length}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                )}
            </DialogContent>
        </Dialog>
    )
}
