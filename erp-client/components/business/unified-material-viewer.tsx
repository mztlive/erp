"use client"

import {
    useEffect,
    useMemo,
    useRef,
    useState,
    type PointerEvent as ReactPointerEvent,
} from "react"
import { useQuery } from "@tanstack/react-query"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    DownloadIcon,
    EyeIcon,
    FileIcon,
    FileTextIcon,
    ImageIcon,
    Maximize2Icon,
    Minimize2Icon,
    RotateCwIcon,
    ScanIcon,
    XIcon,
    ZoomInIcon,
    ZoomOutIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { Spinner } from "@/components/ui/spinner"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    PdfDocumentCanvas,
    type PdfRotation,
} from "@/components/business/pdf-document-canvas"

export type MaterialFileItem = Readonly<{
    id: string
    name: string
    mimeType?: string
    sizeBytes?: number
    /** 由各业务域 apiGetBlob(...) 提供；预览器托管 ObjectURL 与 Blob.arrayBuffer 释放 */
    loadPreview: (signal: AbortSignal) => Promise<Blob>
    onDownload: () => Promise<void>
}>

export type UnifiedMaterialViewerProps = Readonly<{
    files: readonly MaterialFileItem[]
    activeFileId?: string
    onActiveFileChange?: (id: string) => void
    /** "docked" 嵌入工作台右栏做左右对照；"dialog" 窄屏全幅灯箱 */
    mode: "docked" | "dialog"
    open?: boolean
    onOpenChange?: (open: boolean) => void
    idPrefix?: string
    className?: string
    emptyMessage?: string
}>

export type MaterialFileListProps = Readonly<{
    files: readonly MaterialFileItem[]
    /** 点击「预览」触发；传入时展示「预览」按钮并固定排在「下载」之前。 */
    onPreview?: (file: MaterialFileItem) => void
    /** 当前选中或正在预览的文件 ID（用于高亮当前行）。 */
    activeFileId?: string
    /** 外部受控预览加载中的文件 ID。 */
    previewLoadingFileId?: string | null
    /** 外部受控下载中的文件 ID（未传时由列表内部自动维护）。 */
    downloadingFileId?: string | null
    /** 自定义可预览判定；默认支持 PDF 与 image/*（排除 SVG）。 */
    canPreview?: (file: MaterialFileItem) => boolean
    emptyMessage?: string
    idPrefix?: string
    downloadVariant?: "outline" | "ghost"
    downloadButtonClassName?: string
    className?: string
}>

type PreviewMediaKind = "pdf" | "image" | "unknown" | "unsupported"

const ZOOM_MIN = 0.5
const ZOOM_MAX = 2
const ZOOM_STEP = 0.25

const IMAGE_EXTENSION_RE = /\.(?:png|jpe?g|webp|gif|bmp)$/i
const PDF_EXTENSION_RE = /\.pdf$/i
const UNSUPPORTED_EXTENSION_RE =
    /\.(?:xlsx?|docx?|pptx?|csv|txt|zip|rar|7z|tar|gz|svg)$/i

function normalizeMime(mimeType?: string): string {
    return (mimeType ?? "").split(";")[0]?.trim().toLowerCase() ?? ""
}

function detectKindFromMetadata(
    mimeType?: string,
    fileName?: string,
): PreviewMediaKind {
    const mime = normalizeMime(mimeType)
    if (mime === "application/pdf") return "pdf"
    if (mime.startsWith("image/") && mime !== "image/svg+xml") return "image"
    if (mime && mime !== "application/octet-stream") return "unsupported"

    const name = fileName ?? ""
    if (PDF_EXTENSION_RE.test(name)) return "pdf"
    if (IMAGE_EXTENSION_RE.test(name)) return "image"
    if (UNSUPPORTED_EXTENSION_RE.test(name)) return "unsupported"
    return "unknown"
}

