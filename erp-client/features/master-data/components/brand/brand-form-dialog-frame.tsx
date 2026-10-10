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
import { BrandLogoField } from "@/features/master-data/components/brand/brand-logo-field"
import type { BrandFormValues } from "@/features/master-data/components/brand/brand-form-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataMutationResult } from "@/features/master-data/types"

export function BrandFormDialogFrame({
    open,
    onOpenChange,
    title,
    description,
    identity,
    form,
    result,
    pending,
    disabled = false,
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
    identity?: string
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
                    placeholder?: string
                    rows?: number
                    textareaClassName?: string
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
            loading?: boolean
        }>
        handleSubmit: () => unknown
    }
    result: MasterDataMutationResult | null
    pending: boolean
    disabled?: boolean
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
                className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl"
                closeButtonId={`${prefix}-close`}
            >
                <DialogHeader className="shrink-0 gap-2 border-b px-6 py-5 pr-14">
                    <DialogTitle className="text-lg">{title}</DialogTitle>
                    {identity ? (
                        <p className="num text-xs text-muted-foreground">
                            资料编号：{identity}
                        </p>
                    ) : null}
                    {description ? (
                        <DialogDescription>{description}</DialogDescription>
                    ) : null}
                </DialogHeader>
                {result?.outcome !== "succeeded" ? (
                    <form
                        className="flex min-h-0 flex-1 flex-col"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="grid min-h-0 gap-5 overflow-y-auto px-6 py-5">
                            {result?.outcome === "blocked" ? (
                                <FormalActionResult
                                    status="blocked"
                                    title={
                                        identity
                                            ? masterDataCopy.reviseBlockedTitle
                                            : masterDataCopy.createBlockedTitle
                                    }
                                    description={result.message}
                                />
                            ) : null}
                            <div className="grid gap-4 sm:grid-cols-2">
                                <form.AppField
                                    name="name"
                                    children={(field) => (
                                        <field.TextField
                                            label="品牌名称"
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
                                    <BrandLogoField
                                        id={`${prefix}-logo`}
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        disabled={pending || disabled}
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
                                        placeholder={
                                            identity
                                                ? "例如：调整品牌名称、更新品牌 Logo"
                                                : "填写品牌创建说明"
                                        }
                                        rows={3}
                                        textareaClassName="min-h-20 resize-none"
                                        required
                                    />
                                )}
                            />
                        </div>
                        <DialogFooter className="shrink-0 flex-row items-center justify-end border-t bg-surface-sunken/40 px-6 py-4">
                            <DialogClose
                                render={
                                    <Button
                                        id={`${prefix}-cancel`}
                                        type="button"
                                        variant="outline"
                                        disabled={pending || disabled}
                                    />
                                }
                            >
                                取消
                            </DialogClose>
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${prefix}-submit`}
                                    label={pending ? "提交中…" : submitLabel}
                                    pendingLabel="提交中…"
                                    loading={pending}
                                    disabled={pending || disabled}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
