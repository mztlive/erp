"use client"

import * as React from "react"

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
    MediaSingleField,
    newIdempotencyKey,
    notifySuccess,
    resolveBrandLogoFields,
} from "@/features/master-data/components/shared/action-dialog-shared"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
} from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import { currentResourceFieldValues } from "@/features/master-data/lib/resource-fields"
import {
    revisionTargetIds,
    type RevisionTarget,
} from "@/features/master-data/lib/revision-target"
import type { BrandFields, MasterDataMutationResult } from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"
import { z } from "zod"

const brandFormSchema = z.object({
    name: z.string().trim().min(2, "请填写名称"),
    code: z.string().trim().min(1, "请填写品牌代码"),
    logo: z.string(),
    changeReason: z.string().trim().min(2, "请填写变更原因"),
})

type BrandFormValues = {
    name: string
    code: string
    logo: string
    changeReason: string
}

function emptyBrandForm(): BrandFormValues {
    return { name: "", code: "", logo: "", changeReason: "" }
}

export function BrandCreateDialog({
    open,
    onOpenChange,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create-brand"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    const [logoAssetId, setLogoAssetId] = React.useState("")
    const [logoPreviewUrl, setLogoPreviewUrl] = React.useState("")

    const form = useAppForm({
        defaultValues: emptyBrandForm(),
        validators: { onChange: brandFormSchema },
        onSubmit: async ({ value }) => {
            let fields: BrandFields
            try {
                fields = await resolveBrandLogoFields(
                    {
                        code: value.code.trim(),
                        logo: value.logo.trim() || undefined,
                    },
                    pendingFilesRef.current.get(`logo::${value.logo}`),
                    logoAssetId,
                    logoPreviewUrl,
                )
            } catch (error) {
                setResult({
                    outcome: "blocked",
                    code: "MEDIA_UPLOAD_FAILED",
                    message: getErrorMessage(error, "Logo 上传失败"),
                })
                return
            }
            const response = await mutation.mutateAsync({
                resource: "brands",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields,
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
        setIdempotencyKey(newIdempotencyKey("create-brand"))
        setLogoAssetId("")
        setLogoPreviewUrl("")
        pendingFilesRef.current.clear()
        form.reset()
    }

    return (
        <BrandFormDialogFrame
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.createTitle("品牌")}
            description={masterDataCopy.createDesc}
            form={form as never}
            result={result}
            pending={mutation.isPending}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.createSubmit}
            onReset={reset}
            logoPreviewUrl={logoPreviewUrl}
            onLogoFiles={(files) => {
                for (const file of files) {
                    pendingFilesRef.current.set(`logo::${file.name}`, file)
                }
                setLogoAssetId("")
            }}
        />
    )
}

export function BrandReviseDialog({
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
        newIdempotencyKey("revise-brand"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    const [logoAssetId, setLogoAssetId] = React.useState("")
    const [logoPreviewUrl, setLogoPreviewUrl] = React.useState("")

    const form = useAppForm({
        defaultValues: emptyBrandForm(),
        validators: { onChange: brandFormSchema },
        onSubmit: async ({ value }) => {
            if (!ids.stableId || !ids.baseRevisionId) return
            let fields: BrandFields
            try {
                fields = await resolveBrandLogoFields(
                    {
                        code: value.code.trim(),
                        logo: value.logo.trim() || undefined,
                    },
                    pendingFilesRef.current.get(`logo::${value.logo}`),
                    logoAssetId,
                    logoPreviewUrl,
                )
            } catch (error) {
                setResult({
                    outcome: "blocked",
                    code: "MEDIA_UPLOAD_FAILED",
                    message: getErrorMessage(error, "Logo 上传失败"),
                })
                return
            }
            const response = await mutation.mutateAsync({
                resource: "brands",
                stableId: ids.stableId,
                baseRevisionId: ids.baseRevisionId,
                expectedLockVersion: ids.lockVersion,
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields,
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
        form.setFieldValue("logo", values.logo ?? "")
        form.setFieldValue("changeReason", "")
        const logoAsset =
            "mediaAssets" in target ? target.mediaAssets?.logo?.[0] : undefined
        setLogoAssetId(logoAsset?.assetId ?? "")
        setLogoPreviewUrl(logoAsset?.url ?? "")
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("revise-brand"))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId, ids.baseRevisionId])

    return (
        <BrandFormDialogFrame
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
            logoPreviewUrl={logoPreviewUrl}
            onLogoFiles={(files) => {
                for (const file of files) {
                    pendingFilesRef.current.set(`logo::${file.name}`, file)
                }
                setLogoAssetId("")
            }}
        />
    )
}

function BrandFormDialogFrame({
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
    onReset,
    logoPreviewUrl,
    onLogoFiles,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    title: string
    description: React.ReactNode
    form: {
        AppField: React.ComponentType<{
            name: keyof BrandFormValues
            children: (field: {
                TextField: React.ComponentType<{ label: string }>
                TextareaField: React.ComponentType<{ label: string }>
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
    onReset?: () => void
    logoPreviewUrl: string
    onLogoFiles: (files: File[]) => void
}) {
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
                            <div className="grid gap-3 sm:grid-cols-2">
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
                                            label={masterDataCopy.fBrandCode}
                                        />
                                    )}
                                />
                            </div>
                            <form.AppField
                                name="logo"
                                children={(field) => (
                                    <MediaSingleField
                                        label={masterDataCopy.fBrandLogo}
                                        hint={masterDataCopy.brandLogoHint}
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        selectedHint="Logo · 1:1 · 已选择"
                                        aspectRatio="1:1"
                                        previewUrl={logoPreviewUrl}
                                        onFilesSelected={onLogoFiles}
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
