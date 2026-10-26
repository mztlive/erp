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
    status?: "uploaded" | "pending" | "saved"
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
    previewActions = "default",
    showPreviewStatus = true,
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
    controlClassName,
    showTileLabel = false,
    selectedDescription = fileUploadText.selected,
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
    /** 预览更换/移除动作的位置；overlay 适合窄缩略图。 */
    previewActions?: "default" | "overlay"
    /** 是否显示预览状态文案（已上传、已保存等）。 */
    showPreviewStatus?: boolean
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
    /** 只约束上传框尺寸；错误与预览操作保留在框外。 */
    controlClassName?: string
    /** 大尺寸图片入口显示动作和说明，小缩略图入口保持紧凑。 */
    showTileLabel?: boolean
    /** 本地文件下一步的业务动作，由保存或导入入口提供。 */
    selectedDescription?: string
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
        if (
            disabled ||
            readOnly ||
            inputRef.current?.matches(":disabled") ||
            !files?.length
        )
            return
        const selectedFiles = Array.from(files)
        const error =
            fileSelectionError(selectedFiles, {
                accept,
                multiple,
                maxFileSize,
            }) || validateFiles?.(selectedFiles)
        setSelectionError(
            error
                ? `${error}${multiple && selectedFiles.length > 1 ? ` ${fileUploadText.batchRejected}` : ""}`
                : null,
        )
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
            ? selectedDescription
            : activePreview?.status === "saved"
              ? fileUploadText.saved
              : fileUploadText.uploaded

    const hasSelection = Boolean(selectedFiles?.length && !activePreview)
    const effectiveDensity = hasSelection ? "button" : density

    const removePreview = () => {
        setSelectionError(null)
        setPreviewOpen(false)
        clearLocalPreview()
        onPreviewRemove?.()
    }

    const previewActionButtons = (
        <>
            {!readOnly ? (
                <Button
                    id={
                        selectId ??
                        (idPrefix ? `${idPrefix}-select` : undefined)
                    }
                    type="button"
                    variant="secondary"
                    size={previewActions === "overlay" ? "icon-xs" : "icon-sm"}
                    title={fileUploadText.replace}
                    disabled={disabled}
                    aria-describedby={describedBy}
                    aria-invalid={
                        Boolean(selectionError) ||
                        ariaInvalid === true ||
                        ariaInvalid === "true"
                    }
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
                    size={previewActions === "overlay" ? "icon-xs" : "icon-sm"}
                    title={`${fileUploadText.remove} ${activePreview?.name ?? ""}`}
                    disabled={disabled}
                    aria-label={`移除 ${activePreview?.name ?? ""}`}
                    onClick={removePreview}
                >
                    <XIcon className="size-3" />
                </Button>
            ) : null}
        </>
    )

    return (
        <div className={cn("min-w-0 space-y-2", className)}>
            {selectedFiles?.length && !activePreview ? (
                <FileUploadSelection
                    files={selectedFiles}
                    idPrefix={idPrefix}
                    multiple={multiple}
                    disabled={disabled}
                    readOnly={readOnly}
                    fileKey={fileKey}
                    description={selectedDescription}
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
                    "relative flex min-w-0 items-center justify-center rounded-lg border border-dashed bg-surface-sunken transition-colors data-dragging:border-ring data-dragging:bg-accent",
                    effectiveDensity === "hero"
                        ? "min-h-64 flex-col gap-5 px-5 py-10 text-center sm:min-h-72 lg:min-h-90"
                        : effectiveDensity === "button"
                          ? "flex-row flex-wrap justify-start gap-3 border-0 bg-transparent p-0 text-left"
                          : effectiveDensity === "tile"
                            ? "aspect-square w-full flex-col gap-2 p-2 text-center"
                            : effectiveDensity === "compact"
                              ? "flex-row gap-2 p-2 text-left"
                              : "flex-col gap-3 p-6 text-center",
                    activePreview && "overflow-hidden p-0",
                    !activePreview &&
                        effectiveDensity !== "button" &&
                        "has-[button:enabled:hover]:border-ring has-[button:enabled:hover]:bg-accent has-[button:focus-visible]:border-ring has-[button:focus-visible]:ring-2 has-[button:focus-visible]:ring-ring",
                    readOnly && !activePreview && "hidden",
                    disabled && "cursor-not-allowed opacity-50",
                    controlClassName,
                )}
                onDragEnter={(event) => {
                    event.preventDefault()
                    if (
                        !disabled &&
                        !readOnly &&
                        !inputRef.current?.matches(":disabled") &&
                        event.dataTransfer.types.includes("Files")
                    )
                        setDragging(true)
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
                    hidden
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
                                    ? "aspect-square w-full"
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
                        {previewActions === "overlay" && !readOnly ? (
                            <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex items-center justify-end gap-1 bg-gradient-to-t from-overlay/70 via-overlay/20 to-transparent p-1">
                                <div className="pointer-events-auto flex items-center gap-1">
                                    {previewActionButtons}
                                </div>
                            </div>
                        ) : null}
                    </>
                ) : (
                    <>
                        {effectiveDensity !== "tile" &&
                        effectiveDensity !== "button" ? (
                            <div
                                className={cn(
                                    "flex shrink-0 items-center justify-center bg-muted text-foreground",
                                    effectiveDensity === "compact"
                                        ? "size-8 rounded-lg"
                                        : "size-10 rounded-xl",
                                )}
                            >
                                <UploadCloudIcon
                                    className={
                                        effectiveDensity === "compact"
                                            ? "size-4"
                                            : "size-5"
                                    }
                                    aria-hidden="true"
                                />
                            </div>
                        ) : null}
                        <div
                            className={cn(
                                effectiveDensity === "compact" &&
                                    "min-w-0 flex-1",
                                effectiveDensity === "tile" &&
                                    "min-w-0 leading-none",
                            )}
                        >
                            <div
                                className={cn(
                                    "font-medium text-foreground",
                                    effectiveDensity === "hero" &&
                                        "text-base sm:text-lg",
                                    (effectiveDensity === "tile" &&
                                        !showTileLabel) ||
                                        effectiveDensity === "button"
                                        ? "sr-only"
                                        : effectiveDensity === "compact"
                                          ? "text-xs"
                                          : effectiveDensity === "hero"
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
                                    effectiveDensity === "tile" &&
                                        !showTileLabel
                                        ? "sr-only"
                                        : effectiveDensity === "compact"
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
                                effectiveDensity === "hero"
                                    ? "default"
                                    : effectiveDensity === "button"
                                      ? "ghost"
                                      : "outline"
                            }
                            // 扩展原生按钮的命中区域，保留键盘与 fieldset 禁用语义。
                            // 不使用按压位移，避免按钮成为伪元素的定位容器。
                            className={cn(
                                effectiveDensity === "button"
                                    ? "order-first"
                                    : "static cursor-pointer after:absolute after:inset-0 after:rounded-lg active:not-aria-[haspopup]:translate-none",
                            )}
                            size={
                                effectiveDensity === "hero"
                                    ? "lg"
                                    : effectiveDensity === "tile" &&
                                        !showTileLabel
                                      ? "icon-sm"
                                      : effectiveDensity === "compact"
                                        ? "xs"
                                        : "sm"
                            }
                            disabled={disabled || readOnly}
                            aria-describedby={describedBy}
                            aria-invalid={
                                Boolean(selectionError) ||
                                ariaInvalid === true ||
                                ariaInvalid === "true"
                            }
                            aria-label={
                                effectiveDensity === "tile"
                                    ? `选择${typeof label === "string" ? label : "上传文件"}`
                                    : undefined
                            }
                            onClick={() => inputRef.current?.click()}
                        >
                            <FileUpIcon
                                data-icon={
                                    effectiveDensity === "tile"
                                        ? undefined
                                        : "inline-start"
                                }
                                aria-hidden="true"
                            />
                            {effectiveDensity === "tile" && !showTileLabel ? (
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

            {activePreview &&
            previewActions === "default" &&
            (showPreviewStatus || !readOnly) ? (
                <div className="flex flex-wrap items-center justify-between gap-1">
                    {showPreviewStatus ? (
                        <span className="min-w-0 flex-1 basis-full break-words text-xs text-muted-foreground sm:basis-auto">
                            {previewStatus}
                        </span>
                    ) : null}
                    {previewActionButtons}
                </div>
            ) : null}
            {readOnly && !activePreview && !hasSelection ? (
                <p className="text-sm text-muted-foreground">
                    {fileUploadText.empty}
                </p>
            ) : null}

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
