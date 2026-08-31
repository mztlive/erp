"use client"

import * as React from "react"
import { z } from "zod"

import { DiscardConfirmDialog, FormalActionResult } from "@/components/business"
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
import {
    DialogScrollBody,
    newIdempotencyKey,
    notifySuccess,
} from "@/features/master-data/components/shared/action-dialog-shared"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
} from "@/features/master-data/hooks/queries"
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

const QUANTITY_SCALE_OPTIONS = ["0", "1", "2", "3", "4", "5", "6"] as const

const unitFormSchema = z.object({
    name: z.string().trim().min(1, "请填写名称"),
    code: z.string().trim().min(1, "请填写单位代码"),
    symbol: z.string().trim().min(1, "请填写单位符号"),
    quantityScale: z.enum(QUANTITY_SCALE_OPTIONS),
    changeReason: z.string().trim().min(2, "请填写变更原因"),
})

type UnitFormValues = {
    name: string
    code: string
    symbol: string
    quantityScale: (typeof QUANTITY_SCALE_OPTIONS)[number]
    changeReason: string
}

function emptyUnitForm(): UnitFormValues {
    return {
        name: "",
        code: "",
        symbol: "",
        quantityScale: "0",
        changeReason: "",
    }
}

export function UnitOfMeasureCreateDialog({
    open,
    onOpenChange,
    id,
    idPrefix,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    id?: string
    idPrefix?: string
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create-uom"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: emptyUnitForm(),
        validators: { onChange: unitFormSchema },
        onSubmit: async ({ value }) => {
            const response = await mutation.mutateAsync({
                resource: "unit-of-measures",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields: {
                    code: value.code.trim(),
                    symbol: value.symbol.trim(),
                    quantityScale: value.quantityScale,
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
        setIdempotencyKey(newIdempotencyKey("create-uom"))
        form.reset()
    }

    return (
        <UnitFormDialogFrame
            id={id}
            idPrefix={idPrefix}
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.createTitle("计量单位")}
            description={masterDataCopy.createDesc}
            form={form as never}
            result={result}
            pending={mutation.isPending}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.createSubmit}
            codeReadOnly={false}
            onReset={reset}
        />
    )
}

export function UnitOfMeasureReviseDialog({
    open,
    onOpenChange,
    target,
    id,
    idPrefix,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
    id?: string
    idPrefix?: string
}) {
    const mutation = useCreateRevisionMutation()
    const ids = revisionTargetIds(target)
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("revise-uom"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: emptyUnitForm(),
        validators: { onChange: unitFormSchema },
        onSubmit: async ({ value }) => {
            if (!ids.stableId || !ids.baseRevisionId) return
            const response = await mutation.mutateAsync({
                resource: "unit-of-measures",
                stableId: ids.stableId,
                baseRevisionId: ids.baseRevisionId,
                expectedLockVersion: ids.lockVersion,
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields: {
                    code: value.code.trim(),
                    symbol: value.symbol.trim(),
                    quantityScale: value.quantityScale,
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
        const scale = QUANTITY_SCALE_OPTIONS.find(
            (option) => option === values.quantityScale,
        )
        form.setFieldValue("name", target.name)
        form.setFieldValue("code", values.code ?? "")
        form.setFieldValue("symbol", values.symbol ?? "")
        form.setFieldValue("quantityScale", scale ?? "0")
        form.setFieldValue("changeReason", "")
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("revise-uom"))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId, ids.baseRevisionId])

    return (
        <UnitFormDialogFrame
            id={id}
            idPrefix={idPrefix}
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
            codeReadOnly
        />
    )
}

function UnitFormDialogFrame({
    id,
    idPrefix,
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
    codeReadOnly,
    onReset,
}: {
    id?: string
    idPrefix?: string
    open: boolean
    onOpenChange: (open: boolean) => void
    title: string
    description: React.ReactNode
    // 各资源表单实例的泛型无法与 useAppForm 返回值对齐，这里只消费字段渲染契约。
    form: {
        AppField: React.ComponentType<{
            name: keyof UnitFormValues
            children: (field: {
                TextField: React.ComponentType<{
                    label: string
                    disabled?: boolean
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
                    required?: boolean
                    id?: string
                }>
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
        state: { isDirty: boolean }
        reset: () => void
    }
    result: MasterDataMutationResult | null
    pending: boolean
    discardOpen: boolean
    setDiscardOpen: (open: boolean) => void
    submitLabel: string
    codeReadOnly: boolean
    onReset?: () => void
}) {
    const baseId = idPrefix ?? id ?? "master-data-unit-of-measure-form-dialog"
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
            <DialogContent
                closeButtonId={`${baseId}-close`}
                className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg"
            >
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
                                    <field.TextField
                                        id={`${baseId}-name`}
                                        label="名称"
                                        required
                                    />
                                )}
                            />
                            <form.AppField
                                name="code"
                                children={(field) => (
                                    <field.TextField
                                        id={`${baseId}-code`}
                                        label={masterDataCopy.fUnitCode}
                                        disabled={codeReadOnly}
                                        required
                                    />
                                )}
                            />
                            <form.AppField
                                name="symbol"
                                children={(field) => (
                                    <field.TextField
                                        id={`${baseId}-symbol`}
                                        label={masterDataCopy.fUnitSymbol}
                                        required
                                    />
                                )}
                            />
                            <form.AppField
                                name="quantityScale"
                                children={(field) => (
                                    <field.SelectField
                                        id={`${baseId}-quantity-scale`}
                                        label={masterDataCopy.fQuantityScale}
                                        options={QUANTITY_SCALE_OPTIONS.map(
                                            (option) => ({
                                                value: option,
                                                label: option,
                                            }),
                                        )}
                                        required
                                    />
                                )}
                            />
                            <form.AppField
                                name="changeReason"
                                children={(field) => (
                                    <field.TextareaField
                                        id={`${baseId}-change-reason`}
                                        label={masterDataCopy.fieldChangeReason}
                                        required
                                    />
                                )}
                            />
                            <DialogFooter>
                                <DialogClose
                                    id={`${baseId}-cancel`}
                                    render={
                                        <Button
                                            type="button"
                                            variant="outline"
                                            disabled={pending}
                                        />
                                    }
                                >
                                    关闭
                                </DialogClose>
                                <form.AppForm>
                                    <form.SubmitButton
                                        id={`${baseId}-submit`}
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
            <DiscardConfirmDialog
                id={`${baseId}-discard`}
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
