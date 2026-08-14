"use client"

import * as React from "react"

import { uploadFileAssetImage } from "@/features/file-assets/api"
import type { SupplierEditorFormValues } from "@/features/master-data/lib/supplier-editor-model"
import type { MasterDataCenterView } from "@/features/master-data/types"

const MEDIA_FIELDS = [
    "qualification",
    "contractFile",
    "authorizationFile",
    "foodLicense",
    "legalPersonIdCard",
] as const

/** 已登记资质附件：字段 key → fileName → { assetId, url }（回显链接 + 再次保存不重复上传）。 */
export function useSupplierMediaAssets(
    data: MasterDataCenterView | null | undefined,
) {
    const mediaAssetMaps = React.useMemo(() => {
        const maps: Record<
            string,
            Record<string, { assetId: string; url: string }>
        > = {}
        for (const [key, entries] of Object.entries(data?.mediaAssets ?? {})) {
            const map: Record<string, { assetId: string; url: string }> = {}
            for (const entry of entries) {
                map[entry.fileName] = { assetId: entry.assetId, url: entry.url }
            }
            maps[key] = map
        }
        return maps
    }, [data])
    /** 本会话选择但尚未上传的资质文件；保存时按文件名上传并回填 asset id。 */
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    /** 保存失败后保留本会话已上传资产，重试时不重复上传。 */
    const uploadedAssetMapsRef = React.useRef<
        Record<string, Record<string, { assetId: string; url: string }>>
    >({})

    const rememberMediaFiles = React.useCallback((files: File[]) => {
        for (const file of files) {
            pendingFilesRef.current.set(file.name, file)
        }
    }, [])

    const mediaUrlsFor = React.useCallback(
        (fieldKey: string): Readonly<Record<string, string>> => {
            const entries = mediaAssetMaps[fieldKey] ?? {}
            return Object.fromEntries(
                Object.entries(entries).map(([name, info]) => [name, info.url]),
            )
        },
        [mediaAssetMaps],
    )

    const mediaAssetIdsFor = React.useCallback(
        (fieldKey: string): Readonly<Record<string, string>> => {
            const entries = mediaAssetMaps[fieldKey] ?? {}
            return Object.fromEntries(
                Object.entries(entries).map(([name, info]) => [
                    name,
                    info.assetId,
                ]),
            )
        },
        [mediaAssetMaps],
    )

    /** 上传仍为本地待传的资质文件，返回 fileName → asset id 映射（按字段）。 */
    const resolvePendingMedia = React.useCallback(
        async (
            values: SupplierEditorFormValues,
        ): Promise<Record<string, Record<string, string>>> => {
            const out: Record<string, Record<string, string>> = {}
            for (const key of MEDIA_FIELDS) {
                const names = (values[key] ?? "")
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean)
                const existing = mediaAssetMaps[key] ?? {}
                const uploadedInSession = uploadedAssetMapsRef.current[key] ?? {}
                const map: Record<string, string> = {}
                for (const name of names) {
                    const known = existing[name] ?? uploadedInSession[name]
                    if (known?.assetId) {
                        map[name] = known.assetId
                        continue
                    }
                    const file = pendingFilesRef.current.get(name)
                    if (!file) continue
                    const sensitivityClass =
                        key === "legalPersonIdCard"
                            ? "highly_sensitive"
                            : "sensitive"
                    const uploaded = await uploadFileAssetImage(
                        file,
                        "attachment",
                        sensitivityClass,
                    )
                    map[name] = uploaded.fileAssetId
                    uploadedAssetMapsRef.current[key] = {
                        ...uploadedAssetMapsRef.current[key],
                        [name]: {
                            assetId: uploaded.fileAssetId,
                            url: uploaded.url,
                        },
                    }
                }
                out[key] = map
            }
            return out
        },
        [mediaAssetMaps],
    )

    return {
        rememberMediaFiles,
        mediaUrlsFor,
        mediaAssetIdsFor,
        resolvePendingMedia,
    }
}
