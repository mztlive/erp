"use client"

/* eslint-disable @next/next/no-img-element -- 本地待上传图片使用 blob URL，无法经 Next Image 优化。 */

import * as React from "react"
import { SmartphoneIcon } from "lucide-react"

import { DetailWorkspaceSection } from "@/components/business"
import { imagePreviewSource } from "@/components/ui/file-upload"
import { MediaListEditor } from "@/features/master-data/components/product/product-editor-media"
import type { SetProductFields } from "@/features/master-data/components/product/product-editor-sections"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { ProductFields } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

type ProductMediaSectionProps = {
    idPrefix?: string
    productName?: string
    canRevise: boolean
    disabled?: boolean
    fields: ProductFields
    setFields: SetProductFields
    rememberPendingFiles: (files: File[]) => void
}

function MobileDetailPreview({
    productName,
    coverSrc,
    detailImages,
    detailPreviewUrls,
    activeIndex,
}: {
    productName?: string
    coverSrc?: string
    detailImages: readonly string[]
    detailPreviewUrls: Readonly<Record<string, string>>
    activeIndex: number | null
}) {
    return (
        <aside
            aria-label="移动端长图实时预览"
            className="min-w-0 lg:sticky lg:top-28"
        >
            <div className="mb-2.5 flex items-center justify-between gap-2">
                <span className="inline-flex items-center gap-1.5 text-xs font-medium text-foreground/80">
                    <SmartphoneIcon
                        className="size-3.5 text-muted-foreground"
                        aria-hidden
                    />
                    移动端详情长图预览
                </span>
                <span className="num text-xs text-muted-foreground">
                    {detailImages.length} 张无缝拼接
                </span>
            </div>

            <div className="mx-auto w-full max-w-[360px] overflow-hidden rounded-2xl border border-border/90 bg-card shadow-xs">
                {/* 极简手机顶条 */}
                <div className="flex items-center justify-between border-b border-border/60 bg-surface-sunken/80 px-4 py-2 text-xs text-muted-foreground">
                    <span className="truncate font-medium text-foreground/80">
                        {productName || "商品详情"}
                    </span>
                    <span className="shrink-0">客户选品视图</span>
                </div>

                <div className="max-h-[60dvh] overflow-y-auto overscroll-contain bg-card">
                    {/* 顶部迷你首图上下文 */}
                    {coverSrc ? (
                        <div className="flex items-center gap-2.5 border-b border-border/50 bg-surface-sunken/30 px-3 py-2.5">
                            <img
                                src={coverSrc}
                                alt={productName || "商品首图"}
                                className="size-10 shrink-0 rounded-md border border-border/70 object-cover"
                            />
                            <div className="min-w-0 flex-1">
                                <p className="truncate text-xs font-medium text-foreground">
                                    {productName || "未命名商品"}
                                </p>
                                <p className="text-xs text-muted-foreground">
                                    以下为详情长图连续展示效果
                                </p>
                            </div>
                        </div>
                    ) : null}

                    {detailImages.length > 0 ? (
                        <div className="flex flex-col">
                            {detailImages.map((name, index) => {
                                const src =
                                    detailPreviewUrls[name] ??
                                    imagePreviewSource(name)
                                const highlighted = activeIndex === index
                                return (
                                    <div
                                        key={`${name}-${index}`}
                                        className={cn(
                                            "relative transition-all duration-150",
                                            highlighted &&
                                                "ring-2 ring-primary ring-inset",
                                        )}
                                    >
                                        {src ? (
                                            <img
                                                src={src}
                                                alt={`详情图 ${index + 1}：${name}`}
                                                className="block h-auto w-full"
                                            />
                                        ) : (
                                            <div className="flex h-28 items-center justify-center border-b border-border/40 bg-muted/25 px-4 text-center text-xs text-muted-foreground">
                                                {name}
                                            </div>
                                        )}
                                        {highlighted ? (
                                            <span className="num absolute left-2 top-2 rounded-sm bg-primary px-1.5 py-0.5 text-tiny font-medium text-primary-foreground shadow-xs">
                                                第 {index + 1} 张
                                            </span>
                                        ) : null}
                                    </div>
                                )
                            })}
                        </div>
                    ) : (
                        <div className="flex flex-col items-center justify-center gap-1.5 px-6 py-16 text-center">
                            <SmartphoneIcon
                                className="size-6 text-muted-foreground/45"
                                aria-hidden
                            />
                            <p className="text-xs font-medium text-muted-foreground">
                                暂无详情图
                            </p>
                            <p className="text-xs text-muted-foreground/80">
                                在左侧上传并拖拽排序后，此处将实时呈现客户看到的无缝长图效果。
                            </p>
                        </div>
                    )}
                </div>
            </div>
        </aside>
    )
}

