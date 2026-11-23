"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import { bookIdentity } from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type {
    SelectionBook,
    SelectionBookDetail,
} from "@/features/sales-selection/types"
import { getErrorMessage } from "@/lib/api/errors"

const copySchema = z.object({
    customer_id: z.string().trim().min(1, "请选择客户"),
})

/** 按原册版本复制；由调用方按原册身份挂载，每次打开重新选择客户。 */
export function BookCopyDialog({
    source,
    onClose,
    onCopied,
}: {
    source: Pick<SelectionBook, "id" | "book_id" | "version" | "customer_name">
    onClose: () => void
    onCopied: (detail: SelectionBookDetail) => void
}) {
    const { copy } = useBookOperations()
    const [sourceSnapshot] = useState(source)
    const requestKeys = useRef(new Map<string, string>())
    const form = useAppForm({
        defaultValues: { customer_id: "" },
        validators: { onChange: copySchema },
        onSubmit: async ({ value }) => {
            const { customer_id } = copySchema.parse(value)
            // 同一载荷重试必须复用原请求身份，避免响应丢失时重复创建。
            const requestKey =
                requestKeys.current.get(customer_id) ?? createIdempotencyKey()
            requestKeys.current.set(customer_id, requestKey)
            try {
                const detail = await copy.mutateAsync({
                    bookId: bookIdentity(sourceSnapshot),
                    customer_id,
                    expected_version: sourceSnapshot.version,
                    idempotency_key: requestKey,
                })
                onCopied(detail)
            } catch {
                // 失败由 mutation 状态回显；原客户和请求身份保留，供重试。
            }
        },
    })

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !copy.isPending) onClose()
            }}
        >
            <DialogContent size="md" closeButtonId="sales-selection-copy-close">
                <DialogHeader>
                    <DialogTitle>复制选品册</DialogTitle>
                    <DialogDescription>
                        复制{sourceSnapshot.customer_name}
                        的选品册，请重新选择客户。商品、套餐、价格、图片、访问密码及其他配置保持一致；新册需重新发布。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="grid gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="customer_id"
                        children={(field) => (
                            <div className="grid gap-1.5">
                                <Label htmlFor="sales-selection-copy-customer">
                                    客户
                                    <RequiredMark />
                                </Label>
                                <CustomerSearchCombobox
                                    id="sales-selection-copy-customer"
                                    value={field.state.value || undefined}
                                    onValueChange={(next) =>
                                        field.handleChange(next ?? "")
                                    }
                                    placeholder="搜索并选择客户"
                                    disabled={copy.isPending}
                                    aria-label="复制到客户"
                                />
                                {field.state.meta.isTouched &&
                                !field.state.meta.isValid ? (
                                    <p
                                        className="text-xs text-destructive"
                                        role="alert"
                                    >
                                        请选择客户
                                    </p>
                                ) : null}
                            </div>
                        )}
                    />
                    {copy.isError ? (
                        <p
                            className="text-body-compact text-destructive"
                            role="alert"
                        >
                            {getErrorMessage(
                                copy.error,
                                "复制未确认成功，请重试或刷新列表核对。",
                            )}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="sales-selection-copy-cancel"
                            type="button"
                            variant="outline"
                            disabled={copy.isPending}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-selection-copy-submit"
                                label="确认复制"
                                pendingLabel="复制中…"
                                loading={copy.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
