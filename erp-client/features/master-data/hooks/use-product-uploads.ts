"use client"

import * as React from "react"

import { uploadFileAssetImage } from "@/features/file-assets/api"
import type { ProductFields } from "@/features/master-data/types"

/**
 * 商品编辑器的待上传媒体：
 * - 本会话选择但尚未上传的图片文件（SPU 轮播/详情图按 fileName、SKU 主图按行号记录）；
 * - 保存前把仍是本地 blob 预览的图片上传为文件资产，返回回填后的字段。
 */
export function useProductUploads() {
    const [uploadingMedia, setUploadingMedia] = React.useState(false)
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    const pendingSkuFilesRef = React.useRef<Map<number, File>>(new Map())

    const rememberPendingFiles = React.useCallback((files: File[]) => {
        for (const file of files) {
            pendingFilesRef.current.set(file.name, file)
        }
    }, [])
    const rememberSkuFile = React.useCallback((index: number, file?: File) => {
        if (file) pendingSkuFilesRef.current.set(index, file)
    }, [])

    /** 把仍是本地 blob 预览的图片上传为文件资产，返回回填后的字段。 */
    const resolvePendingUploads = React.useCallback(
        async (current: ProductFields): Promise<ProductFields> => {
            const uploadIfPending = async (
                fileName: string,
                previewUrl: string | undefined,
                knownAssetId: string | undefined,
            ): Promise<{ url: string; assetId?: string } | null> => {
                const url = previewUrl?.trim()
                if (!url) return null
                if (url.startsWith("blob:")) {
                    const file = pendingFilesRef.current.get(fileName)
                    if (!file) {
                        throw new Error(
                            `找不到待上传图片「${fileName}」的文件内容，请重新选择`,
                        )
                    }
                    const uploaded = await uploadFileAssetImage(file)
                    return { url: uploaded.url, assetId: uploaded.fileAssetId }
                }
                return {
                    url,
                    ...(knownAssetId?.trim() ? { assetId: knownAssetId } : {}),
                }
            }

            const carouselPreviewUrls: Record<string, string> = {}
            const carouselFileAssetIds: Record<string, string> = {}
            for (const fileName of current.carouselImages) {
                const resolved = await uploadIfPending(
                    fileName,
                    current.carouselPreviewUrls[fileName],
                    current.carouselFileAssetIds[fileName],
                )
                if (resolved) {
                    carouselPreviewUrls[fileName] = resolved.url
                    if (resolved.assetId)
                        carouselFileAssetIds[fileName] = resolved.assetId
                }
            }
            const detailPreviewUrls: Record<string, string> = {}
            const detailFileAssetIds: Record<string, string> = {}
            for (const fileName of current.detailImages) {
                const resolved = await uploadIfPending(
                    fileName,
                    current.detailPreviewUrls[fileName],
                    current.detailFileAssetIds[fileName],
                )
                if (resolved) {
                    detailPreviewUrls[fileName] = resolved.url
                    if (resolved.assetId)
                        detailFileAssetIds[fileName] = resolved.assetId
                }
            }
            const skus = [...current.skus]
            for (let index = 0; index < skus.length; index++) {
                const sku = skus[index]
                if (!sku.mainImage) continue
                const previewUrl = sku.mainImagePreviewUrl?.trim()
                if (!previewUrl) continue
                if (!previewUrl.startsWith("blob:")) continue
                const file = pendingSkuFilesRef.current.get(index)
                if (!file) {
                    throw new Error(
                        `找不到待上传主图「${sku.mainImage}」的文件内容，请重新选择`,
                    )
                }
                const uploaded = await uploadFileAssetImage(file)
                skus[index] = {
                    ...sku,
                    mainImagePreviewUrl: uploaded.url,
                    mainImageAssetId: uploaded.fileAssetId,
                }
            }
            return {
                ...current,
                carouselPreviewUrls,
                carouselFileAssetIds,
                detailPreviewUrls,
                detailFileAssetIds,
                skus,
            }
        },
        [],
    )

    return {
        uploadingMedia,
        setUploadingMedia,
        pendingFilesRef,
        pendingSkuFilesRef,
        rememberPendingFiles,
        rememberSkuFile,
        resolvePendingUploads,
    }
}
