"use client"

import * as React from "react"

import { DiscardConfirmDialog, FormalActionResult } from "@/components/business"
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
                                            disabled={pending}
                                        />
                                    }
                                >
                                    关闭
                                </DialogClose>
                                <Button type="submit" disabled={pending}>
                                    {pending ? "提交中…" : submitLabel}
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
