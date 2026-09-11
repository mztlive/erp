/**
 * 发起选品弹窗：选客户/形态/提交方式/来源 + 档位编辑。
 * 与商品池导出并列的入口，不内嵌生成器；提交走 useBookOperations().create。
 */

"use client"

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
import { Textarea } from "@/components/ui/textarea"
import { Label } from "@/components/ui/label"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    createBookSchema,
    type CreateBookFormValue,
} from "@/features/sales-selection/lib/validation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type {
    PoolFilterSnapshot,
    TierRuleInput,
} from "@/features/sales-selection/types"
import { TierEditor } from "@/features/sales-selection/components/tier-editor"

/**
 * 发起选品弹窗。
 * @param open 是否打开
 * @param onOpenChange 开关变化
 * @param initialFilterQ 商品池当前筛选关键字（来源=筛选时预填）
 * @param initialSkuIds 商品池当前勾选（来源=勾选时预填）
 * @param onCreated 创建成功回调（携带选品册身份）
 */
export const BookCreateDialog = ({
    open,
    onOpenChange,
    initialFilterQ = "",
    initialFilter,
    initialSkuIds = [],
    onCreated,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    initialFilterQ?: string
    initialFilter?: PoolFilterSnapshot
    initialSkuIds?: readonly string[]
    onCreated?: (bookId: string) => void
}) => {
    const operations = useBookOperations()
    const [skuText, setSkuText] = React.useState(() => initialSkuIds.join("\n"))

    React.useEffect(() => {
        if (open) setSkuText(initialSkuIds.join("\n"))
    }, [initialSkuIds, open])

    const form = useAppForm({
        defaultValues: {
            customer_id: "",
            selection_form: "SINGLE_SKU",
            submit_mode: "BY_QUANTITY",
            source_kind: (initialSkuIds.length > 0 ? "SELECTION" : "FILTER") as
                | "FILTER"
                | "SELECTION",
            filter_q: initialFilterQ,
            sku_ids: [...initialSkuIds],
            tiers: [] as TierRuleInput[],
        } as CreateBookFormValue,
        validators: {
            onChange: createBookSchema,
        },
        onSubmit: async ({ value }) => {
            const parsed = createBookSchema.parse(value)
            const skuIds =
                parsed.source_kind === "SELECTION"
                    ? skuText
                          .split(/[\s,，、\n]+/)
                          .map((item) => item.trim())
                          .filter(Boolean)
                    : undefined
            const result = await operations.create.mutateAsync({
                customer_id: parsed.customer_id.trim(),
                selection_form: parsed.selection_form,
                submit_mode: parsed.submit_mode,
                source_kind: parsed.source_kind,
                filter:
                    parsed.source_kind === "FILTER"
                        ? {
                              ...initialFilter,
                              q: parsed.filter_q?.trim() || undefined,
                          }
                        : undefined,
                sku_ids: skuIds,
                tiers:
                    parsed.selection_form === "PACKAGE"
                        ? (parsed.tiers ?? [])
                        : undefined,
                idempotency_key: createIdempotencyKey(),
            })
            onOpenChange(false)
            form.reset()
            onCreated?.(result.book_id ?? result.id)
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90svh] overflow-y-auto sm:max-w-2xl"
                closeButtonId="sales-selection-create-close"
            >
                <DialogHeader>
                    <DialogTitle>发起选品</DialogTitle>
                    <DialogDescription>
                        绑定一家客户并选定形态与提交方式，创建后不可更改。如需两种生意，请开两本选品册。
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
                                    客户 *
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
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
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
                        <form.AppField
                            name="submit_mode"
                            children={(field) => (
                                <field.SelectField
                                    id="sales-selection-create-mode"
                                    label="提交方式"
                                    required
                                    options={[
                                        {
                                            value: "BY_QUANTITY",
                                            label: "按份采购",
                                        },
                                        {
                                            value: "MALL_REDEEM",
                                            label: "商城兑换",
                                        },
                                    ]}
                                />
                            )}
                        />
                        <form.AppField
                            name="source_kind"
                            children={(field) => (
                                <field.SelectField
                                    id="sales-selection-create-source"
                                    label="商品来源"
                                    required
                                    options={[
                                        {
                                            value: "FILTER",
                                            label: "当前筛选",
                                        },
                                        {
                                            value: "SELECTION",
                                            label: "当前勾选",
                                        },
                                    ]}
                                />
                            )}
                        />
                    </div>
                    <form.Subscribe
                        selector={(state) => state.values.source_kind}
                        children={(sourceKind) =>
                            sourceKind === "FILTER" ? (
                                <form.AppField
                                    name="filter_q"
                                    children={(field) => (
                                        <field.TextField
                                            id="sales-selection-create-filter"
                                            label="筛选关键字"
                                            description="与商品池列表相同的筛选条件，不含分页。"
                                            placeholder="留空表示全部可售商品"
                                        />
                                    )}
                                />
                            ) : (
                                <div className="grid gap-1.5">
                                    <Label htmlFor="sales-selection-create-skus">
                                        勾选商品身份 *
                                    </Label>
                                    <Textarea
                                        id="sales-selection-create-skus"
                                        value={skuText}
                                        onChange={(event) => {
                                            setSkuText(event.target.value)
                                            const ids = event.target.value
                                                .split(/[\s,，、\n]+/)
                                                .map((item) => item.trim())
                                                .filter(Boolean)
                                            form.setFieldValue("sku_ids", ids)
                                        }}
                                        placeholder="每行一个稳定 SKU 身份"
                                        rows={4}
                                        aria-label="勾选商品身份"
                                    />
                                    <p className="text-xs text-muted-foreground">
                                        按去重后升序冻结，任一失效整次失败并列明项目。
                                    </p>
                                </div>
                            )
                        }
                    />
                    <form.Subscribe
                        selector={(state) => state.values.selection_form}
                        children={(selectionForm) =>
                            selectionForm === "PACKAGE" ? (
                                <div className="grid gap-2">
                                    <span className="text-sm font-medium">
                                        档位规则 *
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
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-selection-create-submit"
                                label={
                                    operations.create.isPending
                                        ? "创建中…"
                                        : "创建选品册"
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
