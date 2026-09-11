"use client"

import * as React from "react"

import {
    CategoryCombobox,
    DiscardConfirmDialog,
    FormalActionResult,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { getErrorMessage } from "@/lib/api/errors"
import { Label } from "@/components/ui/label"
import type { CategoryFormValues } from "@/features/master-data/components/category/category-form-schema"
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import {
    buildCategoryForest,
    collectDescendantIds,
    flattenCategoryForest,
    toCategoryComboboxItems,
} from "@/features/master-data/lib/category-tree-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataMutationResult } from "@/features/master-data/types"

const PRODUCT_KIND_OPTIONS = ["实物", "虚拟", "服务", "卡券"] as const

/** 新建 / 更新分类共用弹窗骨架：表单与阻断结果。 */
export function CategoryFormDialogFrame({
    open,
    onOpenChange,
    title,
    description,
    form,
    result,
    error,
    pending,
    submitLabel,
    excludeStableId,
    onReset,
    idPrefix,
    mode = "create",
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    title: string
    description: React.ReactNode
    form: {
        AppField: React.ComponentType<{
            name: keyof CategoryFormValues
            children: (field: {
                TextField: React.ComponentType<{
                    label: string
                    required?: boolean
                    id?: string
                }>
                TextareaField: React.ComponentType<{
                    label: string
                    required?: boolean
                    id?: string
                }>
                SelectField: React.ComponentType<{
                    label: string
                    options: readonly { value: string; label: string }[]
                    allowClear?: boolean
                    placeholder?: string
                    required?: boolean
                    id?: string
                }>
                handleChange: (value: string) => void
                state: { value: string }
            }) => React.ReactNode
        }>
        AppForm: React.ComponentType<{ children: React.ReactNode }>
        SubmitButton: React.ComponentType<{
            id?: string
            label?: string
            pendingLabel?: string
            disabled?: boolean
        }>
        state: { isDirty: boolean }
        handleSubmit: () => unknown
    }
    error?: unknown
    result: MasterDataMutationResult | null
    pending: boolean
    submitLabel: string
    excludeStableId?: string
    onReset?: () => void
    idPrefix?: string
    mode?: "create" | "edit" | "move"
}) {
    const prefix = idPrefix ?? "master-data-category-form-dialog"
    const categoryListQuery = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "all",
        revisionTiming: "all",
    })
    const excludeCategoryIds = React.useMemo(() => {
        if (!excludeStableId) return undefined
        const forest = buildCategoryForest(categoryListQuery.data?.rows ?? [])
        return collectDescendantIds(forest, excludeStableId)
    }, [categoryListQuery.data?.rows, excludeStableId])
    const categoryParentOptions = React.useMemo(
        () =>
            toCategoryComboboxItems(categoryListQuery.data?.rows ?? [], {
                excludeIds: excludeCategoryIds,
                enabledOnly: false,
            }),
        [categoryListQuery.data?.rows, excludeCategoryIds],
    )

    const currentNode = flattenCategoryForest(
        buildCategoryForest(categoryListQuery.data?.rows ?? []),
    ).find((node) => node.item.stableId === excludeStableId)
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const close = () => {
        onReset?.()
        onOpenChange(false)
    }
    React.useEffect(() => {
        if (!open) return
        const beforeUnload = (event: BeforeUnloadEvent) => {
            if (pending || form.state.isDirty) event.preventDefault()
        }
        window.addEventListener("beforeunload", beforeUnload)
        return () => window.removeEventListener("beforeunload", beforeUnload)
    }, [open, pending, form])

    return (
        <>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (pending) return
                    if (!next && form.state.isDirty) {
                        setDiscardOpen(true)
                        return
                    }
                    if (!next) close()
                    else onOpenChange(true)
                }}
            >
                <DialogContent
                    className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg"
                    closeButtonId={`${prefix}-close`}
                >
                    <DialogHeader>
                        <DialogTitle>{title}</DialogTitle>
                        {description ? (
                            <DialogDescription>{description}</DialogDescription>
                        ) : null}
                    </DialogHeader>
                    <DialogScrollBody>
                        {result?.outcome === "blocked" ||
                        result?.outcome === "conflict" ? (
                            <FormalActionResult
                                status="blocked"
                                title={
                                    result.outcome === "conflict"
                                        ? "分类已更新，请重新打开后修改"
                                        : masterDataCopy.createBlockedTitle
                                }
                                description={result.message}
                            />
                        ) : null}
                        {error ? (
                            <FormalActionResult
                                status="unknown"
                                title="暂未确认保存结果"
                                description={getErrorMessage(
                                    error,
                                    "请保留当前输入，核对分类资料后再重试。",
                                )}
                            />
                        ) : null}
                        {result?.outcome !== "succeeded" ? (
                            <form
                                className="grid gap-3"
                                onSubmit={(event) => {
                                    event.preventDefault()
                                    void form.handleSubmit()
                                }}
                            >
                                <fieldset
                                    disabled={pending}
                                    className="grid min-w-0 gap-4"
                                >
                                    {mode !== "move" ? (
                                        <>
                                            <form.AppField
                                                name="name"
                                                children={(field) => (
                                                    <field.TextField
                                                        label="名称"
                                                        id={`${prefix}-name`}
                                                        required
                                                    />
                                                )}
                                            />
                                            <form.AppField
                                                name="code"
                                                children={(field) =>
                                                    mode === "create" ? (
                                                        <field.TextField
                                                            label={
                                                                masterDataCopy.fCategoryCode
                                                            }
                                                            id={`${prefix}-code`}
                                                            required
                                                        />
                                                    ) : (
                                                        <div className="space-y-1.5">
                                                            <Label
                                                                htmlFor={`${prefix}-code`}
                                                            >
                                                                分类代码
                                                            </Label>
                                                            <Input
                                                                id={`${prefix}-code`}
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                readOnly
                                                                aria-describedby={`${prefix}-code-help`}
                                                                className="num bg-muted/40"
                                                            />
                                                            <p
                                                                id={`${prefix}-code-help`}
                                                                className="text-xs text-muted-foreground"
                                                            >
                                                                创建后不可修改，可选中复制。
                                                            </p>
                                                        </div>
                                                    )
                                                }
                                            />
                                        </>
                                    ) : null}
                                    <form.AppField
                                        name="parentId"
                                        children={(field) => (
                                            <div className="space-y-1.5">
                                                <Label
                                                    htmlFor={`${prefix}-parent`}
                                                    className="text-sm font-medium"
                                                >
                                                    {
                                                        masterDataCopy.fParentCategory
                                                    }
                                                </Label>
                                                <CategoryCombobox
                                                    id={`${prefix}-parent`}
                                                    categories={
                                                        categoryParentOptions
                                                    }
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    onValueChange={(id) =>
                                                        field.handleChange(
                                                            id ?? "",
                                                        )
                                                    }
                                                    placeholder="可选上级；空为根分类"
                                                    emptyLabel="没有可选上级分类"
                                                    className="w-full"
                                                />
                                                <p className="text-xs text-muted-foreground">
                                                    {categoryListQuery.isPending
                                                        ? "正在加载上级分类…"
                                                        : categoryListQuery.isError
                                                          ? "上级分类加载失败，请重新打开后重试。"
                                                          : "留空表示一级分类；不可选择自身或下级。"}
                                                </p>
                                                {mode === "move" ? (
                                                    <p className="rounded-lg bg-muted/50 px-3 py-2 text-sm">
                                                        当前位置：
                                                        {currentNode?.pathLabel ??
                                                            "分类路径暂不可用"}
                                                        <br />
                                                        调整后：
                                                        {categoryParentOptions.find(
                                                            (option) =>
                                                                option.categoryId ===
                                                                field.state
                                                                    .value,
                                                        )?.pathLabel ??
                                                            (field.state.value
                                                                ? "上级分类暂不可用"
                                                                : "全部分类")}
                                                        {" / "}
                                                        {currentNode?.item
                                                            .name ?? "当前分类"}
                                                    </p>
                                                ) : null}
                                            </div>
                                        )}
                                    />
                                    {mode !== "move" ? (
                                        <form.AppField
                                            name="productKind"
                                            children={(field) => (
                                                <field.SelectField
                                                    label={
                                                        masterDataCopy.fProductKind
                                                    }
                                                    id={`${prefix}-product-kind`}
                                                    options={PRODUCT_KIND_OPTIONS.map(
                                                        (option) => ({
                                                            value: option,
                                                            label: option,
                                                        }),
                                                    )}
                                                    allowClear
                                                    placeholder="未填写"
                                                />
                                            )}
                                        />
                                    ) : null}
                                    <form.AppField
                                        name="changeReason"
                                        children={(field) => (
                                            <field.TextareaField
                                                label={
                                                    description ===
                                                    masterDataCopy.createDesc
                                                        ? "创建说明"
                                                        : masterDataCopy.fieldChangeReason
                                                }
                                                id={`${prefix}-change-reason`}
                                                required
                                            />
                                        )}
                                    />
                                </fieldset>
                                <DialogFooter>
                                    <DialogClose
                                        render={
                                            <Button
                                                id={`${prefix}-cancel`}
                                                type="button"
                                                variant="outline"
                                                disabled={pending}
                                            />
                                        }
                                    >
                                        取消
                                    </DialogClose>
                                    <form.AppForm>
                                        <form.SubmitButton
                                            id={`${prefix}-submit`}
                                            label={
                                                pending
                                                    ? "提交中…"
                                                    : submitLabel
                                            }
                                            pendingLabel="提交中…"
                                            disabled={pending}
                                        />
                                    </form.AppForm>
                                </DialogFooter>
                            </form>
                        ) : null}
                    </DialogScrollBody>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix={`${prefix}-discard`}
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                onConfirm={() => {
                    setDiscardOpen(false)
                    close()
                }}
            />
        </>
    )
}
