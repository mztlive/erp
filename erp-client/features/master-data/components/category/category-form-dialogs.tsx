"use client"

import * as React from "react"
import { z } from "zod"

import {
    CategoryCombobox,
    DiscardConfirmDialog,
    FormalActionResult,
} from "@/components/business"
import { useAppForm } from "@/components/form"
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
import {
    DialogScrollBody,
    newIdempotencyKey,
    notifySuccess,
} from "@/features/master-data/components/shared/action-dialog-shared"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
} from "@/features/master-data/hooks/queries"
import {
    buildCategoryForest,
    collectDescendantIds,
    toCategoryComboboxItems,
} from "@/features/master-data/lib/category-tree-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    currentResourceFieldValues,
    defaultImmediateEffectiveFrom,
} from "@/features/master-data/lib/resource-fields"
import {
    revisionTargetIds,
    type RevisionTarget,
} from "@/features/master-data/lib/revision-target"
import type { MasterDataMutationResult } from "@/features/master-data/types"

const PRODUCT_KIND_OPTIONS = ["实物", "虚拟", "服务", "卡券"] as const

const categoryFormSchema = z.object({
    name: z.string().trim().min(2, "请填写名称"),
    code: z.string().trim().min(1, "请填写分类代码"),
    parentId: z.string(),
    productKind: z.string(),
    changeReason: z.string().trim().min(2, "请填写变更原因"),
})

type CategoryFormValues = {
    name: string
    code: string
    parentId: string
    productKind: string
    changeReason: string
}

function emptyCategoryForm(parentId = ""): CategoryFormValues {
    return {
        name: "",
        code: "",
        parentId,
        productKind: "",
        changeReason: "",
    }
}

export function CategoryCreateDialog({
    open,
    onOpenChange,
    defaultParentId,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    defaultParentId?: string
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create-category"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: emptyCategoryForm(defaultParentId),
        validators: { onChange: categoryFormSchema },
        onSubmit: async ({ value }) => {
            const response = await mutation.mutateAsync({
                resource: "categories",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields: {
                    code: value.code.trim(),
                    parentId: value.parentId.trim() || undefined,
                    productKind: value.productKind.trim() || undefined,
                },
                idempotencyKey,
            })
            if (response.outcome === "succeeded") {
                notifySuccess(masterDataCopy.createSuccessTitle, response)
                reset()
                onOpenChange(false)
                return
            }
            setResult(response)
        },
    })

    const reset = () => {
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("create-category"))
        form.reset()
    }

    return (
        <CategoryFormDialogFrame
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.createTitle("商品分类")}
            description={masterDataCopy.createDesc}
            form={form as never}
            result={result}
            pending={mutation.isPending}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.createSubmit}
            onReset={reset}
        />
    )
}

export function CategoryReviseDialog({
    open,
    onOpenChange,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
}) {
    const mutation = useCreateRevisionMutation()
    const ids = revisionTargetIds(target)
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("revise-category"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: emptyCategoryForm(),
        validators: { onChange: categoryFormSchema },
        onSubmit: async ({ value }) => {
            if (!ids.stableId || !ids.baseRevisionId) return
            const response = await mutation.mutateAsync({
                resource: "categories",
                stableId: ids.stableId,
                baseRevisionId: ids.baseRevisionId,
                expectedLockVersion: ids.lockVersion,
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields: {
                    code: value.code.trim(),
                    parentId: value.parentId.trim() || undefined,
                    productKind: value.productKind.trim() || undefined,
                },
                idempotencyKey,
            })
            if (response.outcome === "succeeded") {
                notifySuccess(masterDataCopy.reviseSuccessTitle, response)
                onOpenChange(false)
                return
            }
            setResult(response)
        },
    })

    React.useEffect(() => {
        if (!open || !target) return
        const values = currentResourceFieldValues(target)
        form.setFieldValue("name", target.name)
        form.setFieldValue("code", values.code ?? "")
        form.setFieldValue("parentId", values.parentId ?? "")
        form.setFieldValue("productKind", values.productKind ?? "")
        form.setFieldValue("changeReason", "")
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("revise-category"))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId, ids.baseRevisionId])

    return (
        <CategoryFormDialogFrame
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.reviseTitle}
            description={
                <>
                    {masterDataCopy.reviseDesc}
                    {target ? (
                        <>
                            {" "}
                            资料编号{" "}
                            <span className="num">{target.stableNo}</span>
                        </>
                    ) : null}
                </>
            }
            form={form as never}
            result={result}
            pending={mutation.isPending || !target}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.reviseSubmit}
            excludeStableId={ids.stableId || undefined}
        />
    )
}

function CategoryFormDialogFrame({
    open,
    onOpenChange,
    title,
    description,
    form,
    result,
    pending,
    discardOpen,
    setDiscardOpen,
    submitLabel,
    excludeStableId,
    onReset,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    title: string
    description: React.ReactNode
    form: {
        AppField: React.ComponentType<{
            name: keyof CategoryFormValues
            children: (field: {
                TextField: React.ComponentType<{ label: string }>
                TextareaField: React.ComponentType<{ label: string }>
                SelectField: React.ComponentType<{
                    label: string
                    options: readonly { value: string; label: string }[]
                    allowClear?: boolean
                    placeholder?: string
                }>
                handleChange: (value: string) => void
                state: { value: string }
            }) => React.ReactNode
        }>
        handleSubmit: () => unknown
        state: { isDirty: boolean }
        reset: () => void
    }
    result: MasterDataMutationResult | null
    pending: boolean
    discardOpen: boolean
    setDiscardOpen: (open: boolean) => void
    submitLabel: string
    excludeStableId?: string
    onReset?: () => void
}) {
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

    const requestClose = (next: boolean) => {
        if (next) {
            onOpenChange(true)
            return
        }
        if (form.state.isDirty || result) {
            setDiscardOpen(true)
            return
        }
        onReset?.()
        onOpenChange(false)
    }

    return (
        <Dialog open={open} onOpenChange={requestClose}>
            <DialogContent className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>{description}</DialogDescription>
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
                                    <field.TextField label="名称" />
                                )}
                            />
                            <form.AppField
                                name="code"
                                children={(field) => (
                                    <field.TextField
                                        label={masterDataCopy.fCategoryCode}
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
                                        label={masterDataCopy.fieldChangeReason}
                                    />
                                )}
                            />
                            <DialogFooter>
                                <DialogClose
                                    render={
                                        <Button
                                            type="button"
                                            variant="outline"
                                        />
                                    }
                                >
                                    关闭
                                </DialogClose>
                                <Button type="submit" disabled={pending}>
                                    {submitLabel}
                                </Button>
                            </DialogFooter>
                        </form>
                    ) : null}
                </DialogScrollBody>
            </DialogContent>
            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃本次填写？"
                description="关闭后本次填写的内容将丢失。"
                confirmLabel="放弃填写"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setDiscardOpen(false)
                    onReset?.()
                    onOpenChange(false)
                }}
            />
        </Dialog>
    )
}
