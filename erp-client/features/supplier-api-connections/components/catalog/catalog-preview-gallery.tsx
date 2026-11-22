"use client"

import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CatalogImage, CatalogItem } from "../../api/catalog"
import { hasVerifiedCatalogImage } from "../../lib/catalog-media"
import { CatalogSourceImage } from "./catalog-source-image"

export type PreviewImage = {
    key: string
    image: CatalogImage
    label: string
    detail: boolean
}

export function previewImages(item: CatalogItem): PreviewImage[] {
    return [
        { images: item.images, detail: false, title: "商品图片" },
        { images: item.detail_images, detail: true, title: "商品详情图" },
    ].flatMap(({ images, detail, title }) => {
        const seen = new Set<string>()
        return (images ?? []).flatMap((image) => {
            const identity =
                image.file_asset_id || image.content_fingerprint || image.url
            if (seen.has(identity)) return []
            seen.add(identity)
            return [
                {
                    key: `${detail ? "detail" : "main"}-${identity}`,
                    image,
                    detail,
                    label: `${title} ${seen.size}`,
                },
            ]
        })
    })
}

export function CatalogPreviewThumbnails({
    images,
    onOpen,
}: {
    images: PreviewImage[]
    onOpen: (key: string) => void
}) {
    return (
        <div className="flex flex-wrap gap-2">
            {images.map((entry) => (
                <Button
                    key={entry.key}
                    id={`supplier-catalog-preview-image-${toAutomationIdSegment(entry.key)}`}
                    variant="outline"
                    className="size-16 overflow-hidden p-0"
                    aria-label={`放大查看${entry.label}`}
                    disabled={!hasVerifiedCatalogImage(entry.image)}
                    onClick={() => onOpen(entry.key)}
                >
                    <CatalogSourceImage
                        image={entry.image}
                        alt={entry.label}
                        width={64}
                        height={64}
                        compact
                        className="size-full object-contain"
                    />
                </Button>
            ))}
        </div>
    )
}

export function CatalogPreviewGallery({
    images,
    activeKey,
    onChange,
    name,
}: {
    images: PreviewImage[]
    activeKey: string | null
    onChange: (key: string | null) => void
    name: string
}) {
    const available = images.filter((entry) =>
        hasVerifiedCatalogImage(entry.image),
    )
    const index = available.findIndex((entry) => entry.key === activeKey)
    const active = available[index]
    return (
        <Dialog
            open={Boolean(active)}
            onOpenChange={(open) => {
                if (!open) onChange(null)
            }}
        >
            <DialogContent
                closeButtonId="supplier-catalog-preview-gallery-close"
                className="flex max-h-[90dvh] flex-col overflow-hidden sm:max-w-3xl"
                onKeyDown={(event) => {
                    if (event.key === "ArrowLeft" && index > 0) {
                        event.preventDefault()
                        onChange(available[index - 1].key)
                    } else if (
                        event.key === "ArrowRight" &&
                        index < available.length - 1
                    ) {
                        event.preventDefault()
                        onChange(available[index + 1].key)
                    }
                }}
            >
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle>{name}</DialogTitle>
                    <DialogDescription>
                        {active?.label} · {index + 1} / {available.length}
                    </DialogDescription>
                </DialogHeader>
                <div key={activeKey} className="min-h-0 flex-1 overflow-y-auto">
                    {active ? (
                        <CatalogSourceImage
                            image={active.image}
                            alt={`${name} · ${active.label}`}
                            width={1200}
                            height={1200}
                            className="h-auto w-full object-contain"
                        />
                    ) : null}
                </div>
                <DialogFooter className="shrink-0">
                    <Button
                        id="supplier-catalog-preview-gallery-previous"
                        variant="outline"
                        disabled={index <= 0}
                        onClick={() => onChange(available[index - 1].key)}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                        上一张
                    </Button>
                    <Button
                        id="supplier-catalog-preview-gallery-next"
                        variant="outline"
                        disabled={index < 0 || index >= available.length - 1}
                        onClick={() => onChange(available[index + 1].key)}
                    >
                        下一张
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
