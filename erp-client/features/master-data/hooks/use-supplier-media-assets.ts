"use client"

import * as React from "react"

import { pendingFileReference } from "@/features/master-data/api/pending-assets"
import type { SupplierEditorFormValues } from "@/features/master-data/lib/supplier-editor-model"
import type {
    MasterDataCenterView,
    PendingAssetUpload,
} from "@/features/master-data/types"

const MEDIA_FIELDS = [
    "qualification",
    "contractFile",
    "authorizationFile",
    "foodLicense",
    "legalPersonIdCard",
] as const

const QUALIFICATION_TYPE_BY_FIELD: Record<
    (typeof MEDIA_FIELDS)[number],
    string
> = {
    qualification: "certificate",
    contractFile: "contract",
    authorizationFile: "authorization",
    foodLicense: "food_license",
    legalPersonIdCard: "legal_person_id",
}

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

    /** 形成临时资产引用；文件与供应商根命令由一次 multipart 请求提交。 */
    const preparePendingMedia = React.useCallback(
        (
            values: SupplierEditorFormValues,
        ): {
            assetMaps: Record<string, Record<string, string>>
            pendingAssetUploads: readonly PendingAssetUpload[]
        } => {
            const out: Record<string, Record<string, string>> = {}
            const pendingAssetUploads: PendingAssetUpload[] = []
            for (const key of MEDIA_FIELDS) {
                const names = (values[key] ?? "")
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean)
                const existing = mediaAssetMaps[key] ?? {}
                const map: Record<string, string> = {}
                for (const [index, name] of names.entries()) {
                    const known = existing[name]
                    if (known?.assetId) {
                        map[name] = known.assetId
                        continue
                    }
                    const file = pendingFilesRef.current.get(name)
                    if (!file) continue
                    const reference = pendingFileReference(
                        "supplier",
                        QUALIFICATION_TYPE_BY_FIELD[key],
                        index,
                    )
                    map[name] = reference
                    pendingAssetUploads.push({ reference, file })
                }
                out[key] = map
            }
            return { assetMaps: out, pendingAssetUploads }
        },
        [mediaAssetMaps],
    )

    return {
        rememberMediaFiles,
        mediaUrlsFor,
        mediaAssetIdsFor,
        preparePendingMedia,
    }
}
