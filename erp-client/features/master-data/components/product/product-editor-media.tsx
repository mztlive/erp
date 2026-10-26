"use client"

import * as React from "react"
import { ArrowDownIcon, ArrowUpIcon, ImageIcon, XIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { FileUpload, imagePreviewSource } from "@/components/ui/file-upload"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { moveListItem } from "@/features/master-data/lib/move-list-item"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { fileNameConflictError } from "@/lib/file-selection"
import { fileUploadText } from "@/lib/ui-text"

function MoneyInput({
    value,
    onChange,
    disabled = false,
    "aria-label": ariaLabel,
    id,
}: {
    value: string
    onChange: (next: string) => void
    disabled?: boolean
    "aria-label": string
    id?: string
}) {
    const showPrefix = !value.trim().startsWith("¥")
    return (
        <div className="relative">
            {showPrefix ? (
                <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
                    ¥
                </span>
            ) : null}
            <Input
                id={id}
                className={cn("h-control-sm", showPrefix && "pl-6")}
                value={value}
                disabled={disabled}
                onChange={(event) =>
                    onChange(event.target.value.replaceAll("¥", ""))
                }
                aria-label={ariaLabel}
            />
        </div>
    )
}

function MediaListEditor({
    idPrefix,
    label,
    hint,
    value,
    onChange,
    previewUrls,
    onPreviewUrlsChange,
    onFilesSelected,
    mode = "carousel",
    readOnly = false,
    disabled = false,
    existingFileNames,
}: {
    idPrefix?: string
    label: string
    hint?: string
    value: readonly string[]
    onChange: (next: string[]) => void
    /** fileName → 可访问预览地址（远程 URL 或本地 blob）。 */
    previewUrls?: Readonly<Record<string, string>>
    onPreviewUrlsChange?: (next: Record<string, string>) => void
    /** 选择文件时透出原始文件（用于保存前上传）。 */
    onFilesSelected?: (files: File[]) => void
    mode?: "carousel" | "detail"
    readOnly?: boolean
    disabled?: boolean
    existingFileNames?: readonly string[]
}) {
    const [localPreviewUrls, setLocalPreviewUrls] = React.useState<
        ReadonlyMap<string, string>
    >(() => new Map())
    const localPreviewUrlsRef = React.useRef<ReadonlyMap<string, string>>(
        new Map(),
    )
    const [expandedPreview, setExpandedPreview] = React.useState<{
        name: string
        src: string
    } | null>(null)

    const updateLocalPreviewUrls = React.useCallback(
        (
            update: (
                previous: ReadonlyMap<string, string>,
            ) => Map<string, string>,
        ) => {
            const next = update(localPreviewUrlsRef.current)
            localPreviewUrlsRef.current = next
            setLocalPreviewUrls(next)
        },
        [],
    )

    React.useEffect(
        () => () => {
            for (const src of localPreviewUrlsRef.current.values()) {
                URL.revokeObjectURL(src)
            }
        },
        [],
    )

    React.useEffect(() => {
        const retainedNames = new Set(value)
        const removedNames = [...localPreviewUrlsRef.current.keys()].filter(
            (name) =>
                !retainedNames.has(name) ||
                Boolean(
                    previewUrls?.[name] &&
                    !previewUrls[name].startsWith("blob:"),
                ),
        )
        if (removedNames.length === 0) return
        setExpandedPreview((current) =>
            current && removedNames.includes(current.name) ? null : current,
        )
        updateLocalPreviewUrls((previous) => {
            const next = new Map(previous)
            for (const name of removedNames) {
                const src = next.get(name)
                if (src) URL.revokeObjectURL(src)
                next.delete(name)
            }
            return next
        })
    }, [updateLocalPreviewUrls, value, previewUrls])

    const derivedBasePrefix =
        idPrefix ?? `master-data-product-media-${toAutomationIdSegment(label)}`
    return (
        <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
                <div>
                    <Label className="text-sm font-medium">{label}</Label>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {hint ?? masterDataCopy.mediaUploadHint}
                    </p>
                </div>
                <Badge variant="secondary">
                    {masterDataCopy.mediaCount(value.length)}
                </Badge>
            </div>
            <div
                className={cn(
                    "grid gap-3",
                    mode === "carousel"
                        ? "grid-cols-2 sm:grid-cols-4"
                        : "grid-cols-2 sm:grid-cols-3 lg:grid-cols-6",
                )}
            >
                {value.map((name, index) => {
                    const previewSrc =
                        localPreviewUrls.get(name) ??
                        previewUrls?.[name] ??
                        imagePreviewSource(name)
                    const itemSegment = toAutomationIdSegment(
                        name || `item-${index}`,
                    )
                    const base = `${derivedBasePrefix}-${itemSegment}`
                    return (
                        <div
                            key={`${name}-${index}`}
                            className="group relative overflow-hidden rounded-xl border border-border bg-surface-sunken"
                        >
                            <button
                                id={`${base}-preview`}
                                type="button"
                                className={cn(
                                    "relative flex w-full flex-col items-center justify-center gap-2 overflow-hidden p-3 text-center focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                                    mode === "carousel"
                                        ? "aspect-square"
                                        : "aspect-[4/5]",
                                    previewSrc && "cursor-zoom-in p-0",
                                )}
                                aria-label={
                                    previewSrc ? `放大预览 ${name}` : name
                                }
                                onClick={() => {
                                    if (previewSrc)
                                        setExpandedPreview({
                                            name,
                                            src: previewSrc,
                                        })
                                }}
                            >
                                {previewSrc ? (
                                    // eslint-disable-next-line @next/next/no-img-element -- 本地待上传图片使用 blob URL。
                                    <img
                                        src={previewSrc}
                                        alt={name}
                                        className="absolute inset-0 size-full object-cover"
                                    />
                                ) : (
                                    <>
                                        <ImageIcon
                                            className="size-7 text-muted-foreground"
                                            aria-hidden
                                        />
                                        <span className="line-clamp-2 break-all text-xs text-muted-foreground">
                                            {name}
                                        </span>
                                    </>
                                )}
                            </button>
                            <Badge
                                variant="secondary"
                                className="absolute left-2 top-2 tabular-nums"
                            >
                                {index + 1}
                            </Badge>
                            {mode === "carousel" && index === 0 ? (
                                <Badge className="absolute right-2 top-2">
                                    首图
                                </Badge>
                            ) : null}
                            <p className="px-2 py-1 text-xs text-muted-foreground">
                                {previewSrc?.startsWith("blob:")
                                    ? fileUploadText.pendingSave
                                    : fileUploadText.saved}
                            </p>
                            {!readOnly ? (
                                <div className="flex flex-wrap items-center justify-center gap-1 border-t border-border bg-surface-sunken/95 p-1">
                                    <Button
                                        id={`${base}-move-up`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        disabled={disabled || index === 0}
                                        aria-label={`${name} 前移`}
                                        title="前移"
                                        onClick={() =>
                                            onChange(
                                                moveListItem(
                                                    value,
                                                    index,
                                                    index - 1,
                                                ),
                                            )
                                        }
                                    >
                                        <ArrowUpIcon />
                                    </Button>
                                    <Button
                                        id={`${base}-move-down`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        disabled={
                                            disabled ||
                                            index === value.length - 1
                                        }
                                        aria-label={`${name} 后移`}
                                        title="后移"
                                        onClick={() =>
                                            onChange(
                                                moveListItem(
                                                    value,
                                                    index,
                                                    index + 1,
                                                ),
                                            )
                                        }
                                    >
                                        <ArrowDownIcon />
                                    </Button>
                                    {mode === "carousel" && index > 0 ? (
                                        <Button
                                            id={`${base}-set-cover`}
                                            disabled={disabled}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            onClick={() =>
                                                onChange(
                                                    moveListItem(
                                                        value,
                                                        index,
                                                        0,
                                                    ),
                                                )
                                            }
                                        >
                                            设为首图
                                        </Button>
                                    ) : null}
                                    <Button
                                        id={`${base}-remove`}
                                        disabled={disabled}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                                        onClick={() =>
                                            onChange(
                                                value.filter(
                                                    (_, i) => i !== index,
                                                ),
                                            )
                                        }
                                    >
                                        <XIcon />
                                    </Button>
                                </div>
                            ) : null}
                        </div>
                    )
                })}
                {!readOnly ? (
                    <FileUpload
                        idPrefix={`${derivedBasePrefix}-upload`}
                        disabled={disabled}
                        showTileLabel
                        validateFiles={(files) =>
                            fileNameConflictError(
                                files,
                                existingFileNames ?? value,
                            )
                        }
                        accept="image/jpeg,image/png,image/webp"
                        multiple
                        label={`添加${label}`}
                        density="tile"
                        description={
                            mode === "carousel"
                                ? "点击或拖入图片，支持多选"
                                : "点击或拖入图片，按顺序展示"
                        }
                        onFilesSelected={(files) => {
                            onFilesSelected?.(files)
                            const addedUrls: Record<string, string> = {}
                            updateLocalPreviewUrls((previous) => {
                                const next = new Map(previous)
                                for (const file of files) {
                                    const previousSrc = next.get(file.name)
                                    if (previousSrc)
                                        URL.revokeObjectURL(previousSrc)
                                    const blobUrl = URL.createObjectURL(file)
                                    next.set(file.name, blobUrl)
                                    addedUrls[file.name] = blobUrl
                                }
                                return next
                            })
                            if (onPreviewUrlsChange) {
                                onPreviewUrlsChange({
                                    ...previewUrls,
                                    ...addedUrls,
                                })
                            }
                            onChange([...value, ...files.map((f) => f.name)])
                        }}
                        controlClassName={
                            mode === "carousel"
                                ? "aspect-square"
                                : "aspect-[4/5]"
                        }
                    />
                ) : null}
            </div>
            {value.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.mediaEmpty}（
                    {masterDataCopy.mediaAllowEmpty}）
                </p>
            ) : null}

            <Dialog
                open={Boolean(expandedPreview)}
                onOpenChange={(open) => {
                    if (!open) setExpandedPreview(null)
                }}
            >
                <DialogContent
                    className="gap-4 p-4 sm:max-w-4xl"
                    closeButtonId={`${derivedBasePrefix}-preview-close`}
                >
                    <DialogHeader className="pr-10">
                        <DialogTitle>
                            {expandedPreview?.name ?? "图片预览"}
                        </DialogTitle>
                        <DialogDescription>图片预览</DialogDescription>
                    </DialogHeader>
                    <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-lg bg-surface-sunken">
                        {expandedPreview ? (
                            // eslint-disable-next-line @next/next/no-img-element -- 本地待上传图片使用 blob URL。
                            <img
                                src={expandedPreview.src}
                                alt={expandedPreview.name}
                                className="max-h-[75dvh] max-w-full object-contain"
                            />
                        ) : null}
                    </div>
                </DialogContent>
            </Dialog>
        </div>
    )
}

function SkuMainImageField({
    value,
    previewUrl,
    onChange,
    onFilesSelected,
    disabled = false,
    idPrefix,
}: {
    value: string
    /** 可访问预览地址（远程 URL 或本地 blob）；缺省回退文件名。 */
    previewUrl?: string
    onChange: (next: string) => void
    /** 选择文件时透出原始文件（用于保存前上传）。 */
    onFilesSelected?: (files: File[]) => void
    disabled?: boolean
    idPrefix?: string
}) {
    const [file, setFile] = React.useState<File | null>(null)
    return (
        <FileUpload
            idPrefix={idPrefix ?? "master-data-product-sku-main-image"}
            accept="image/jpeg,image/png,image/webp"
            multiple={false}
            label={masterDataCopy.fMainImage}
            description="1:1"
            density="tile"
            className="w-16"
            controlClassName="aspect-square"
            selectedDescription={fileUploadText.pendingSave}
            readOnly={disabled}
            previewActions="overlay"
            showPreviewStatus={false}
            previewSelectedImage
            selectedImageFile={
                previewUrl?.startsWith("blob:") && file?.name === value
                    ? file
                    : null
            }
            preview={
                value
                    ? {
                          src: previewUrl ?? imagePreviewSource(value),
                          name: value,
                          status: previewUrl?.startsWith("blob:")
                              ? "pending"
                              : "saved",
                      }
                    : null
            }
            onPreviewRemove={() => {
                setFile(null)
                onChange("")
            }}
            onFilesSelected={(files) => {
                setFile(files[0] ?? null)
                onFilesSelected?.(files)
                if (files[0]) onChange(files[0].name)
            }}
        />
    )
}

export { MediaListEditor, MoneyInput, SkuMainImageField }
