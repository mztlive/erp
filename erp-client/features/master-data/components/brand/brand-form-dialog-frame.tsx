"use client"

import * as React from "react"

import { FormalActionResult } from "@/components/business"
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
import type { BrandFormValues } from "@/features/master-data/components/brand/brand-form-model"
import {
    DialogScrollBody,
    MediaSingleField,
} from "@/features/master-data/components/shared/action-dialog-shared"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataMutationResult } from "@/features/master-data/types"

export function BrandFormDialogFrame({
    open,
    onOpenChange,
    title,
    description,
    form,
    result,
    pending,
    submitLabel,
    onReset,
    logoPreviewUrl,
    onLogoFiles,
    idPrefix,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    title: string
    description: React.ReactNode
    form: {
        AppField: React.ComponentType<{
            name: keyof BrandFormValues
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
    onReset?: () => void
    logoPreviewUrl: string
    onLogoFiles: (files: File[]) => void
    idPrefix?: string
}) {
    const prefix = idPrefix ?? "master-data-brand-form-dialog"

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
                            <div className="grid gap-3 sm:grid-cols-2">
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
                                            label={masterDataCopy.fBrandCode}
                                            id={`${prefix}-code`}
                                            required
                                        />
                                    )}
                                />
                            </div>
                            <form.AppField
                                name="logo"
                                children={(field) => (
                                    <MediaSingleField
                                        id={`${prefix}-logo`}
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
