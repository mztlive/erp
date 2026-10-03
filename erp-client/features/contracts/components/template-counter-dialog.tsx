"use client"

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
import { getErrorMessage } from "@/lib/api/errors"
import type { ContractCounter } from "../api/templates"
import {
    useContractCountersQuery,
    useTemplateMutations,
} from "../hooks/template-queries"

function CounterForm({ counter }: { counter: ContractCounter }) {
    const { counter: mutation } = useTemplateMutations()
    const form = useAppForm({
        defaultValues: { last: String(counter.last_sequence) },
        validators: {
            onSubmit: z.object({
                last: z
                    .string()
                    .regex(/^\d{1,4}$/, "请输入 0 至 9999 的整数")
                    .refine(
                        (value) => Number(value) >= counter.last_sequence,
                        "已用流水不能向后调整",
                    ),
            }),
        },
        onSubmit: async ({ value }) => {
            try {
                await mutation.mutateAsync({
                    ...counter,
                    last_sequence: Number(value.last),
                })
            } catch {
                /* 错误直接由 mutation 渲染。 */
            }
        },
    })
    return (
        <form
            className="space-y-2 rounded-md border p-3"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <div className="flex items-end gap-3">
                <div className="min-w-0 flex-1">
                    <form.AppField name="last">
                        {(field) => (
                            <field.TextField
                                id={`template-counter-${counter.group}-last`.toLowerCase()}
                                label={`${counter.group} · ${counter.year} 年已用到第几号`}
                                inputMode="numeric"
                                disabled={mutation.isPending}
                            />
                        )}
                    </form.AppField>
                </div>
                <form.AppForm>
                    <form.SubmitButton
                        id={`template-counter-${counter.group}-save`.toLowerCase()}
                        label="保存"
                        pendingLabel="保存中…"
                    />
                </form.AppForm>
            </div>
            {mutation.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(
                        mutation.error,
                        "流水保存失败，请刷新后重试",
                    )}
                </p>
            )}
            {mutation.isSuccess && (
                <p role="status" className="text-xs text-muted-foreground">
                    已保存
                </p>
            )}
        </form>
    )
}

export function TemplateCounterDialog({ onClose }: { onClose: () => void }) {
    const query = useContractCountersQuery(true)
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent
                id="template-counter-dialog"
                closeButtonId="template-counter-dismiss"
            >
                <DialogHeader>
                    <DialogTitle>年度合同流水</DialogTitle>
                    <DialogDescription>
                        只允许向前校准。2026 年已承接 FSY 452、ZHYF 22、GYL
                        15、BDKJ 24；跨年重新从 0001 开始。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    {query.isPending && <p role="status">正在读取流水…</p>}
                    {query.isError && (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(query.error, "流水加载失败")}
                        </p>
                    )}
                    {query.data?.map((counter) => (
                        <CounterForm
                            key={`${counter.group}:${counter.version}:${counter.last_sequence}`}
                            counter={counter}
                        />
                    ))}
                </div>
                <Button
                    id="template-counter-close"
                    variant="outline"
                    onClick={onClose}
                >
                    关闭
                </Button>
            </DialogContent>
        </Dialog>
    )
}