function ProductMediaSection({
    idPrefix,
    productName,
    canRevise,
    disabled = false,
    fields,
    setFields,
    rememberPendingFiles,
}: ProductMediaSectionProps) {
    const prefix = idPrefix ?? "master-data-product-media"
    const [activeDetailIndex, setActiveDetailIndex] = React.useState<
        number | null
    >(null)

    const coverName = fields.carouselImages[0]
    const coverSrc = coverName
        ? (fields.carouselPreviewUrls[coverName] ??
          imagePreviewSource(coverName))
        : fields.skus.find((sku) => sku.mainImagePreviewUrl)
              ?.mainImagePreviewUrl

    return (
        <fieldset id="product-section-media" className="min-w-0">
            <legend className="sr-only">
                {masterDataCopy.fieldMediaSection}
            </legend>

            <DetailWorkspaceSection width="full">
                <MediaListEditor
                    readOnly={!canRevise}
                    disabled={disabled}
                    existingFileNames={[
                        ...fields.carouselImages,
                        ...fields.detailImages,
                    ]}
                    idPrefix={`${prefix}-carousel`}
                    label={masterDataCopy.fCarouselImages}
                    hint="建议 3–5 张，可直接拖拽排序；首张作为商品主图"
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
            </DetailWorkspaceSection>

            <DetailWorkspaceSection width="full">
                <div className="grid min-w-0 items-start gap-8 lg:grid-cols-[minmax(0,1fr)_320px] xl:grid-cols-[minmax(0,1fr)_360px]">
                    <div className="min-w-0">
                        <MediaListEditor
                            readOnly={!canRevise}
                            disabled={disabled}
                            existingFileNames={[
                                ...fields.carouselImages,
                                ...fields.detailImages,
                            ]}
                            idPrefix={`${prefix}-detail`}
                            label={masterDataCopy.fDetailImages}
                            hint="支持批量上传与拖拽排序，悬停卡片可同步高亮右侧手机长图预览"
                            value={fields.detailImages}
                            previewUrls={fields.detailPreviewUrls}
                            onFilesSelected={rememberPendingFiles}
                            mode="detail"
                            onActiveImageChange={setActiveDetailIndex}
                            onChange={(next) =>
                                setFields((prev) => {
                                    const retained = new Set(next)
                                    return {
                                        ...prev,
                                        detailImages: next,
                                        detailPreviewUrls: Object.fromEntries(
                                            Object.entries(
                                                prev.detailPreviewUrls,
                                            ).filter(([name]) =>
                                                retained.has(name),
                                            ),
                                        ),
                                        detailFileAssetIds: Object.fromEntries(
                                            Object.entries(
                                                prev.detailFileAssetIds,
                                            ).filter(([name]) =>
                                                retained.has(name),
                                            ),
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
                    </div>
                    <MobileDetailPreview
                        productName={productName}
                        coverSrc={coverSrc}
                        detailImages={fields.detailImages}
                        detailPreviewUrls={fields.detailPreviewUrls}
                        activeIndex={activeDetailIndex}
                    />
                </div>
            </DetailWorkspaceSection>
        </fieldset>
    )
}

export { ProductMediaSection }