function detectKindFromBlob(
    blob: Blob,
    mimeType?: string,
    fileName?: string,
): Exclude<PreviewMediaKind, "unknown"> {
    const fromBlob = detectKindFromMetadata(blob.type)
    if (fromBlob === "pdf" || fromBlob === "image") return fromBlob
    const fromMeta = detectKindFromMetadata(mimeType, fileName)
    if (fromMeta === "pdf" || fromMeta === "image") return fromMeta
    return "unsupported"
}

export function isMaterialPreviewable(
    file: Pick<MaterialFileItem, "mimeType" | "name">,
): boolean {
    const kind = detectKindFromMetadata(file.mimeType, file.name)
    return kind === "pdf" || kind === "image"
}

export function formatMaterialFileSize(sizeBytes: number): string {
    if (sizeBytes < 1024 * 1024) {
        return `${Math.max(1, Math.ceil(sizeBytes / 1024))} KB`
    }
    return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`
}

function materialKindMeta(file: Pick<MaterialFileItem, "mimeType" | "name">) {
    const kind = detectKindFromMetadata(file.mimeType, file.name)
    if (kind === "pdf") return { label: "PDF", Icon: FileTextIcon }
    if (kind === "image") return { label: "图片", Icon: ImageIcon }
    return { label: "文件", Icon: FileIcon }
}

function nextRotation(current: PdfRotation): PdfRotation {
    if (current === 0) return 90
    if (current === 90) return 180
    if (current === 180) return 270
    return 0
}

/**
 * 统一附件列表组件：按钮统一采用 `LoadingButton`，顺序固定为「预览 → 下载」。
 */
export function MaterialFileList({
    files,
    onPreview,
    activeFileId,
    previewLoadingFileId,
    downloadingFileId,
    canPreview = isMaterialPreviewable,
    emptyMessage = "暂无附件。",
    idPrefix = "material-file",
    downloadVariant = "outline",
    downloadButtonClassName,
    className,
}: MaterialFileListProps) {
    const [internalDownloadingId, setInternalDownloadingId] = useState<
        string | null
    >(null)
    const [downloadError, setDownloadError] = useState<string | null>(null)

    const activeDownloadingId = downloadingFileId ?? internalDownloadingId

    const handleDownload = async (file: MaterialFileItem) => {
        if (activeDownloadingId) return
        setDownloadError(null)
        setInternalDownloadingId(file.id)
        try {
            await file.onDownload()
        } catch (cause: unknown) {
            setDownloadError(getErrorMessage(cause, "附件读取失败，请重试"))
        } finally {
            setInternalDownloadingId((current) =>
                current === file.id ? null : current,
            )
        }
    }

    if (files.length === 0) {
        return (
            <p
                className={cn(
                    "rounded-lg border border-dashed border-border px-4 py-3 text-xs text-muted-foreground",
                    className,
                )}
            >
                {emptyMessage}
            </p>
        )
    }

    return (
        <div
            data-slot="material-file-list"
            className={cn("space-y-2", className)}
        >
            <ul className="divide-y divide-border/70 rounded-lg border border-border/80 bg-card">
                {files.map((file) => {
                    const kind = materialKindMeta(file)
                    const segment = toAutomationIdSegment(file.id)
                    const isSelected = activeFileId === file.id
                    const isDownloading = activeDownloadingId === file.id
                    const isPreviewLoading = previewLoadingFileId === file.id
                    const previewable = Boolean(onPreview) && canPreview(file)
                    return (
                        <li
                            key={file.id}
                            className={cn(
                                "flex flex-wrap items-center gap-3 px-4 py-3 text-body-compact transition-colors",
                                isSelected && "bg-sidebar-accent/40",
                            )}
                        >
                            <span className="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                                <kind.Icon
                                    aria-hidden="true"
                                    className="size-4"
                                />
                            </span>
                            <span className="min-w-0 flex-1">
                                <span className="block break-all font-medium text-foreground">
                                    {file.name}
                                </span>
                                <span className="num block text-xs text-muted-foreground">
                                    {kind.label}
                                    {file.sizeBytes != null
                                        ? ` · ${formatMaterialFileSize(file.sizeBytes)}`
                                        : ""}
                                </span>
                            </span>
                            <div className="flex shrink-0 items-center gap-2">
                                {previewable ? (
                                    <LoadingButton
                                        id={`${idPrefix}-${segment}-preview`}
                                        type="button"
                                        variant={
                                            isSelected ? "secondary" : "ghost"
                                        }
                                        size="sm"
                                        loading={isPreviewLoading}
                                        onClick={() => onPreview?.(file)}
                                    >
                                        <EyeIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        预览
                                    </LoadingButton>
                                ) : null}
                                <LoadingButton
                                    id={`${idPrefix}-${segment}-download`}
                                    type="button"
                                    variant={downloadVariant}
                                    size="sm"
                                    loading={isDownloading}
                                    disabled={Boolean(activeDownloadingId)}
                                    className={cn(
                                        "shrink-0",
                                        downloadButtonClassName,
                                    )}
                                    onClick={() => void handleDownload(file)}
                                >
                                    {isDownloading ? "读取中…" : "下载"}
                                </LoadingButton>
                            </div>
                        </li>
                    )
                })}
            </ul>
            {downloadError ? (
                <p role="alert" className="text-body-compact text-destructive">
                    {downloadError}
                </p>
            ) : null}
        </div>
    )
}

function ImageBlobViewport({
    blob,
    fileName,
    zoom,
    fitWidth,
    rotation,
    idPrefix,
}: {
    blob: Blob
    fileName: string
    zoom: number
    fitWidth: boolean
    rotation: PdfRotation
    idPrefix: string
}) {
    const [imageUrl, setImageUrl] = useState<string | null>(null)
    const [naturalSize, setNaturalSize] = useState<{
        width: number
        height: number
    } | null>(null)
    const [viewportSize, setViewportSize] = useState<{
        width: number
        height: number
    }>({ width: 0, height: 0 })
    const [dragging, setDragging] = useState(false)

    const containerRef = useRef<HTMLDivElement>(null)
    const dragRef = useRef<{
        pointerId: number
        startX: number
        startY: number
        scrollLeft: number
        scrollTop: number
    } | null>(null)

    useEffect(() => {
        const nextUrl = URL.createObjectURL(blob)
        setImageUrl(nextUrl)
        setNaturalSize(null)
        return () => {
            URL.revokeObjectURL(nextUrl)
        }
    }, [blob])

    const hasImageUrl = Boolean(imageUrl)

    useEffect(() => {
        if (!hasImageUrl) {
            setViewportSize({ width: 0, height: 0 })
            return
        }
        const element = containerRef.current
        if (!element) return
        const updateViewportSize = () => {
            const nextWidth = Math.floor(element.clientWidth)
            const nextHeight = Math.floor(element.clientHeight)
            setViewportSize((current) =>
                current.width === nextWidth && current.height === nextHeight
                    ? current
                    : { width: nextWidth, height: nextHeight },
            )
        }
        updateViewportSize()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(updateViewportSize)
        observer.observe(element)
        return () => observer.disconnect()
    }, [hasImageUrl])

    const dimensions = useMemo(() => {
        if (
            !naturalSize ||
            viewportSize.width <= 0 ||
            viewportSize.height <= 0
        ) {
            return null
        }
        const isQuarter = rotation === 90 || rotation === 270
        const rotatedWidth = isQuarter ? naturalSize.height : naturalSize.width
        const rotatedHeight = isQuarter ? naturalSize.width : naturalSize.height
        const availWidth = Math.max(viewportSize.width - 32, 160)
        const availHeight = Math.max(viewportSize.height - 32, 160)

        const baseScale = fitWidth
            ? availWidth / rotatedWidth
            : Math.min(
                  availWidth / rotatedWidth,
                  availHeight / rotatedHeight,
                  1,
              )
        const effectiveScale = baseScale * zoom

        return {
            boxWidth: Math.max(1, Math.round(rotatedWidth * effectiveScale)),
            boxHeight: Math.max(1, Math.round(rotatedHeight * effectiveScale)),
            imgWidth: Math.max(
                1,
                Math.round(naturalSize.width * effectiveScale),
            ),
            imgHeight: Math.max(
                1,
                Math.round(naturalSize.height * effectiveScale),
            ),
        }
    }, [naturalSize, viewportSize, fitWidth, zoom, rotation])

    const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
        if (event.button !== 0) return
        const element = containerRef.current
        if (!element) return
        const canScroll =
            element.scrollWidth > element.clientWidth + 1 ||
            element.scrollHeight > element.clientHeight + 1
        if (!canScroll) return
        dragRef.current = {
            pointerId: event.pointerId,
            startX: event.clientX,
            startY: event.clientY,
            scrollLeft: element.scrollLeft,
            scrollTop: element.scrollTop,
        }
        element.setPointerCapture(event.pointerId)
        setDragging(true)
    }

    const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
        const drag = dragRef.current
        const element = containerRef.current
        if (!drag || !element || drag.pointerId !== event.pointerId) return
        element.scrollLeft = drag.scrollLeft - (event.clientX - drag.startX)
        element.scrollTop = drag.scrollTop - (event.clientY - drag.startY)
    }

    const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
        const drag = dragRef.current
        if (!drag || drag.pointerId !== event.pointerId) return
        dragRef.current = null
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
            event.currentTarget.releasePointerCapture(event.pointerId)
        }
        setDragging(false)
    }

    if (!imageUrl) {
        return (
            <div
                role="status"
                className="flex h-full min-h-48 items-center justify-center gap-2 text-xs text-muted-foreground"
            >
                <Spinner />
                正在准备图片预览…
            </div>
        )
    }

    return (
        <div
            ref={containerRef}
            id={`${idPrefix}-image-viewport`}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            className={cn(
                "flex h-full min-h-0 flex-1 overflow-auto bg-muted/25 p-4",
                dragging ? "cursor-grabbing select-none" : "cursor-grab",
            )}
        >
            <div className="m-auto flex shrink-0 items-center justify-center">
                {dimensions ? (
                    <div
                        className="relative flex shrink-0 items-center justify-center"
                        style={{
                            width: dimensions.boxWidth,
                            height: dimensions.boxHeight,
                        }}
                    >
                        {/* eslint-disable-next-line @next/next/no-img-element -- 认证拉取的本地 Blob URL 不能经 Next Image 优化。 */}
                        <img
                            src={imageUrl}
                            alt={fileName}
                            draggable={false}
                            style={{
                                width: dimensions.imgWidth,
                                height: dimensions.imgHeight,
                                transform: rotation
                                    ? `rotate(${rotation}deg)`
                                    : undefined,
                            }}
                            className="block max-w-none shrink-0 rounded-xs bg-card shadow-xs select-none"
                        />
                    </div>
                ) : (
                    // eslint-disable-next-line @next/next/no-img-element -- 认证拉取的本地 Blob URL 不能经 Next Image 优化。
                    <img
                        src={imageUrl}
                        alt={fileName}
                        draggable={false}
                        onLoad={(event) => {
                            setNaturalSize({
                                width: event.currentTarget.naturalWidth,
                                height: event.currentTarget.naturalHeight,
                            })
                        }}
                        className="block max-h-full max-w-full rounded-xs bg-card object-contain shadow-xs select-none"
                    />
                )}
            </div>
        </div>
    )
}

function MaterialFilePane({
    file,
    enabled,
    mode,
    idPrefix,
    fullscreen,
    onToggleFullscreen,
}: {
    file: MaterialFileItem
    enabled: boolean
    mode: "docked" | "dialog"
    idPrefix: string
    fullscreen?: boolean
    onToggleFullscreen?: () => void
}) {
    const [zoom, setZoom] = useState(1)
    const [fitWidth, setFitWidth] = useState(true)
    const [rotation, setRotation] = useState<PdfRotation>(0)
    const [pdfPage, setPdfPage] = useState(1)
    const [pdfPageCount, setPdfPageCount] = useState(0)
    const [downloading, setDownloading] = useState(false)
    const [downloadError, setDownloadError] = useState<string | null>(null)

    const hintKind = detectKindFromMetadata(file.mimeType, file.name)
    const previewQuery = useQuery({
        queryKey: [
            "unified-material-viewer",
            idPrefix,
            file.id,
            file.name,
            file.mimeType ?? "",
            file.sizeBytes ?? 0,
        ],
        queryFn: ({ signal }) => file.loadPreview(signal),
        enabled: enabled && hintKind !== "unsupported",
        staleTime: 0,
        gcTime: 0,
        retry: false,
    })

    const effectiveKind: Exclude<PreviewMediaKind, "unknown"> | "loading" =
        hintKind === "unsupported"
            ? "unsupported"
            : previewQuery.data
              ? detectKindFromBlob(previewQuery.data, file.mimeType, file.name)
              : "loading"

    const canManipulate = effectiveKind === "pdf" || effectiveKind === "image"

    const handleDownload = async () => {
        if (downloading) return
        setDownloadError(null)
        setDownloading(true)
        try {
            await file.onDownload()
        } catch (cause: unknown) {
            setDownloadError(getErrorMessage(cause, "附件下载失败，请重试"))
        } finally {
            setDownloading(false)
        }
    }

    const toolbar = (
        <div
            className={cn(
                "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/70 bg-card",
                mode === "dialog" ? "px-5 py-2.5" : "px-3 py-2",
            )}
        >
            <div className="flex min-w-0 items-center gap-2">
                {mode === "dialog" ? (
                    <DialogTitle className="min-w-0 max-w-80 truncate text-section-title font-semibold tracking-tight">
                        {file.name}
                    </DialogTitle>
                ) : (
                    <p
                        className="min-w-0 max-w-48 truncate text-body-compact font-medium text-foreground"
                        title={file.name}
                    >
                        {file.name}
                    </p>
                )}
                {file.sizeBytes != null ? (
                    <span className="num shrink-0 text-xs text-muted-foreground">
                        {formatMaterialFileSize(file.sizeBytes)}
                    </span>
                ) : null}
            </div>

            {canManipulate ? (
                <div className="flex flex-wrap items-center gap-1">
                    {effectiveKind === "pdf" && pdfPageCount > 0 ? (
                        <div className="mr-1.5 flex items-center gap-1 border-r border-border/70 pr-2">
                            <Button
                                id={`${idPrefix}-pdf-prev`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="上一页"
                                disabled={pdfPage <= 1}
                                onClick={() =>
                                    setPdfPage((current) =>
                                        Math.max(1, current - 1),
                                    )
                                }
                            >
                                <ChevronLeftIcon aria-hidden="true" />
                            </Button>
                            <span
                                className="num px-1 text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                {pdfPage} / {pdfPageCount}
                            </span>
                            <Button
                                id={`${idPrefix}-pdf-next`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="下一页"
                                disabled={pdfPage >= pdfPageCount}
                                onClick={() =>
                                    setPdfPage((current) =>
                                        Math.min(pdfPageCount, current + 1),
                                    )
                                }
                            >
                                <ChevronRightIcon aria-hidden="true" />
                            </Button>
                        </div>
                    ) : null}

                    <Button
                        id={`${idPrefix}-zoom-out`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="缩小"
                        disabled={zoom <= ZOOM_MIN}
                        onClick={() =>
                            setZoom((current) =>
                                Math.max(
                                    ZOOM_MIN,
                                    Math.round((current - ZOOM_STEP) * 100) /
                                        100,
                                ),
                            )
                        }
                    >
                        <ZoomOutIcon aria-hidden="true" />
                    </Button>
                    <Button
                        id={`${idPrefix}-zoom-reset`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-label="重置缩放比例"
                        className="num min-w-12 px-1.5 text-xs text-muted-foreground hover:text-foreground"
                        onClick={() => {
                            setZoom(1)
                            setFitWidth(true)
                        }}
                    >
                        {Math.round(zoom * 100)}%
                    </Button>
                    <Button
                        id={`${idPrefix}-zoom-in`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="放大"
                        disabled={zoom >= ZOOM_MAX}
                        onClick={() =>
                            setZoom((current) =>
                                Math.min(
                                    ZOOM_MAX,
                                    Math.round((current + ZOOM_STEP) * 100) /
                                        100,
                                ),
                            )
                        }
                    >
                        <ZoomInIcon aria-hidden="true" />
                    </Button>
                    <Button
                        id={`${idPrefix}-fit-width`}
                        type="button"
                        variant={fitWidth ? "secondary" : "ghost"}
                        size="xs"
                        aria-pressed={fitWidth}
                        onClick={() => setFitWidth((current) => !current)}
                    >
                        <ScanIcon data-icon="inline-start" aria-hidden="true" />
                        {fitWidth ? "适应宽度" : "适应屏幕"}
                    </Button>
                    <Button
                        id={`${idPrefix}-rotate`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="顺时针旋转 90 度"
                        onClick={() =>
                            setRotation((current) => nextRotation(current))
                        }
                    >
                        <RotateCwIcon aria-hidden="true" />
                    </Button>
                </div>
            ) : null}

            <div className="flex shrink-0 items-center gap-1.5">
                <LoadingButton
                    id={`${idPrefix}-download`}
                    type="button"
                    variant="outline"
                    size="sm"
                    loading={downloading}
                    onClick={() => void handleDownload()}
                >
                    <DownloadIcon data-icon="inline-start" aria-hidden="true" />
                    {downloading ? "下载中…" : "下载原文件"}
                </LoadingButton>
                {mode === "dialog" && onToggleFullscreen ? (
                    <Button
                        id={`${idPrefix}-fullscreen`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label={fullscreen ? "退出全屏" : "全屏预览"}
                        className="text-muted-foreground hover:text-foreground"
                        onClick={onToggleFullscreen}
                    >
                        {fullscreen ? (
                            <Minimize2Icon aria-hidden="true" />
                        ) : (
                            <Maximize2Icon aria-hidden="true" />
                        )}
                    </Button>
                ) : null}
                {mode === "dialog" ? (
                    <DialogClose
                        render={
                            <Button
                                id={`${idPrefix}-close`}
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                className="text-muted-foreground hover:text-foreground"
                            />
                        }
                    >
                        <XIcon aria-hidden="true" />
                        <span className="sr-only">关闭附件预览</span>
                    </DialogClose>
                ) : null}
            </div>
        </div>
    )

    const viewportContent = (
        <>
            {effectiveKind === "unsupported" ? (
                <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
                    <span className="flex size-12 items-center justify-center rounded-lg border border-border bg-card text-muted-foreground">
                        <FileIcon aria-hidden="true" className="size-5" />
                    </span>
                    <div className="space-y-1">
                        <p className="break-all text-body-compact font-medium text-foreground">
                            {file.name}
                        </p>
                        <p className="text-xs text-muted-foreground">
                            此文件格式暂不支持在线预览，请下载原文件查看。
                        </p>
                    </div>
                    <LoadingButton
                        id={`${idPrefix}-fallback-download`}
                        type="button"
                        variant="outline"
                        size="sm"
                        loading={downloading}
                        onClick={() => void handleDownload()}
                    >
                        <DownloadIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        下载原文件
                    </LoadingButton>
                </div>
            ) : previewQuery.isError ? (
                <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        {getErrorMessage(
                            previewQuery.error,
                            "附件预览读取失败，请重试",
                        )}
                    </p>
                    <div className="flex flex-wrap items-center justify-center gap-2">
                        <Button
                            id={`${idPrefix}-retry`}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void previewQuery.refetch()}
                        >
                            重试预览
                        </Button>
                        <LoadingButton
                            id={`${idPrefix}-error-download`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            loading={downloading}
                            onClick={() => void handleDownload()}
                        >
                            下载原文件
                        </LoadingButton>
                    </div>
                </div>
            ) : previewQuery.isPending || !previewQuery.data ? (
                <div
                    role="status"
                    className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground"
                >
                    <Spinner />
                    正在读取附件…
                </div>
            ) : effectiveKind === "pdf" ? (
                <PdfDocumentCanvas
                    blob={previewQuery.data}
                    scale={zoom}
                    fitWidth={fitWidth}
                    rotation={rotation}
                    page={pdfPage}
                    onPageChange={setPdfPage}
                    onPageCountChange={setPdfPageCount}
                    showPagination={false}
                    idPrefix={`${idPrefix}-pdf`}
                    className="gap-0"
                    canvasContainerClassName="rounded-none border-0"
                    containerAriaLabel={`${file.name} PDF 预览，可滚动查看`}
                    canvasAriaLabel={(pageNumber) =>
                        `${file.name} 第 ${pageNumber} 页`
                    }
                />
            ) : (
                <ImageBlobViewport
                    blob={previewQuery.data}
                    fileName={file.name}
                    zoom={zoom}
                    fitWidth={fitWidth}
                    rotation={rotation}
                    idPrefix={idPrefix}
                />
            )}
        </>
    )

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {toolbar}
            {downloadError ? (
                <p
                    role="alert"
                    className="shrink-0 border-b border-destructive-border bg-destructive-soft px-4 py-1.5 text-xs text-destructive-soft-foreground"
                >
                    {downloadError}
                </p>
            ) : null}
            {mode === "dialog" ? (
                <DialogBody className="relative flex flex-col overflow-hidden bg-muted/20 p-0">
                    {viewportContent}
                </DialogBody>
            ) : (
                <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-muted/20">
                    {viewportContent}
                </div>
            )}
        </div>
    )
}

function MultiFileSwitcher({
    files,
    activeFileId,
    onSelect,
    idPrefix,
}: {
    files: readonly MaterialFileItem[]
    activeFileId: string
    onSelect: (id: string) => void
    idPrefix: string
}) {
    if (files.length <= 1) return null
    const currentIndex = Math.max(
        0,
        files.findIndex((item) => item.id === activeFileId),
    )

    return (
        <div
            aria-label="切换附件"
            className="flex shrink-0 items-center gap-1.5 border-b border-border/70 bg-muted/20 px-3 py-1.5"
        >
            <Button
                id={`${idPrefix}-file-prev`}
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label="上一个附件"
                disabled={currentIndex <= 0}
                onClick={() => {
                    const prev = files[currentIndex - 1]
                    if (prev) onSelect(prev.id)
                }}
            >
                <ChevronLeftIcon aria-hidden="true" />
            </Button>
            <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
                {files.map((item) => {
                    const selected = item.id === activeFileId
                    const kind = materialKindMeta(item)
                    return (
                        <Button
                            key={item.id}
                            id={`${idPrefix}-file-${toAutomationIdSegment(item.id)}`}
                            type="button"
                            variant={selected ? "secondary" : "ghost"}
                            size="xs"
                            aria-pressed={selected}
                            title={item.name}
                            className={cn(
                                "max-w-44 shrink-0 justify-start",
                                selected &&
                                    "border border-primary/35 bg-sidebar-accent text-sidebar-accent-foreground",
                            )}
                            onClick={() => onSelect(item.id)}
                        >
                            <kind.Icon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            <span className="truncate">{item.name}</span>
                        </Button>
                    )
                })}
            </div>
            <span className="num shrink-0 text-tiny text-muted-foreground">
                {currentIndex + 1}/{files.length}
            </span>
            <Button
                id={`${idPrefix}-file-next`}
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label="下一个附件"
                disabled={currentIndex >= files.length - 1}
                onClick={() => {
                    const next = files[currentIndex + 1]
                    if (next) onSelect(next.id)
                }}
            >
                <ChevronRightIcon aria-hidden="true" />
            </Button>
        </div>
    )
}

/**
 * 统一附件与材料对照预览器：
 * - 严格通过业务域提供的认证 `loadPreview(signal)` 拉取 `Blob`，不向 `<img>`/`pdfjs` 传递裸受保护 URL；
 * - 支持 PDF（`PdfDocumentCanvas`）、图片（原生 `<img>` 缩放/适应宽度/90° 旋转/拖拽滚动）与其他格式降级下载卡；
 * - 支持 `mode="docked"`（工作台右栏左右对照）与 `mode="dialog"`（灯箱预览）。
 */
export function UnifiedMaterialViewer({
    files,
    activeFileId,
    onActiveFileChange,
    mode,
    open = false,
    onOpenChange,
    idPrefix = "unified-material-viewer",
    className,
    emptyMessage = "暂无可预览的附件。",
}: UnifiedMaterialViewerProps) {
    const [internalFileId, setInternalFileId] = useState<string | undefined>(
        () => files[0]?.id,
    )
    const [fullscreen, setFullscreen] = useState(false)

    const resolvedFile =
        files.find((item) => item.id === (activeFileId ?? internalFileId)) ??
        files[0] ??
        null

    const handleSelectFile = (nextId: string) => {
        if (activeFileId === undefined) {
            setInternalFileId(nextId)
        }
        onActiveFileChange?.(nextId)
    }

    if (mode === "docked") {
        if (!resolvedFile) {
            return (
                <div
                    data-slot="unified-material-viewer"
                    className={cn(
                        "flex min-h-48 flex-1 items-center justify-center rounded-md border border-dashed border-border bg-muted/15 p-6 text-xs text-muted-foreground",
                        className,
                    )}
                >
                    {emptyMessage}
                </div>
            )
        }

        return (
            <section
                data-slot="unified-material-viewer"
                aria-label="附件侧边对照预览"
                className={cn(
                    "flex h-full min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border/80 bg-card",
                    className,
                )}
            >
                <MultiFileSwitcher
                    files={files}
                    activeFileId={resolvedFile.id}
                    onSelect={handleSelectFile}
                    idPrefix={idPrefix}
                />
                <MaterialFilePane
                    key={resolvedFile.id}
                    file={resolvedFile}
                    enabled
                    mode="docked"
                    idPrefix={idPrefix}
                />
            </section>
        )
    }

    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) setFullscreen(false)
                onOpenChange?.(nextOpen)
            }}
        >
            <DialogContent
                size={fullscreen ? "full" : "2xl"}
                scroll="body"
                showCloseButton={false}
                className={cn(!fullscreen && "h-[min(92dvh,56rem)]", className)}
            >
                <DialogDescription className="sr-only">
                    附件在线预览灯箱，支持缩放、旋转、多附件切换与下载原文件，按
                    Esc 或关闭按钮退出。
                </DialogDescription>
                {resolvedFile ? (
                    <>
                        <MaterialFilePane
                            key={resolvedFile.id}
                            file={resolvedFile}
                            enabled={open}
                            mode="dialog"
                            idPrefix={idPrefix}
                            fullscreen={fullscreen}
                            onToggleFullscreen={() =>
                                setFullscreen((current) => !current)
                            }
                        />
                        {files.length > 1 ? (
                            <div className="shrink-0 border-t border-border/70">
                                <MultiFileSwitcher
                                    files={files}
                                    activeFileId={resolvedFile.id}
                                    onSelect={handleSelectFile}
                                    idPrefix={idPrefix}
                                />
                            </div>
                        ) : null}
                    </>
                ) : (
                    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-xs text-muted-foreground">
                        <DialogTitle className="sr-only">附件预览</DialogTitle>
                        <p>{emptyMessage}</p>
                        <DialogClose
                            render={
                                <Button
                                    id={`${idPrefix}-empty-close`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                />
                            }
                        >
                            关闭
                        </DialogClose>
                    </div>
                )}
            </DialogContent>
        </Dialog>
    )
}
