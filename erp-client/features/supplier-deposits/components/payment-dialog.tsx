"use client"

import { useRef, useState } from "react"
import { format } from "date-fns"
import { useMutation } from "@tanstack/react-query"
import { z } from "zod"
import { useAppForm, applyApiFormError } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { MoneyValue } from "@/components/business"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { isApiError } from "@/lib/api/errors"
import { useDepositCommand } from "../hooks/queries"
import type { AccountDetail, RechargeRequest, DepositCommand } from "../types"

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
    const frozen = useRef<DepositCommand | null>(null)
    const form = useAppForm({
        defaultValues: {
            bank: "",
            paid_at: format(new Date(), "yyyy-MM-dd'T'HH:mm"),
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
                            paid_at: Math.floor(
                                Date.parse(value.paid_at) / 1000,
                            ),
                            bank_reference: value.bank_reference.trim(),
                            bank_receipt_asset_id: receipt.id,
                        },
                    }
                }
                await mutation.mutateAsync(frozen.current)
                onOpenChange(false)
            } catch (error) {
                const uncertain =
                    frozen.current !== null &&
                    (!isApiError(error) ||
                        ["Network", "Parse", "Unknown"].includes(error.kind) ||
                        (error.status ?? 0) >= 500)
                setUnknown(uncertain)
                if (!uncertain) frozen.current = null
                applyApiFormError(form, error)
            }
        },
    })
    const busy = mutation.isPending || upload.isPending
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
                    <DialogTitle>登记充值银行付款</DialogTitle>
                    <DialogDescription>
                        仅登记已经实际支付的充值。银行已付后仍须核对供应商到账，才增加可用余额。
                    </DialogDescription>
                </DialogHeader>
                <p className="text-sm">
                    {account.supplier_name} · {account.payer_party_name} ·{" "}
                    <MoneyValue value={request.amount} display="exact" />
                </p>
                {!banks.length && (
                    <Alert>
                        <AlertDescription>
                            尚无可用的供应商收款账户，请先在供应商资料中补齐并核验后刷新账户。
                        </AlertDescription>
                    </Alert>
                )}
                {unknown && (
                    <Alert>
                        <AlertDescription>
                            本次付款登记结果尚未查明。请先刷新核对原记录；再次提交使用原付款资料，避免重复登记。
                        </AlertDescription>
                    </Alert>
                )}
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
                            <field.TextField
                                id="supplier-deposits-payment-time"
                                type="datetime-local"
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
                                        : "登记已实际付款"
                                }
                                loading={busy}
                                disabled={!banks.length}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
