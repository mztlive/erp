"use client"

import { useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { ApiErrorException, getErrorMessage } from "@/lib/api/errors"
import type {
    ApplyContractInput,
    ContractApplication,
    ContractTemplate,
} from "../api/templates"
import { useTemplateMutations } from "../hooks/template-queries"

export type PendingContractApplication = {
    template: ContractTemplate
    input: ApplyContractInput
}
export const pendingContractKey = (accountId: string) =>
    `contract-application-pending:${accountId}`

export function readPendingContract(
    accountId: string,
): PendingContractApplication | null {
    try {
        const raw = sessionStorage.getItem(pendingContractKey(accountId))
        const parsed = raw
            ? (JSON.parse(raw) as PendingContractApplication)
            : null
        return parsed?.template?.id &&
            parsed?.input?.command_id &&
            parsed.input.template_id === parsed.template.id
            ? parsed
            : null
    } catch {
        return null
    }
}

export function TemplateApplyDialog({
    template,
    pending,
    accountId,
    onClose,
}: {
    template: ContractTemplate
    pending?: PendingContractApplication | null
    accountId: string
    onClose: () => void
}) {
    const mutations = useTemplateMutations()
    const [commandId] = useState(
        () => pending?.input.command_id ?? crypto.randomUUID(),
    )
    const [uncertain, setUncertain] = useState(Boolean(pending))
    const [created, setCreated] = useState<ContractApplication | null>(null)
    const [error, setError] = useState("")
    const form = useAppForm({
        defaultValues: { purpose: pending?.input.purpose ?? "" },
        validators: {
            onSubmit: z.object({
                purpose: z.string().trim().max(256, "申请用途不超过 256 字"),
            }),
        },
        onSubmit: async ({ value }) => {
            setError("")
            const input = {
                command_id: commandId,
                template_id: template.id,
                purpose: value.purpose.trim(),
            }
            try {
                sessionStorage.setItem(
                    pendingContractKey(accountId),
                    JSON.stringify({ template, input }),
                )
            } catch {
                /* 当前弹窗仍保留同一申请键。 */
            }
            let result: ContractApplication
            try {
                result = await mutations.apply.mutateAsync(input)
            } catch (cause) {
                const unknown = !(
                    cause instanceof ApiErrorException &&
                    typeof cause.status === "number" &&
                    cause.status >= 400 &&
                    cause.status < 500 &&
                    cause.status !== 408 &&
                    cause.status !== 409
                )
                setUncertain(unknown)
                if (!unknown) {
                    try {
                        sessionStorage.removeItem(pendingContractKey(accountId))
                    } catch {
                        /* 保留当前字段。 */
                    }
                }
                setError(
                    getErrorMessage(cause, "申请失败，请重试") +
                        (unknown
                            ? "。结果暂未确认，请保持本次申请内容重试核对。"
                            : ""),
                )
                return
            }
            setCreated(result)
            setUncertain(false)
            try {
                sessionStorage.removeItem(pendingContractKey(accountId))
            } catch {
                /* 重新进入仍可按同一键核对。 */
            }
            try {
                await mutations.download.mutateAsync({
                    id: result.id,
                    filename: `${result.contract_no}.docx`,
                })
            } catch (cause) {
                setError(
                    getErrorMessage(
                        cause,
                        "编号已申请，Word 下载失败。请点击重新下载",
                    ),
                )
            }
        },
    })
    const busy = mutations.apply.isPending || mutations.download.isPending
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !busy) onClose()
            }}
        >
            <DialogContent
                id="template-apply-dialog"
                closeButtonId="template-apply-dismiss"
            >
                <DialogHeader>
                    <DialogTitle>
                        {created ? "合同编号已申请" : "申请合同编号"}
                    </DialogTitle>
                    <DialogDescription>
                        {template.name} · {template.company_name}
                    </DialogDescription>
                </DialogHeader>
                {created ? (
                    <div className="space-y-4">
                        <p className="num text-xl font-semibold">
                            {created.contract_no}
                        </p>
                        <p className="text-sm text-muted-foreground">
                            编号已写入 Word 第一页右上角。下载后用 Word
                            打开并打印；重新下载使用同一编号。
                        </p>
                        <div className="flex justify-end gap-2">
                            <Button
                                id="template-apply-done"
                                variant="outline"
                                disabled={busy}
                                onClick={onClose}
                            >
                                完成
                            </Button>
                            <Button
                                id="template-apply-redownload"
                                disabled={busy}
                                onClick={() => {
                                    setError("")
                                    mutations.download.mutate(
                                        {
                                            id: created.id,
                                            filename: `${created.contract_no}.docx`,
                                        },
                                        {
                                            onError: (cause) =>
                                                setError(
                                                    getErrorMessage(
                                                        cause,
                                                        "Word 下载失败，请重试",
                                                    ),
                                                ),
                                        },
                                    )
                                }}
                            >
                                重新下载 Word
                            </Button>
                        </div>
                    </div>
                ) : (
                    <form
                        className="space-y-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <p className="text-sm text-muted-foreground">
                            使用 {template.group}{" "}
                            销售合同编号。同一编号组的公司共用年度流水，新申请后号码不回收。
                        </p>
                        <form.AppField name="purpose">
                            {(field) => (
                                <field.TextField
                                    id="template-apply-purpose"
                                    label="申请用途（选填）"
                                    placeholder="例如：某客户春节福利采购"
                                    disabled={busy || uncertain}
                                />
                            )}
                        </form.AppField>
                        <div className="flex justify-end gap-2">
                            <Button
                                id="template-apply-cancel"
                                type="button"
                                variant="outline"
                                disabled={busy}
                                onClick={onClose}
                            >
                                {uncertain ? "稍后核对" : "取消"}
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="template-apply-submit"
                                    label={
                                        uncertain
                                            ? "核对本次申请并下载"
                                            : "申请并下载 Word"
                                    }
                                    pendingLabel="正在申请…"
                                />
                            </form.AppForm>
                        </div>
                    </form>
                )}
                {error && (
                    <p role="alert" className="text-sm text-destructive">
                        {error}
                    </p>
                )}
            </DialogContent>
        </Dialog>
    )
}
