"use client"
import Image from "next/image"
import { useEffect, useState } from "react"
import type {
    NewProductInput,
    PortalApplication,
} from "@/features/supplier-portal/types"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePortalReviewFile } from "../hooks"
function ReviewImage({
    applicationId,
    id,
    label,
}: {
    applicationId: string
    id: string
    label: string
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
    return (
        <div className="space-y-2 rounded-lg border p-3">
            <p className="text-xs text-muted-foreground">{label}</p>
            {url ? (
                <>
                    <Image
                        src={url}
                        alt={label}
                        width={144}
                        height={144}
                        unoptimized
                        className="h-36 w-36 object-contain"
                    />
                    <a
                        id={`supplier-portal-review-image-download-${toAutomationIdSegment(id)}`}
                        href={url}
                        download={label}
                        className="text-sm text-primary"
                    >
                        下载原图
                    </a>
                </>
            ) : (
                <p className="text-xs text-muted-foreground">
                    {query.isError ? "图片暂不可读取" : "正在读取图片…"}
                </p>
            )}
            {query.isError && (
                <Button
                    id={`supplier-portal-review-image-retry-${toAutomationIdSegment(id)}`}
                    type="button"
                    variant="ghost"
                    size="sm"
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
        <section className="space-y-3">
            <h2 className="font-semibold">供应商原稿图片及资料</h2>
            <div className="flex flex-wrap gap-3">
                {unique.map((row) => (
                    <ReviewImage
                        key={row.id}
                        applicationId={application.id}
                        id={row.id}
                        label={row.label}
                    />
                ))}
            </div>
        </section>
    )
}
