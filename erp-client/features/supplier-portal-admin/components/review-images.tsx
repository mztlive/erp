"use client"
import Image from "next/image"
import { useEffect, useState } from "react"
import { DownloadIcon, ZoomInIcon } from "lucide-react"
import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import type {
    NewProductInput,
    PortalApplication,
} from "@/features/supplier-portal/types"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePortalReviewFile } from "../hooks"

function ReviewImage({
    applicationId,
    id,
    label,
    onPreview,
}: {
    applicationId: string
    id: string
    label: string
    onPreview: (preview: { id: string; url: string; label: string }) => void
}) {
    const query = usePortalReviewFile(applicationId, id)
    const [url, setUrl] = useState<string | null>(null)
    useEffect(() => {
        if (!query.data) return
        const next = URL.createObjectURL(query.data)
        setUrl(next)
        return () => {
            URL.revokeObjectURL(next)
            setUrl(null)
        }
    }, [query.data])
    const seg = toAutomationIdSegment(id)
    return (
        <div className="flex w-44 flex-col gap-2 border border-border/70 bg-card p-3">
            <p className="truncate text-xs text-muted-foreground" title={label}>
                {label}
            </p>
            {url ? (
                <>
                    <button
                        id={`supplier-portal-review-image-preview-${seg}`}
                        type="button"
                        aria-label={`放大查看 ${label}`}
                        onClick={() => onPreview({ id, url, label })}
                        className="group relative flex h-36 w-full items-center justify-center overflow-hidden bg-muted/25 transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        <Image
                            src={url}
                            alt={label}
                            width={144}
                            height={144}
                            unoptimized
                            className="h-36 w-36 object-contain transition-transform duration-150 group-hover:scale-105"
                        />
                        <span className="pointer-events-none absolute right-1.5 bottom-1.5 inline-flex items-center gap-1 rounded bg-background/85 px-1.5 py-0.5 text-xs text-foreground opacity-0 shadow-2xs transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
                            <ZoomInIcon className="size-3" aria-hidden="true" />
                            查看大图
                        </span>
                    </button>
                    <a
                        id={`supplier-portal-review-image-download-${seg}`}
                        href={url}
                        download={label}
                        className="inline-flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
                    >
                        <DownloadIcon className="size-3.5" aria-hidden="true" />
                        下载原图
                    </a>
                </>
            ) : (
                <div className="flex h-36 items-center justify-center bg-muted/20 px-2 text-center">
                    <p className="text-xs text-muted-foreground">
                        {query.isError ? "图片暂不可读取" : "正在读取图片…"}
                    </p>
                </div>
            )}
            {query.isError && (
                <Button
                    id={`supplier-portal-review-image-retry-${seg}`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.subtleButton}
                    onClick={() => void query.refetch()}
                >
                    重新读取图片
                </Button>
            )}
        </div>
    )
}

export function PortalReviewImages({
    application,
}: {
    application: PortalApplication
}) {
    const [preview, setPreview] = useState<{
        id: string
        url: string
        label: string
    } | null>(null)
    const product = (application.submitted_snapshot ??
        application.input) as unknown as NewProductInput
    const rows = [
        ...(product.image_asset_ids ?? []).map((id) => ({
            id,
            label: "商品图片",
        })),
        ...(product.file_asset_ids ?? []).map((id) => ({
            id,
            label: "商品资料图片",
        })),
        ...(product.skus ?? []).flatMap((row) =>
            row.image_asset_id
                ? [{ id: row.image_asset_id, label: `${row.name} · 规格图片` }]
                : [],
        ),
    ]
    const unique = rows.filter(
        (row, index) => rows.findIndex((item) => item.id === row.id) === index,
    )
    if (!unique.length) return null
    return (
        <DetailWorkspaceSection title="供应商提报图片及资料">
            <div className="flex flex-wrap gap-3">
                {unique.map((row) => (
                    <ReviewImage
                        key={row.id}
                        applicationId={application.id}
                        id={row.id}
                        label={row.label}
                        onPreview={setPreview}
                    />
                ))}
            </div>
            <Dialog
                open={preview !== null}
                onOpenChange={(open) => {
                    if (!open) setPreview(null)
                }}
            >
                <DialogContent
                    closeButtonId="supplier-portal-review-image-dialog-close"
                    className="sm:max-w-3xl"
                >
                    <DialogHeader>
                        <DialogTitle>
                            {preview?.label ?? "图片预览"}
                        </DialogTitle>
                        <DialogDescription>
                            查看供应商提交的原图，可点击下方按钮下载留存。
                        </DialogDescription>
                    </DialogHeader>
                    {preview ? (
                        <div className="flex max-h-[70vh] items-center justify-center overflow-auto bg-muted/20 p-4">
                            <Image
                                src={preview.url}
                                alt={preview.label}
                                width={960}
                                height={720}
                                unoptimized
                                className="max-h-[65vh] w-auto object-contain"
                            />
                        </div>
                    ) : null}
                    <DialogFooter>
                        {preview ? (
                            <a
                                id="supplier-portal-review-image-dialog-download"
                                href={preview.url}
                                download={preview.label}
                                className={buttonVariants({
                                    variant: "outline",
                                    size: "sm",
                                })}
                            >
                                <DownloadIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                下载原图
                            </a>
                        ) : null}
                        <Button
                            id="supplier-portal-review-image-dialog-dismiss"
                            type="button"
                            size="sm"
                            onClick={() => setPreview(null)}
                        >
                            关闭
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </DetailWorkspaceSection>
    )
}
