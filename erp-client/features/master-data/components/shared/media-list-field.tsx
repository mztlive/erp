"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ImageIcon, XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FileUpload } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import { fetchFileAssetPreviewBlob } from "@/features/file-assets/api"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    joinMediaList,
    parseMediaList,
} from "@/features/master-data/lib/resource-fields"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { getErrorMessage } from "@/lib/api/errors"

/** 根据展示文件名判定内嵌预览类型。 */
const mediaPreviewKind = (fileName: string): "image" | "pdf" | null => {
    if (/\.(?:jpe?g|png|webp)$/i.test(fileName)) return "image"
    if (/\.pdf$/i.test(fileName)) return "pdf"
    return null
}

export function MediaListField({
    idPrefix,
    label,
    hint,
    value,
    onChange,
    accept = "image/jpeg,image/png,image/webp",
    urlByFileName,
    assetIdByFileName,
    onFilesSelected,
    disabled = false,
}: {
    idPrefix?: string
    label: string
    hint?: string
    value: string
    onChange: (next: string) => void
    /** 允许上传的文件类型；默认图片。 */
    accept?: string
    /** fileName → 可访问 URL（已上传文件回显为链接）。 */
    urlByFileName?: Readonly<Record<string, string>>
    /** fileName → 文件资产 ID（敏感文件通过受控接口预览）。 */
    assetIdByFileName?: Readonly<Record<string, string>>
    /** 选择文件时透出原始文件（供保存前上传）。 */
    onFilesSelected?: (files: File[]) => void
    /** 禁止新增和移除文件；已登记文件仍可查看。 */
    disabled?: boolean
}) {
    const basePrefix =
        idPrefix ?? `master-data-media-${toAutomationIdSegment(label)}`
    const queryClient = useQueryClient()
    const items = parseMediaList(value)
    const [preview, setPreview] = React.useState<{
        name: string
        src: string
        kind: "image" | "pdf"
    } | null>(null)
    const [previewLoading, setPreviewLoading] = React.useState<string | null>(
        null,
    )
    const [localPreviewUrls, setLocalPreviewUrls] = React.useState<
        Readonly<Record<string, string>>
    >({})
    const localPreviewUrlsRef = React.useRef<Record<string, string>>({})

    React.useEffect(
        () => () => {
            for (const url of Object.values(localPreviewUrlsRef.current)) {
                URL.revokeObjectURL(url)
            }
            localPreviewUrlsRef.current = {}
        },
        [],
    )

    /** 打开本地图片、公开图片或经鉴权读取的敏感图片预览。 */
    const openPreview = async (name: string) => {
        const kind = mediaPreviewKind(name)
        if (!kind) return
        const directUrl =
            localPreviewUrls[name] ?? urlByFileName?.[name]?.trim()
        if (directUrl) {
            setPreview({ name, src: directUrl, kind })
            return
        }
        const assetId = assetIdByFileName?.[name]?.trim()
        if (!assetId) return
        setPreviewLoading(name)
        try {
            const blob = await queryClient.fetchQuery({
                queryKey: ["file-assets", "preview", assetId],
                queryFn: () => fetchFileAssetPreviewBlob(assetId),
                staleTime: 60_000,
            })
            const previous = localPreviewUrlsRef.current[name]
            if (previous) URL.revokeObjectURL(previous)
            const src = URL.createObjectURL(blob)
            localPreviewUrlsRef.current[name] = src
            setLocalPreviewUrls({ ...localPreviewUrlsRef.current })
            setPreview({ name, src, kind })
        } catch (error) {
            toast.add({
                title: "文件预览失败",
                description: getErrorMessage(error, "请稍后重试"),
                type: "error",
                timeout: 4000,
            })
        } finally {
            setPreviewLoading(null)
        }
    }

    /** 释放已经从列表移除的本地 Blob URL。 */
    const removeLocalPreview = (name: string) => {
        const url = localPreviewUrlsRef.current[name]
        if (!url) return
        URL.revokeObjectURL(url)
        delete localPreviewUrlsRef.current[name]
        setLocalPreviewUrls({ ...localPreviewUrlsRef.current })
        setPreview((current) => (current?.name === name ? null : current))
    }

    return (
        <div className="space-y-2">
            <div className="flex items-center justify-between gap-2">
                <Label className="text-sm font-medium">{label}</Label>
                <span className="text-xs text-muted-foreground">
                    {masterDataCopy.mediaCount(items.length)}
                    {hint ? ` · ${hint}` : null}
                </span>
            </div>
            {items.length > 0 ? (
                <ul className="space-y-1.5">
                    {items.map((name, index) => {
                        const url = urlByFileName?.[name]?.trim()
                        const previewUrl = localPreviewUrls[name] ?? url
                        const canPreview = Boolean(
                            mediaPreviewKind(name) &&
                            (previewUrl || assetIdByFileName?.[name]?.trim()),
                        )
                        return (
                            <li
                                key={`${name}-${index}`}
                                className="flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5"
                            >
                                <button
                                    id={`${basePrefix}-item-${toAutomationIdSegment(name)}-preview`}
                                    type="button"
                                    className="flex min-w-0 flex-1 items-center gap-2 text-left disabled:cursor-default"
                                    disabled={
                                        !canPreview || previewLoading === name
                                    }
                                    onClick={() => void openPreview(name)}
                                    aria-label={
                                        canPreview ? `预览 ${name}` : name
                                    }
                                >
                                    <span className="flex size-9 shrink-0 items-center justify-center overflow-hidden rounded bg-muted">
                                        {previewUrl &&
                                        mediaPreviewKind(name) === "image" ? (
                                            // eslint-disable-next-line @next/next/no-img-element -- 本地 Blob 与受控文件内容不能交给 Next Image 优化。
                                            <img
                                                src={previewUrl}
                                                alt=""
                                                className="size-full object-cover"
                                            />
                                        ) : (
                                            <ImageIcon
                                                className="size-4 text-muted-foreground"
                                                aria-hidden
                                            />
                                        )}
                                    </span>
                                    <span
                                        className={cn(
                                            "min-w-0 flex-1 truncate text-sm",
                                            canPreview &&
                                                "text-primary underline-offset-2 hover:underline",
                                        )}
                                    >
                                        {previewLoading === name
                                            ? "正在打开预览…"
                                            : name}
                                    </span>
                                </button>
                                <Button
                                    id={`${basePrefix}-item-${toAutomationIdSegment(name)}-remove`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                                    disabled={disabled}
                                    onClick={() => {
                                        removeLocalPreview(name)
                                        const next = items.filter(
                                            (_, i) => i !== index,
                                        )
                                        onChange(joinMediaList(next))
                                    }}
                                >
                                    <XIcon className="size-3.5" />
                                </Button>
                            </li>
                        )
                    })}
                </ul>
            ) : (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.mediaEmpty}（
                    {masterDataCopy.mediaAllowEmpty}）
                </p>
            )}
            <FileUpload
                idPrefix={`${basePrefix}-upload`}
                accept={accept}
                multiple
                disabled={disabled}
                label={`添加${label}`}
                description={masterDataCopy.mediaUploadHint}
                onFilesSelected={(files) => {
                    onFilesSelected?.(files)
                    for (const file of files) {
                        if (
                            !file.type.startsWith("image/") &&
                            file.type !== "application/pdf"
                        ) {
                            continue
                        }
                        const previous = localPreviewUrlsRef.current[file.name]
                        if (previous) URL.revokeObjectURL(previous)
                        localPreviewUrlsRef.current[file.name] =
                            URL.createObjectURL(file)
                    }
                    setLocalPreviewUrls({ ...localPreviewUrlsRef.current })
                    const names = files.map((f) => f.name)
                    onChange(joinMediaList([...items, ...names]))
                }}
                className="p-3"
            />
            <Dialog
                open={preview != null}
                onOpenChange={(open) => {
                    if (!open) setPreview(null)
                }}
            >
                <DialogContent
                    className="sm:max-w-4xl"
                    closeButtonId={`${basePrefix}-preview-close`}
                >
                    <DialogHeader>
                        <DialogTitle>{preview?.name ?? "图片预览"}</DialogTitle>
                        <DialogDescription>
                            仅在当前登录会话中查看。
                        </DialogDescription>
                    </DialogHeader>
                    {preview?.kind === "image" ? (
                        <div className="flex max-h-[72vh] min-h-64 items-center justify-center overflow-auto rounded-md border bg-muted/30 p-3">
                            {/* eslint-disable-next-line @next/next/no-img-element -- 预览来源可能是本地 Blob 或需鉴权读取的对象。 */}
                            <img
                                src={preview.src}
                                alt={preview.name}
                                className="max-h-[68vh] max-w-full object-contain"
                            />
                        </div>
                    ) : preview?.kind === "pdf" ? (
                        <iframe
                            src={preview.src}
                            title={preview.name}
                            sandbox=""
                            referrerPolicy="no-referrer"
                            className="h-[72vh] w-full rounded-md border bg-background"
                        />
                    ) : null}
                </DialogContent>
            </Dialog>
        </div>
    )
}
