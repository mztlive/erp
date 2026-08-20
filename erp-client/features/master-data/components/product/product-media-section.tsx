"use client"

import { masterDataCopy } from "@/features/master-data/lib/copy"
import { MediaListEditor } from "@/features/master-data/components/product/product-editor-media"
import type { SetProductFields } from "@/features/master-data/components/product/product-editor-sections"
import type { ProductFields } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

type ProductMediaSectionProps = {
    canRevise: boolean
    fields: ProductFields
    setFields: SetProductFields
    rememberPendingFiles: (files: File[]) => void
}

function ProductMediaSection({
    canRevise,
    fields,
    setFields,
    rememberPendingFiles,
}: ProductMediaSectionProps) {
    return (
        <fieldset
            id="product-section-media"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] space-y-5 border-b border-grid p-5 last:border-b-0",
            )}
            disabled={!canRevise}
        >
            <legend className="sr-only">
                {masterDataCopy.fieldMediaSection}
            </legend>
            <div className="text-base font-semibold">
                {masterDataCopy.fieldMediaSection}
            </div>
            <p className="text-xs text-muted-foreground">
                {masterDataCopy.productSpuMediaHint}
            </p>
            <section className="space-y-3">
                <MediaListEditor
                    label={masterDataCopy.fCarouselImages}
                    hint="建议上传 3–5 张，支持排序；首张作为商品首图"
                    value={fields.carouselImages}
                    previewUrls={fields.carouselPreviewUrls}
                    onFilesSelected={rememberPendingFiles}
                    onChange={(next) =>
                        setFields((prev) => {
                            const retained = new Set(next)
                            return {
                                ...prev,
                                carouselImages: next,
                                carouselPreviewUrls: Object.fromEntries(
                                    Object.entries(
                                        prev.carouselPreviewUrls,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                                carouselFileAssetIds: Object.fromEntries(
                                    Object.entries(
                                        prev.carouselFileAssetIds,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                            }
                        })
                    }
                    onPreviewUrlsChange={(next) =>
                        setFields((prev) => ({
                            ...prev,
                            carouselPreviewUrls: next,
                        }))
                    }
                />
            </section>
            <div className="border-t border-border" />
            <section className="space-y-3">
                <MediaListEditor
                    label={masterDataCopy.fDetailImages}
                    hint="支持批量上传与顺序调整，保存后详情图随商品版本一起保留"
                    value={fields.detailImages}
                    previewUrls={fields.detailPreviewUrls}
                    onFilesSelected={rememberPendingFiles}
                    mode="detail"
                    onChange={(next) =>
                        setFields((prev) => {
                            const retained = new Set(next)
                            return {
                                ...prev,
                                detailImages: next,
                                detailPreviewUrls: Object.fromEntries(
                                    Object.entries(
                                        prev.detailPreviewUrls,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                                detailFileAssetIds: Object.fromEntries(
                                    Object.entries(
                                        prev.detailFileAssetIds,
                                    ).filter(([name]) => retained.has(name)),
                                ),
                            }
                        })
                    }
                    onPreviewUrlsChange={(next) =>
                        setFields((prev) => ({
                            ...prev,
                            detailPreviewUrls: next,
                        }))
                    }
                />
            </section>
        </fieldset>
    )
}

export { ProductMediaSection }
