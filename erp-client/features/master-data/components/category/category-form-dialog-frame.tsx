"use client"

import * as React from "react"

import { CategoryCombobox, FormalActionResult } from "@/components/business"
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
import { Label } from "@/components/ui/label"
import type { CategoryFormValues } from "@/features/master-data/components/category/category-form-schema"
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import {
    buildCategoryForest,
    collectDescendantIds,
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
    pending,
    submitLabel,
    excludeStableId,
    onReset,
    idPrefix,
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
        handleSubmit: () => unknown
    }
    result: MasterDataMutationResult | null
    pending: boolean
    submitLabel: string
    excludeStableId?: string
    onReset?: () => void
    idPrefix?: string
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

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next) onReset?.()
                onOpenChange(next)
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
                    {result?.outcome === "blocked" ? (
                        <FormalActionResult
                            status="blocked"
                            title={masterDataCopy.createBlockedTitle}
                            description={result.message}
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
                                children={(field) => (
                                    <field.TextField
                                        label={masterDataCopy.fCategoryCode}
                                        id={`${prefix}-code`}
                                        required
                                    />
                                )}
                            />
                            <form.AppField
                                name="parentId"
                                children={(field) => (
                                    <div className="space-y-1.5">
                                        <Label className="text-sm font-medium">
                                            {masterDataCopy.fParentCategory}
                                        </Label>
                                        <CategoryCombobox
                                            id={`${prefix}-parent`}
                                            categories={categoryParentOptions}
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(id) =>
                                                field.handleChange(id ?? "")
                                            }
                                            placeholder="可选上级；空为根分类"
                                            emptyLabel="没有可选上级分类"
                                            className="w-full"
                                        />
                                        <p className="text-xs text-muted-foreground">
                                            留空表示根分类；不可选择自身或下级。
                                        </p>
                                    </div>
                                )}
                            />
                            <form.AppField
                                name="productKind"
                                children={(field) => (
                                    <field.SelectField
                                        label={masterDataCopy.fProductKind}
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
                                            pending ? "提交中…" : submitLabel
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
    )
}
