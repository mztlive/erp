"use client"

/* eslint-disable @next/next/no-img-element -- 本地待上传图片使用 blob URL，无法经 Next Image 优化。 */

import * as React from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    ArrowUpRightIcon,
    GripVerticalIcon,
    ImageIcon,
    Maximize2Icon,
    PlusIcon,
    StarIcon,
    XIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FileUpload, imagePreviewSource } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { moveListItem } from "@/features/master-data/lib/move-list-item"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fileNameConflictError, fileSelectionError } from "@/lib/file-selection"
import { fileUploadText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

function MoneyInput({
    value,
    onChange,
    disabled = false,
    align = "start",
    "aria-label": ariaLabel,
    id,
}: {
    value: string
    onChange: (next: string) => void
    disabled?: boolean
    align?: "start" | "end"
    "aria-label": string
    id?: string
}) {
    const showPrefix = !value.trim().startsWith("¥")
    return (
        <div className="relative">
            {showPrefix ? (
                <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-xs text-muted-foreground/70">
                    ¥
                </span>
            ) : null}
            <Input
                id={id}
                inputMode="decimal"
                className={cn(
                    "h-control-sm num",
                    showPrefix && "pl-6",
                    align === "end" && "text-right",
                )}
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

function useMediaLocalPreviews(
    value: readonly string[],
    previewUrls?: Readonly<Record<string, string>>,
) {
    const [localPreviewUrls, setLocalPreviewUrls] = React.useState<
        ReadonlyMap<string, string>
    >(() => new Map())
    const localPreviewUrlsRef = React.useRef<ReadonlyMap<string, string>>(
        new Map(),
    )

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

    const registerFiles = React.useCallback(
        (files: File[]) => {
            const addedUrls: Record<string, string> = {}
            updateLocalPreviewUrls((previous) => {
                const next = new Map(previous)
                for (const file of files) {
                    const previousSrc = next.get(file.name)
                    if (previousSrc) URL.revokeObjectURL(previousSrc)
                    const blobUrl = URL.createObjectURL(file)
                    next.set(file.name, blobUrl)
                    addedUrls[file.name] = blobUrl
                }
                return next
            })
            return addedUrls
        },
        [updateLocalPreviewUrls],
    )

    const resolveSrc = React.useCallback(
        (name: string) =>
            localPreviewUrls.get(name) ??
            previewUrls?.[name] ??
            imagePreviewSource(name),
        [localPreviewUrls, previewUrls],
    )

    return { resolveSrc, registerFiles }
}

/**
 * 现代画廊式多图管理器：
 * - 极净静止态（去除臃肿底栏与冗余「已保存」脚标，仅本地待保存图显琥珀角标）；
 * - 支持直接拖拽排序 + 悬停柔和浮层操作（放大 / 设首图 / 前后微调 / 删除）；
 * - 保留所有既有自动化 DOM ID。
 */
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
    gridClassName,
    onActiveImageChange,
}: {
    idPrefix?: string
    label: string
    hint?: string
    value: readonly string[]
    onChange: (next: string[]) => void
    previewUrls?: Readonly<Record<string, string>>
    onPreviewUrlsChange?: (next: Record<string, string>) => void
    onFilesSelected?: (files: File[]) => void
    mode?: "carousel" | "detail"
    readOnly?: boolean
    disabled?: boolean
    existingFileNames?: readonly string[]
    gridClassName?: string
    onActiveImageChange?: (index: number | null) => void
}) {
    const { resolveSrc, registerFiles } = useMediaLocalPreviews(
        value,
        previewUrls,
    )
    const [expandedPreview, setExpandedPreview] = React.useState<{
        name: string
        src: string
    } | null>(null)
    const [dragIndex, setDragIndex] = React.useState<number | null>(null)
    const [dropIndex, setDropIndex] = React.useState<number | null>(null)

    const derivedBasePrefix =
        idPrefix ?? `master-data-product-media-${toAutomationIdSegment(label)}`
    const canReorder = !readOnly && !disabled && value.length > 1

    const handleDropOnTile = (targetIndex: number) => {
        if (dragIndex === null || dragIndex === targetIndex) {
            setDragIndex(null)
            setDropIndex(null)
            return
        }
        onChange(moveListItem(value, dragIndex, targetIndex))
        setDragIndex(null)
        setDropIndex(null)
    }

    return (
        <div className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        {label}
                    </h2>
                    <span className="num rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                        {masterDataCopy.mediaCount(value.length)}
                    </span>
                    <p className="text-xs text-muted-foreground">
                        {hint ?? masterDataCopy.mediaUploadHint}
                    </p>
                </div>
            </div>

            <div
                className={cn(
                    "grid gap-3",
                    gridClassName ??
                        (mode === "carousel"
                            ? "grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-7"
                            : "grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-4"),
                )}
            >
                {value.map((name, index) => {
                    const previewSrc = resolveSrc(name)
                    const isPending = Boolean(previewSrc?.startsWith("blob:"))
                    const itemSegment = toAutomationIdSegment(
                        name || `item-${index}`,
                    )
                    const base = `${derivedBasePrefix}-${itemSegment}`
                    const isDragging = dragIndex === index
                    const isDropTarget =
                        dropIndex === index &&
                        dragIndex !== null &&
                        dragIndex !== index

                    return (
                        <div
                            key={`${name}-${index}`}
                            draggable={canReorder}
                            onDragStart={(event) => {
                                if (!canReorder) return
                                setDragIndex(index)
                                event.dataTransfer.effectAllowed = "move"
                                event.dataTransfer.setData(
                                    "text/plain",
                                    String(index),
                                )
                            }}
                            onDragOver={(event) => {
                                if (dragIndex === null) return
                                event.preventDefault()
                                event.dataTransfer.dropEffect = "move"
                                if (dropIndex !== index) setDropIndex(index)
                            }}
                            onDragLeave={() => {
                                if (dropIndex === index) setDropIndex(null)
                            }}
                            onDrop={(event) => {
                                if (dragIndex === null) return
                                event.preventDefault()
                                handleDropOnTile(index)
                            }}
                            onDragEnd={() => {
                                setDragIndex(null)
                                setDropIndex(null)
                            }}
                            onMouseEnter={() => onActiveImageChange?.(index)}
                            onMouseLeave={() => onActiveImageChange?.(null)}
                            className={cn(
                                "group/tile relative overflow-hidden rounded-lg border border-border/75 bg-surface-sunken/50 transition-all duration-150",
                                canReorder &&
                                    "cursor-grab active:cursor-grabbing",
                                isDragging && "scale-[0.97] opacity-40",
                                isDropTarget &&
                                    "border-primary ring-2 ring-primary/35",
                            )}
                        >
                            <button
                                id={`${base}-preview`}
                                type="button"
                                className={cn(
                                    "relative flex w-full flex-col items-center justify-center gap-1.5 overflow-hidden bg-muted/30 p-3 text-center focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                                    mode === "carousel"
                                        ? "aspect-square"
                                        : "aspect-[3/4]",
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
                                    <img
                                        src={previewSrc}
                                        alt={name}
                                        className="absolute inset-0 size-full object-cover transition-transform duration-200 group-hover/tile:scale-[1.03]"
                                    />
                                ) : (
                                    <>
                                        <ImageIcon
                                            className="size-6 text-muted-foreground/70"
                                            aria-hidden
                                        />
                                        <span className="line-clamp-2 break-all text-xs text-muted-foreground">
                                            {name}
                                        </span>
                                    </>
                                )}
                            </button>

                            {/* 左上角角标：首图徽章或柔和序号 */}
                            <div className="pointer-events-none absolute left-1.5 top-1.5 z-10 flex items-center gap-1">
                                {mode === "carousel" && index === 0 ? (
                                    <Badge className="h-5 rounded-sm bg-primary/90 px-1.5 text-xs font-medium text-primary-foreground shadow-xs">
                                        首图
                                    </Badge>
                                ) : (
                                    <span className="num inline-flex h-5 min-w-5 items-center justify-center rounded-sm bg-foreground/65 px-1 text-tiny font-medium text-card backdrop-blur-xs">
                                        {index + 1}
                                    </span>
                                )}
                                {isPending ? (
                                    <span className="inline-flex h-5 items-center rounded-sm bg-warning px-1.5 text-tiny font-medium text-warning-foreground shadow-xs">
                                        待保存
                                    </span>
                                ) : null}
                            </div>

                            {/* 右上角删除按钮（悬停/聚焦显现） */}
                            {!readOnly ? (
                                <div className="absolute right-1.5 top-1.5 z-10 flex items-center gap-1 opacity-0 transition-opacity duration-150 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100 [@media(hover:none)]:opacity-100">
                                    {canReorder ? (
                                        <span
                                            aria-hidden="true"
                                            title="拖拽可调整顺序"
                                            className="inline-flex size-6 items-center justify-center rounded-md bg-card/85 text-muted-foreground shadow-xs backdrop-blur-xs"
                                        >
                                            <GripVerticalIcon className="size-3.5" />
                                        </span>
                                    ) : null}
                                    <Button
                                        id={`${base}-remove`}
                                        disabled={disabled}
                                        type="button"
                                        variant="secondary"
                                        size="icon-xs"
                                        className="size-6 rounded-md bg-card/90 text-foreground shadow-xs backdrop-blur-xs hover:bg-destructive hover:text-destructive-foreground"
                                        aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                                        title="移除图片"
                                        onClick={() =>
                                            onChange(
                                                value.filter(
                                                    (_, i) => i !== index,
                                                ),
                                            )
                                        }
                                    >
                                        <XIcon className="size-3" />
                                    </Button>
                                </div>
                            ) : null}

                            {/* 底部悬停轻浮层：排序与设为首图（静止态隐藏，画面干干净净） */}
                            {!readOnly ? (
                                <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex items-center justify-between gap-1 bg-gradient-to-t from-foreground/75 via-foreground/40 to-transparent px-1.5 pb-1.5 pt-5 opacity-0 transition-opacity duration-150 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100 [@media(hover:none)]:opacity-100">
                                    <div className="pointer-events-auto flex items-center gap-0.5">
                                        <Button
                                            id={`${base}-move-up`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            className="size-6 rounded-sm text-card hover:bg-card/20 hover:text-card disabled:opacity-35"
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
                                            <ArrowLeftIcon className="size-3" />
                                        </Button>
                                        <Button
                                            id={`${base}-move-down`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            className="size-6 rounded-sm text-card hover:bg-card/20 hover:text-card disabled:opacity-35"
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
                                            <ArrowRightIcon className="size-3" />
                                        </Button>
                                    </div>
                                    {mode === "carousel" && index > 0 ? (
                                        <Button
                                            id={`${base}-set-cover`}
                                            disabled={disabled}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className="pointer-events-auto h-6 rounded-sm px-1.5 text-xs font-medium text-card hover:bg-card/20 hover:text-card"
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
                                            <StarIcon
                                                className="size-3"
                                                aria-hidden
                                            />
                                            首图
                                        </Button>
                                    ) : null}
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
                                ? "点击或拖入多图"
                                : "点击或拖入长图"
                        }
                        onFilesSelected={(files) => {
                            onFilesSelected?.(files)
                            const addedUrls = registerFiles(files)
                            if (onPreviewUrlsChange) {
                                onPreviewUrlsChange({
                                    ...previewUrls,
                                    ...addedUrls,
                                })
                            }
                            onChange([...value, ...files.map((f) => f.name)])
                        }}
                        controlClassName={cn(
                            "border-border/80 bg-muted/25 hover:border-primary/50 hover:bg-sidebar-accent/30",
                            mode === "carousel"
                                ? "aspect-square"
                                : "aspect-[3/4]",
                        )}
                    />
                ) : null}
            </div>

            {value.length === 0 && readOnly ? (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.mediaEmpty}
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
                    </DialogHeader>
                    <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-lg bg-surface-sunken">
                        {expandedPreview ? (
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

/**
 * 首屏「商品资料」右侧媒体栏：
 * 让用户在第一眼就能直接查看与维护商品轮播主图（支持快捷上传、设首图、拖拽排序与放大），
 * 并提供详情图数量与快捷跳转至「图片与详情」手机长图预览工作台。
 */
function ProductBasicMediaRail({
    idPrefix = "master-data-product-basic-media",
    carouselImages,
    carouselPreviewUrls,
    detailImages,
    detailPreviewUrls,
    canRevise,
    disabled = false,
    onCarouselChange,
    onCarouselPreviewUrlsChange,
    onFilesSelected,
    onOpenMediaTab,
}: {
    idPrefix?: string
    carouselImages: readonly string[]
    carouselPreviewUrls: Readonly<Record<string, string>>
    detailImages: readonly string[]
    detailPreviewUrls: Readonly<Record<string, string>>
    canRevise: boolean
    disabled?: boolean
    onCarouselChange: (next: string[]) => void
    onCarouselPreviewUrlsChange: (next: Record<string, string>) => void
    onFilesSelected: (files: File[]) => void
    onOpenMediaTab: () => void
}) {
    const inputRef = React.useRef<HTMLInputElement>(null)
    const { resolveSrc, registerFiles } = useMediaLocalPreviews(
        carouselImages,
        carouselPreviewUrls,
    )
    const [uploadError, setUploadError] = React.useState<string | null>(null)
    const [expandedPreview, setExpandedPreview] = React.useState<{
        name: string
        src: string
    } | null>(null)
    const [dragIndex, setDragIndex] = React.useState<number | null>(null)
    const [dropIndex, setDropIndex] = React.useState<number | null>(null)

    const coverName = carouselImages[0]
    const coverSrc = coverName ? resolveSrc(coverName) : undefined
    const editable = canRevise && !disabled
    const existingNames = React.useMemo(
        () => [...carouselImages, ...detailImages],
        [carouselImages, detailImages],
    )

    const handleSelectFiles = (fileList: FileList | null) => {
        if (!editable || !fileList?.length) return
        const files = Array.from(fileList)
        const error =
            fileSelectionError(files, {
                accept: "image/jpeg,image/png,image/webp",
                multiple: true,
            }) || fileNameConflictError(files, existingNames)
        setUploadError(error ?? null)
        if (error) return
        onFilesSelected(files)
        const addedUrls = registerFiles(files)
        onCarouselPreviewUrlsChange({ ...carouselPreviewUrls, ...addedUrls })
        onCarouselChange([...carouselImages, ...files.map((f) => f.name)])
    }

    const handleDropTile = (targetIndex: number) => {
        if (dragIndex === null || dragIndex === targetIndex) {
            setDragIndex(null)
            setDropIndex(null)
            return
        }
        onCarouselChange(moveListItem(carouselImages, dragIndex, targetIndex))
        setDragIndex(null)
        setDropIndex(null)
    }

    return (
        <aside
            aria-label="商品图片速览"
            className="min-w-0 space-y-4 xl:border-l xl:border-border/60 xl:pl-7"
        >
            <div className="flex items-center justify-between gap-2">
                <div className="flex items-baseline gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        商品轮播图
                    </h2>
                    <span className="num rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                        {carouselImages.length} 张
                    </span>
                </div>
                <Button
                    id={`${idPrefix}-manage`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-6 px-1.5 text-xs font-normal text-muted-foreground hover:bg-muted/70 hover:text-primary"
                    onClick={onOpenMediaTab}
                >
                    全部图片
                    <ArrowUpRightIcon
                        data-icon="inline-end"
                        className="size-3"
                        aria-hidden
                    />
                </Button>
            </div>

            <input
                id={`${idPrefix}-upload-input`}
                ref={inputRef}
                type="file"
                accept="image/jpeg,image/png,image/webp"
                multiple
                hidden
                disabled={!editable}
                onChange={(event) => {
                    handleSelectFiles(event.target.files)
                    event.target.value = ""
                }}
            />

            {carouselImages.length > 0 ? (
                <div className="flex items-start gap-3">
                    {/* 首图主预览 */}
                    <div className="group/cover relative size-28 shrink-0 overflow-hidden rounded-lg border border-border/80 bg-muted/30">
                        <button
                            id={`${idPrefix}-cover-preview`}
                            type="button"
                            className="relative block size-full cursor-zoom-in focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
                            aria-label={`放大预览首图 ${coverName}`}
                            onClick={() => {
                                if (coverSrc && coverName) {
                                    setExpandedPreview({
                                        name: coverName,
                                        src: coverSrc,
                                    })
                                }
                            }}
                        >
                            {coverSrc ? (
                                <img
                                    src={coverSrc}
                                    alt={coverName}
                                    className="size-full object-cover"
                                />
                            ) : (
                                <span className="flex size-full items-center justify-center text-muted-foreground">
                                    <ImageIcon className="size-6" aria-hidden />
                                </span>
                            )}
                            <span className="absolute inset-0 flex items-center justify-center bg-foreground/0 text-card opacity-0 transition group-hover/cover:bg-foreground/30 group-hover/cover:opacity-100">
                                <Maximize2Icon className="size-4" aria-hidden />
                            </span>
                        </button>
                        <span className="pointer-events-none absolute left-1.5 top-1.5 rounded-sm bg-primary/90 px-1.5 py-0.5 text-tiny leading-none font-medium text-primary-foreground">
                            首图
                        </span>
                        {editable ? (
                            <Button
                                id={`${idPrefix}-cover-remove`}
                                type="button"
                                variant="secondary"
                                size="icon-xs"
                                className="absolute right-1 top-1 size-5 rounded-sm bg-card/90 text-foreground opacity-0 shadow-xs transition-opacity group-hover/cover:opacity-100 group-focus-within/cover:opacity-100 hover:bg-destructive hover:text-destructive-foreground [@media(hover:none)]:opacity-100"
                                aria-label={`移除首图 ${coverName}`}
                                onClick={() =>
                                    onCarouselChange(carouselImages.slice(1))
                                }
                            >
                                <XIcon className="size-3" />
                            </Button>
                        ) : null}
                    </div>

                    {/* 其余轮播缩略图 + 快捷添加 */}
                    <div className="grid min-w-0 flex-1 grid-cols-3 gap-2">
                        {carouselImages.slice(1, 6).map((name, offset) => {
                            const index = offset + 1
                            const src = resolveSrc(name)
                            const seg = toAutomationIdSegment(
                                name || `item-${index}`,
                            )
                            return (
                                <div
                                    key={`${name}-${index}`}
                                    draggable={editable}
                                    onDragStart={(event) => {
                                        if (!editable) return
                                        setDragIndex(index)
                                        event.dataTransfer.effectAllowed =
                                            "move"
                                    }}
                                    onDragOver={(event) => {
                                        if (dragIndex === null) return
                                        event.preventDefault()
                                        if (dropIndex !== index)
                                            setDropIndex(index)
                                    }}
                                    onDrop={(event) => {
                                        if (dragIndex === null) return
                                        event.preventDefault()
                                        handleDropTile(index)
                                    }}
                                    onDragEnd={() => {
                                        setDragIndex(null)
                                        setDropIndex(null)
                                    }}
                                    className={cn(
                                        "group/thumb relative aspect-square overflow-hidden rounded-md border border-border/75 bg-muted/30",
                                        editable && "cursor-grab",
                                        dropIndex === index &&
                                            dragIndex !== index &&
                                            "border-primary ring-2 ring-primary/30",
                                    )}
                                >
                                    <button
                                        id={`${idPrefix}-thumb-${seg}-preview`}
                                        type="button"
                                        className="block size-full cursor-zoom-in focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
                                        aria-label={`预览 ${name}`}
                                        onClick={() => {
                                            if (src)
                                                setExpandedPreview({
                                                    name,
                                                    src,
                                                })
                                        }}
                                    >
                                        {src ? (
                                            <img
                                                src={src}
                                                alt={name}
                                                className="size-full object-cover"
                                            />
                                        ) : (
                                            <span className="flex size-full items-center justify-center text-muted-foreground/70">
                                                <ImageIcon
                                                    className="size-4"
                                                    aria-hidden
                                                />
                                            </span>
                                        )}
                                    </button>
                                    {editable ? (
                                        <div className="absolute inset-x-0 bottom-0 flex items-center justify-between bg-foreground/65 px-1 py-0.5 opacity-0 transition-opacity group-hover/thumb:opacity-100 group-focus-within/thumb:opacity-100 [@media(hover:none)]:opacity-100">
                                            <button
                                                id={`${idPrefix}-thumb-${seg}-cover`}
                                                type="button"
                                                title="设为首图"
                                                aria-label={`将 ${name} 设为首图`}
                                                className="inline-flex size-4 items-center justify-center rounded-xs text-card hover:text-warning"
                                                onClick={() =>
                                                    onCarouselChange(
                                                        moveListItem(
                                                            carouselImages,
                                                            index,
                                                            0,
                                                        ),
                                                    )
                                                }
                                            >
                                                <StarIcon className="size-3" />
                                            </button>
                                            <button
                                                id={`${idPrefix}-thumb-${seg}-remove`}
                                                type="button"
                                                title="移除"
                                                aria-label={`移除 ${name}`}
                                                className="inline-flex size-4 items-center justify-center rounded-xs text-card hover:text-destructive-soft"
                                                onClick={() =>
                                                    onCarouselChange(
                                                        carouselImages.filter(
                                                            (_, i) =>
                                                                i !== index,
                                                        ),
                                                    )
                                                }
                                            >
                                                <XIcon className="size-3" />
                                            </button>
                                        </div>
                                    ) : null}
                                </div>
                            )
                        })}
                        {carouselImages.length > 6 ? (
                            <button
                                id={`${idPrefix}-more`}
                                type="button"
                                onClick={onOpenMediaTab}
                                className="num flex aspect-square items-center justify-center rounded-md border border-border/75 bg-muted/45 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                            >
                                +{carouselImages.length - 6}
                            </button>
                        ) : null}
                        {editable ? (
                            <button
                                id={`${idPrefix}-add`}
                                type="button"
                                onClick={() => inputRef.current?.click()}
                                onDragOver={(event) => event.preventDefault()}
                                onDrop={(event) => {
                                    event.preventDefault()
                                    handleSelectFiles(event.dataTransfer.files)
                                }}
                                aria-label="添加商品轮播图"
                                title="添加轮播图"
                                className="flex aspect-square flex-col items-center justify-center gap-0.5 rounded-md border border-dashed border-border/90 bg-muted/25 text-muted-foreground transition-colors hover:border-primary/50 hover:bg-sidebar-accent/35 hover:text-primary"
                            >
                                <PlusIcon className="size-4" aria-hidden />
                                <span className="text-xs">上传</span>
                            </button>
                        ) : null}
                    </div>
                </div>
            ) : editable ? (
                <button
                    id={`${idPrefix}-empty-upload`}
                    type="button"
                    onClick={() => inputRef.current?.click()}
                    onDragOver={(event) => event.preventDefault()}
                    onDrop={(event) => {
                        event.preventDefault()
                        handleSelectFiles(event.dataTransfer.files)
                    }}
                    className="flex h-28 w-full flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed border-border/90 bg-muted/25 px-4 text-center transition-colors hover:border-primary/50 hover:bg-sidebar-accent/30"
                >
                    <PlusIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden
                    />
                    <span className="text-body-compact font-medium text-foreground/80">
                        添加商品轮播图
                    </span>
                    <span className="text-xs text-muted-foreground">
                        点击或拖入多张图片，首张将作为商品主图
                    </span>
                </button>
            ) : (
                <div className="flex h-24 items-center justify-center rounded-lg bg-muted/30 text-xs text-muted-foreground">
                    暂未上传商品轮播图
                </div>
            )}

            {uploadError ? (
                <p role="alert" className="text-xs text-destructive">
                    {uploadError}
                </p>
            ) : null}

            {/* 商品详情长图紧凑速览条 */}
            <div className="flex items-center justify-between gap-3 border-t border-border/50 pt-3.5">
                <div className="min-w-0 space-y-1.5">
                    <div className="flex items-center gap-2">
                        <span className="text-xs font-medium text-foreground/80">
                            商品详情图
                        </span>
                        <span className="num text-xs text-muted-foreground">
                            {detailImages.length} 张
                        </span>
                    </div>
                    {detailImages.length > 0 ? (
                        <div className="flex items-center gap-1.5">
                            {detailImages.slice(0, 5).map((name, index) => {
                                const src =
                                    detailPreviewUrls[name] ??
                                    imagePreviewSource(name)
                                return (
                                    <span
                                        key={`${name}-${index}`}
                                        className="block h-10 w-8 overflow-hidden rounded-sm border border-border/75 bg-muted/35"
                                    >
                                        {src ? (
                                            <img
                                                src={src}
                                                alt={name}
                                                className="size-full object-cover"
                                            />
                                        ) : null}
                                    </span>
                                )
                            })}
                            {detailImages.length > 5 ? (
                                <span className="num text-xs text-muted-foreground">
                                    +{detailImages.length - 5}
                                </span>
                            ) : null}
                        </div>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            暂未上传详情长图
                        </p>
                    )}
                </div>
                <Button
                    id={`${idPrefix}-open-detail-images`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-7 shrink-0 rounded-md bg-muted/65 px-2.5 text-xs font-normal text-foreground/80 hover:bg-muted hover:text-foreground"
                    onClick={onOpenMediaTab}
                >
                    {detailImages.length > 0 ? "排序与预览" : "去上传"}
                </Button>
            </div>

            <Dialog
                open={Boolean(expandedPreview)}
                onOpenChange={(open) => {
                    if (!open) setExpandedPreview(null)
                }}
            >
                <DialogContent
                    className="gap-4 p-4 sm:max-w-4xl"
                    closeButtonId={`${idPrefix}-preview-close`}
                >
                    <DialogHeader className="pr-10">
                        <DialogTitle>
                            {expandedPreview?.name ?? "图片预览"}
                        </DialogTitle>
                    </DialogHeader>
                    <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-lg bg-surface-sunken">
                        {expandedPreview ? (
                            <img
                                src={expandedPreview.src}
                                alt={expandedPreview.name}
                                className="max-h-[75dvh] max-w-full object-contain"
                            />
                        ) : null}
                    </div>
                </DialogContent>
            </Dialog>
        </aside>
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
    previewUrl?: string
    onChange: (next: string) => void
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
            className="w-12"
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

export { MediaListEditor, MoneyInput, ProductBasicMediaRail, SkuMainImageField }
