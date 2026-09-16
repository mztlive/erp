/**
 * 发起选品弹窗：选客户、形态、提交方式；套餐再填档位。
 * 商品来源由商品池当前筛选或勾选锁定；提交一次创建接口，后端排队首次准备。
 */

"use client"

import * as React from "react"
import { useAppForm } from "@/components/form"
import { OwnerCombobox } from "@/components/business"
import { Input } from "@/components/ui/input"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
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
    const ownerOptionsQuery = useOwnerOptionsQuery()

    const form = useAppForm({
        defaultValues: {
            customer_id: "",
            sales_owner_user_id: "",
            business_org_unit_id: "",
            selection_form: "SINGLE_SKU",
            submit_mode: "BY_QUANTITY",
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
                sales_owner_user_id: parsed.sales_owner_user_id.trim(),
                business_org_unit_id: parsed.business_org_unit_id.trim(),
                selection_form: parsed.selection_form,
                submit_mode: parsed.submit_mode,
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
                sales_owner_user_id: "",
                business_org_unit_id: "",
                selection_form: "SINGLE_SKU",
                submit_mode: "BY_QUANTITY",
                source_kind: sourceKind,
                sku_ids: [...initialSkuIds],
                tiers: [],
            })
        }
        wasOpen.current = open
    }, [form, initialSkuIds, open, sourceKind])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90svh] overflow-y-auto sm:max-w-2xl"
                closeButtonId="sales-selection-create-close"
            >
                <DialogHeader>
                    <DialogTitle>发起选品</DialogTitle>
                    <DialogDescription>
                        绑定一家客户并选定形态与提交方式，创建后不可更改。提交后立刻开始准备陈列，进度在选品册查看。
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
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField
                            name="sales_owner_user_id"
                            children={(field) => (
                                <div className="grid gap-1.5">
                                    <Label htmlFor="sales-selection-create-owner">
                                        负责销售 *
                                    </Label>
                                    <OwnerCombobox
                                        id="sales-selection-create-owner"
                                        owners={ownerOptionsQuery.data ?? []}
                                        loading={ownerOptionsQuery.isFetching}
                                        value={field.state.value || undefined}
                                        onValueChange={(next) =>
                                            field.handleChange(next ?? "")
                                        }
                                        placeholder="搜索负责人或工号"
                                    />
                                    {field.state.meta.isTouched &&
                                    !field.state.meta.isValid ? (
                                        <p
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            请选择负责销售
                                        </p>
                                    ) : null}
                                </div>
                            )}
                        />
                        <form.AppField
                            name="business_org_unit_id"
                            children={(field) => (
                                <div className="grid gap-1.5">
                                    <Label htmlFor="sales-selection-create-org">
                                        业务组织 *
                                    </Label>
                                    <Input
                                        id="sales-selection-create-org"
                                        value={field.state.value}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                        onBlur={field.handleBlur}
                                        placeholder="负责人有效主属组织 ID"
                                        aria-label="业务组织"
                                    />
                                    {field.state.meta.isTouched &&
                                    !field.state.meta.isValid ? (
                                        <p
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            请填写业务组织
                                        </p>
                                    ) : null}
                                </div>
                            )}
                        />
                    </div>
                    <div className="grid gap-1.5 rounded-lg border bg-muted/40 px-3 py-2">
                        <p className="text-xs text-muted-foreground">
                            商品来源
                        </p>
                        <p className="text-sm">{sourceSummary}</p>
                    </div>
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
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
                    </div>
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
                                label="创建并开始准备"
                                pendingLabel="提交中…"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
