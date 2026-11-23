/**
 * 发起选品弹窗：选客户与形态；套餐再填档位。
 * 商品来源由商品池当前筛选或勾选锁定；提交一次创建接口，后端排队首次准备。
 */

"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import * as React from "react"
import { useAppForm } from "@/components/form"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
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
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import { bookIdentity } from "@/features/sales-selection/lib/presentation"
import {
    createBookSchema,
    type CreateBookFormValue,
} from "@/features/sales-selection/lib/validation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type {
    PoolFilterSnapshot,
    PoolSourceKind,
    TierRuleInput,
} from "@/features/sales-selection/types"
import { TierEditor } from "@/features/sales-selection/components/tier-editor"

export type BookLaunchResult = {
    bookId: string
    customerName: string
    prepared: boolean
}

/**
 * 发起选品弹窗。
 * @param open 是否打开
 * @param onOpenChange 开关变化
 * @param sourceKind 由商品池勾选自动决定，弹窗内不可改
 * @param sourceSummary 只读来源说明
 * @param initialFilter 当前筛选快照（来源=筛选时提交）
 * @param initialSkuIds 当前勾选（来源=勾选时提交）
 * @param onFlyToNav 创建成功关窗后立刻投到选品册
 * @param onLaunched 创建结束回调；prepared 表示后端已排队首次准备
 */
export const BookCreateDialog = ({
    open,
    onOpenChange,
    sourceKind,
    sourceSummary,
    initialFilter,
    initialSkuIds = [],
    onFlyToNav,
    onLaunched,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    sourceKind: PoolSourceKind
    sourceSummary: string
    initialFilter?: PoolFilterSnapshot
    initialSkuIds?: readonly string[]
    onFlyToNav?: () => void
    onLaunched?: (result: BookLaunchResult) => void
}) => {
    const operations = useBookOperations()
    const form = useAppForm({
        defaultValues: {
            customer_id: "",
            selection_form: "SINGLE_SKU",
            access_password: "",
            source_kind: sourceKind,
            sku_ids: [...initialSkuIds],
            tiers: [] as TierRuleInput[],
        } as CreateBookFormValue,
        validators: {
            onChange: createBookSchema,
        },
        onSubmit: async ({ value }) => {
            const parsed = createBookSchema.parse({
                ...value,
                source_kind: sourceKind,
                sku_ids:
                    sourceKind === "SELECTION" ? [...initialSkuIds] : undefined,
            })
            const created = await operations.create.mutateAsync({
                customer_id: parsed.customer_id.trim(),
                selection_form: parsed.selection_form,
                submit_mode: "BY_QUANTITY",
                access_password: parsed.access_password,
                source_kind: sourceKind,
                filter:
                    sourceKind === "FILTER" ? { ...initialFilter } : undefined,
                sku_ids:
                    sourceKind === "SELECTION" ? [...initialSkuIds] : undefined,
                tiers:
                    parsed.selection_form === "PACKAGE"
                        ? (parsed.tiers ?? [])
                        : undefined,
                idempotency_key: createIdempotencyKey(),
                silent: true,
            })
            const bookId = bookIdentity(created)
            const customerName = created.customer_name?.trim() || "客户"
            onOpenChange(false)
            form.reset()
            onFlyToNav?.()
            onLaunched?.({
                bookId,
                customerName,
                prepared: created.status === "PREPARING",
            })
        },
    })

    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            form.reset({
                customer_id: "",
                selection_form: "SINGLE_SKU",
                access_password: "",
                source_kind: sourceKind,
                sku_ids: [...initialSkuIds],
                tiers: [],
            })
        }
        wasOpen.current = open
    }, [form, initialSkuIds, open, sourceKind])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && operations.create.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="lg"
                closeButtonId="sales-selection-create-close"
                showCloseButton={!operations.create.isPending}
            >
                <DialogHeader>
                    <DialogTitle>发起选品</DialogTitle>
                    <DialogDescription>
                        绑定一家客户并选定形态，创建后不可更改。提交后立刻开始准备陈列，进度在选品册查看。
                    </DialogDescription>
                </DialogHeader>
                <form
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                    className="flex flex-col gap-4"
                >
                    <form.AppField
                        name="customer_id"
                        children={(field) => (
                            <div className="grid gap-1.5">
                                <Label htmlFor="sales-selection-create-customer">
                                    客户
                                    <RequiredMark />
                                </Label>
                                <CustomerSearchCombobox
                                    id="sales-selection-create-customer"
                                    value={field.state.value || undefined}
                                    onValueChange={(next) =>
                                        field.handleChange(next ?? "")
                                    }
                                    placeholder="搜索并选择客户"
                                    aria-label="客户"
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
                    <form.AppField
                        name="access_password"
                        children={(field) => (
                            <field.TextField
                                id="sales-selection-create-access-password"
                                label="访问密码"
                                required
                                type="password"
                                autoComplete="new-password"
                                placeholder="设置 8 到 64 位访问密码"
                                description="访问选品册时需要填写此密码，请与选品链接分别发送。"
                            />
                        )}
                    />
                    <div className="grid gap-1.5 rounded-lg border bg-muted/40 px-3 py-2">
                        <p className="text-xs text-muted-foreground">
                            商品来源
                        </p>
                        <p className="text-body-compact">{sourceSummary}</p>
                    </div>
                    <form.AppField
                        name="selection_form"
                        children={(field) => (
                            <field.SelectField
                                id="sales-selection-create-form"
                                label="选品形态"
                                required
                                options={[
                                    {
                                        value: "SINGLE_SKU",
                                        label: "单品",
                                    },
                                    {
                                        value: "PACKAGE",
                                        label: "套餐",
                                    },
                                ]}
                            />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) => state.values.selection_form}
                        children={(selectionForm) =>
                            selectionForm === "PACKAGE" ? (
                                <div className="grid gap-2">
                                    <span className="text-body-compact font-medium">
                                        档位规则
                                        <RequiredMark />
                                    </span>
                                    <form.AppField
                                        name="tiers"
                                        children={(field) => (
                                            <TierEditor
                                                value={
                                                    (field.state.value ??
                                                        []) as TierRuleInput[]
                                                }
                                                onChange={(next) =>
                                                    field.handleChange(next)
                                                }
                                            />
                                        )}
                                    />
                                    <form.AppField
                                        name="tiers"
                                        children={(field) =>
                                            field.state.meta.isTouched &&
                                            !field.state.meta.isValid ? (
                                                <p
                                                    className="text-xs text-destructive"
                                                    role="alert"
                                                >
                                                    套餐形态需要填写 1 到 10
                                                    个档位，目标大于
                                                    0，容差不小于 0
                                                </p>
                                            ) : null
                                        }
                                    />
                                </div>
                            ) : null
                        }
                    />
                    <DialogFooter>
                        <Button
                            id="sales-selection-create-cancel"
                            type="button"
                            variant="outline"
                            disabled={operations.create.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-selection-create-submit"
                                label="创建并开始准备"
                                pendingLabel="提交中…"
                                loading={operations.create.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
