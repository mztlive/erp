"use client"

/* eslint-disable @next/next/no-img-element -- 本地预览包含 blob URL，不能使用 Next Image 优化器。 */

import * as React from "react"
import {
    FileUpIcon,
    RefreshCwIcon,
    ImageIcon,
    Maximize2Icon,
    UploadCloudIcon,
    XIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import { fileSelectionError } from "@/lib/file-selection"
import { fileUploadText } from "@/lib/ui-text"
import { FileUploadSelection } from "@/components/ui/file-upload-selection"

type FileUploadImagePreview = Readonly<{
    /** 已上传图片的可访问 URL；不传时以文件名占位回显。 */
    src?: string
    name: string
    status?: "uploaded" | "pending"
}>

/** 仅把浏览器可直接访问的字符串识别为图片预览源。 */
function imagePreviewSource(value?: string | null): string | undefined {
    const source = value?.trim()
    if (!source) return undefined
    return /^(https?:\/\/|\/|blob:|data:image\/)/i.test(source)
        ? source
        : undefined
}

function FileUpload({
    onFilesSelected,
    accept,
    multiple = true,
    disabled = false,
    label = fileUploadText.upload,
    description = fileUploadText.hint,
    density = "default",
    preview,
    previewSelectedImage = false,
    previewFit = "cover",
    selectedImageFile,
    onPreviewRemove,
    idPrefix,
    inputId,
    selectId,
    selectLabel,
    selectedFiles,
    onFileRemove,
    fileKey,
    maxFileSize,
    validateFiles,
    readOnly = false,
    className,
    "aria-describedby": ariaDescribedby,
    "aria-invalid": ariaInvalid,
}: {
    onFilesSelected: (files: File[]) => void
    accept?: string
    multiple?: boolean
    disabled?: boolean
    label?: React.ReactNode
    description?: React.ReactNode
    density?: "default" | "compact" | "tile" | "hero" | "button"
    /** 已上传图片回显；src 必须是浏览器可访问 URL。 */
    preview?: FileUploadImagePreview | null
    /** 选择单张本地图片后立即生成 blob URL 预览。 */
    previewSelectedImage?: boolean
    previewFit?: "cover" | "contain"
    /** 受控的本地图片；传入 `null` 时同步清除内部 blob 预览。 */
    selectedImageFile?: File | null
    /** 移除已上传或待上传预览；调用方负责同步清空表单值。 */
    onPreviewRemove?: () => void
    idPrefix?: string
    /** 迁移旧入口时保留原生 input 和触发按钮的自动化 ID。 */
    inputId?: string
    selectId?: string
    selectLabel?: string
    /** 受控的待提交文件列表；不代表服务端已上传。 */
    selectedFiles?: readonly File[]
    onFileRemove?: (file: File) => void
    fileKey?: (file: File) => string
    maxFileSize?: number
    validateFiles?: (files: readonly File[]) => string | null | undefined
    /** 仅展示已上传图片，禁止选择、替换和拖入文件。 */
    readOnly?: boolean
    className?: string
    "aria-describedby"?: string
    "aria-invalid"?: boolean | string
}) {
    const inputRef = React.useRef<HTMLInputElement>(null)
    const [dragging, setDragging] = React.useState(false)
    const [selectionError, setSelectionError] = React.useState<string | null>(
        null,
    )
    const errorId = idPrefix ? `${idPrefix}-selection-error` : undefined
    const descriptionId = idPrefix
        ? `${idPrefix}-upload-description`
        : undefined
    const describedBy =
        [
            ariaDescribedby,
            description ? descriptionId : undefined,
            selectionError ? errorId : undefined,
        ]
            .filter(Boolean)
            .join(" ") || undefined
    const [previewOpen, setPreviewOpen] = React.useState(false)
    const [localPreview, setLocalPreview] =
        React.useState<FileUploadImagePreview | null>(null)
    const localPreviewUrlRef = React.useRef<string | null>(null)

    const clearLocalPreview = React.useCallback(() => {
        if (localPreviewUrlRef.current) {
            URL.revokeObjectURL(localPreviewUrlRef.current)
            localPreviewUrlRef.current = null
        }
        setLocalPreview(null)
    }, [])

    React.useEffect(() => {
        if (selectedImageFile === undefined || !previewSelectedImage) return
        clearLocalPreview()
        if (
            !selectedImageFile ||
            !selectedImageFile.type.startsWith("image/")
        ) {
            return
        }
        const src = URL.createObjectURL(selectedImageFile)
        localPreviewUrlRef.current = src
        setLocalPreview({
            src,
            name: selectedImageFile.name,
            status: "pending",
        })
    }, [clearLocalPreview, previewSelectedImage, selectedImageFile])

    React.useEffect(
        () => () => {
            if (localPreviewUrlRef.current) {
                URL.revokeObjectURL(localPreviewUrlRef.current)
                localPreviewUrlRef.current = null
            }
        },
        [],
    )

    const selectFiles = (files: FileList | null) => {
        if (disabled || readOnly || !files?.length) return
        const selectedFiles = Array.from(files)
        const error =
            fileSelectionError(selectedFiles, {
                accept,
                multiple,
                maxFileSize,
            }) || validateFiles?.(selectedFiles)
        setSelectionError(error ?? null)
        if (error) return
        if (previewSelectedImage && selectedImageFile === undefined) {
            const image = selectedFiles.find((file) =>
                file.type.startsWith("image/"),
            )
            clearLocalPreview()
            if (image) {
                const src = URL.createObjectURL(image)
                localPreviewUrlRef.current = src
                setLocalPreview({ src, name: image.name, status: "pending" })
            }
        }
        onFilesSelected(selectedFiles)
    }

    const activePreview = localPreview ?? preview ?? null
    const previewStatus =
        activePreview?.status === "pending"
            ? fileUploadText.selected
            : fileUploadText.uploaded

    const removePreview = () => {
        setSelectionError(null)
        setPreviewOpen(false)
        clearLocalPreview()
        onPreviewRemove?.()
    }

    return (
        <div className={cn("min-w-0 space-y-2", className)}>
            {selectedFiles?.length && !activePreview ? (
                <FileUploadSelection
                    files={selectedFiles}
                    idPrefix={idPrefix}
                    multiple={multiple}
                    disabled={disabled || readOnly}
                    fileKey={fileKey}
                    onRemove={
                        onFileRemove
                            ? (file) => {
                                  setSelectionError(null)
                                  onFileRemove(file)
                              }
                            : undefined
                    }
                />
            ) : null}
            <div
                id={idPrefix ? `${idPrefix}-dropzone` : undefined}
                data-slot="file-upload"
                data-dragging={dragging || undefined}
                className={cn(
                    "flex min-w-0 items-center justify-center rounded-lg border border-dashed bg-surface-sunken transition-colors data-dragging:border-ring data-dragging:bg-accent",
                    density === "hero"
                        ? "min-h-64 flex-col gap-5 px-5 py-10 text-center sm:min-h-72 lg:min-h-90"
                        : density === "button"
                          ? "flex-row flex-wrap justify-start gap-3 border-0 bg-transparent p-0 text-left"
                          : density === "tile"
                            ? "size-full flex-col gap-1 p-1 text-center"
                            : density === "compact"
                              ? "flex-row gap-2 p-2 text-left"
                              : "flex-col gap-3 p-6 text-center",
                    activePreview && "relative overflow-hidden p-0",
                    disabled && "cursor-not-allowed opacity-50",
                )}
                onDragEnter={(event) => {
                    event.preventDefault()
                    if (!disabled && !readOnly) setDragging(true)
                }}
                onDragOver={(event) => event.preventDefault()}
                onDragLeave={(event) => {
                    if (
                        !event.currentTarget.contains(
                            event.relatedTarget as Node | null,
                        )
                    ) {
                        setDragging(false)
                    }
                }}
                onDrop={(event) => {
                    event.preventDefault()
                    setDragging(false)
                    selectFiles(event.dataTransfer.files)
                }}
            >
                <input
                    id={inputId ?? (idPrefix ? `${idPrefix}-input` : undefined)}
                    ref={inputRef}
                    className="sr-only"
                    type="file"
                    accept={accept}
                    multiple={multiple}
                    disabled={disabled || readOnly}
                    aria-describedby={describedBy}
                    aria-invalid={
                        Boolean(selectionError) ||
                        (ariaInvalid as boolean | undefined)
                    }
                    onChange={(event) => {
                        selectFiles(event.target.files)
                        event.target.value = ""
                    }}
                    aria-label={
                        typeof label === "string" ? label : "选择上传文件"
                    }
                />

                {activePreview ? (
                    <>
                        <span id={descriptionId} className="sr-only">
                            {description}
                        </span>
                        <button
                            id={idPrefix ? `${idPrefix}-preview` : undefined}
                            type="button"
                            className={cn(
                                "group relative flex w-full items-center justify-center overflow-hidden bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                                density === "tile"
                                    ? "size-full"
                                    : density === "compact"
                                      ? "min-h-24"
                                      : "min-h-40",
                                activePreview.src && "cursor-zoom-in",
                            )}
                            aria-label={
                                activePreview.src
                                    ? `放大预览 ${activePreview.name}`
                                    : activePreview.name
                            }
                            onClick={() => {
                                if (activePreview.src) setPreviewOpen(true)
                            }}
                        >
                            {activePreview.src ? (
                                <img
                                    src={activePreview.src}
                                    alt={activePreview.name}
                                    className={cn(
                                        "absolute inset-0 size-full",
                                        previewFit === "contain"
                                            ? "object-contain p-2"
                                            : "object-cover",
                                    )}
                                />
                            ) : (
                                <div className="flex min-w-0 flex-col items-center gap-1 p-2 text-muted-foreground">
                                    <ImageIcon
                                        className="size-5"
                                        aria-hidden="true"
                                    />
                                    <span className="line-clamp-2 break-all text-2xs leading-tight">
                                        {activePreview.name}
                                    </span>
                                </div>
                            )}
                            {activePreview.src ? (
                                <span className="absolute inset-0 flex items-center justify-center bg-overlay/0 text-primary-foreground opacity-0 transition group-hover:bg-overlay/50 group-hover:opacity-100 group-focus-visible:bg-overlay/50 group-focus-visible:opacity-100">
                                    <Maximize2Icon
                                        className="size-4"
                                        aria-hidden="true"
                                    />
                                </span>
                            ) : null}
                        </button>
                        <span className="absolute bottom-1 left-1 rounded-md bg-background/90 px-1 text-2xs font-medium text-foreground shadow-xs">
                            {previewStatus}
                        </span>
                        {!readOnly ? (
                            <Button
                                id={
                                    selectId ??
                                    (idPrefix
                                        ? `${idPrefix}-select`
                                        : undefined)
                                }
                                type="button"
                                variant="secondary"
                                size="icon-xs"
                                className="absolute left-1 top-1 size-5"
                                disabled={disabled}
                                aria-label={fileUploadText.replace}
                                onClick={() => inputRef.current?.click()}
                            >
                                <RefreshCwIcon className="size-3" />
                            </Button>
                        ) : null}
                        {!readOnly && onPreviewRemove ? (
                            <Button
                                id={idPrefix ? `${idPrefix}-remove` : undefined}
                                type="button"
                                variant="secondary"
                                size="icon-xs"
                                className="absolute right-1 top-1 size-5"
                                disabled={disabled}
                                aria-label={`移除 ${activePreview.name}`}
                                onClick={removePreview}
                            >
                                <XIcon className="size-3" />
                            </Button>
                        ) : null}
                    </>
                ) : (
                    <>
                        {density !== "tile" && density !== "button" ? (
                            <div
                                className={cn(
                                    "flex shrink-0 items-center justify-center bg-muted text-foreground",
                                    density === "compact"
                                        ? "size-8 rounded-lg"
                                        : "size-10 rounded-xl",
                                )}
                            >
                                <UploadCloudIcon
                                    className={
                                        density === "compact"
                                            ? "size-4"
                                            : "size-5"
                                    }
                                    aria-hidden="true"
                                />
                            </div>
                        ) : null}
                        <div
                            className={cn(
                                density === "compact" && "min-w-0 flex-1",
                                density === "tile" && "min-w-0 leading-none",
                            )}
                        >
                            <div
                                className={cn(
                                    "font-medium text-foreground",
                                    density === "hero" &&
                                        "text-base sm:text-lg",
                                    density === "tile" || density === "button"
                                        ? "sr-only"
                                        : density === "compact"
                                          ? "text-xs"
                                          : density === "hero"
                                            ? undefined
                                            : "text-sm",
                                )}
                            >
                                {label}
                            </div>
                            <div
                                id={descriptionId}
                                className={cn(
                                    "text-xs text-muted-foreground",
                                    density === "tile"
                                        ? "sr-only"
                                        : density === "compact"
                                          ? "break-words"
                                          : "mt-1",
                                )}
                            >
                                {description}
                            </div>
                        </div>
                        <Button
                            id={
                                selectId ??
                                (idPrefix ? `${idPrefix}-select` : undefined)
                            }
                            type="button"
                            variant={
                                density === "hero"
                                    ? "default"
                                    : density === "button"
                                      ? "ghost"
                                      : "outline"
                            }
                            className={
                                density === "button" ? "order-first" : undefined
                            }
                            size={
                                density === "hero"
                                    ? "lg"
                                    : density === "tile"
                                      ? "icon-xs"
                                      : density === "compact"
                                        ? "xs"
                                        : "sm"
                            }
                            disabled={disabled || readOnly}
                            aria-describedby={describedBy}
                            aria-label={
                                density === "tile"
                                    ? `选择${typeof label === "string" ? label : "上传文件"}`
                                    : undefined
                            }
                            onClick={() => inputRef.current?.click()}
                        >
                            <FileUpIcon
                                data-icon={
                                    density === "tile"
                                        ? undefined
                                        : "inline-start"
                                }
                                aria-hidden="true"
                            />
                            {density === "tile" ? (
                                <span className="sr-only">选择文件</span>
                            ) : (
                                (selectLabel ??
                                (selectedFiles?.length
                                    ? multiple
                                        ? fileUploadText.add
                                        : fileUploadText.replace
                                    : fileUploadText.select))
                            )}
                        </Button>
                    </>
                )}
            </div>

            {selectionError ? (
                <p
                    id={errorId}
                    role="alert"
                    className="text-sm text-destructive"
                >
                    {selectionError}
                </p>
            ) : null}
            {activePreview?.src ? (
                <Dialog open={previewOpen} onOpenChange={setPreviewOpen}>
                    <DialogContent
                        closeButtonId={
                            idPrefix ? `${idPrefix}-preview-close` : undefined
                        }
                        className="gap-4 p-4 sm:max-w-4xl"
                    >
                        <DialogHeader className="pr-10">
                            <DialogTitle>{activePreview.name}</DialogTitle>
                            <DialogDescription>
                                {previewStatus} · 图片预览
                            </DialogDescription>
                        </DialogHeader>
                        <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-lg bg-surface-sunken">
                            <img
                                src={activePreview.src}
                                alt={activePreview.name}
                                className="max-h-[75dvh] max-w-full object-contain"
                            />
                        </div>
                    </DialogContent>
                </Dialog>
            ) : null}
        </div>
    )
}

export { FileUpload, imagePreviewSource, type FileUploadImagePreview }
