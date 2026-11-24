"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { useMemo } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import {
    downloadApprovalMaterial,
    fetchApprovalMaterials,
    fetchApprovalMaterialPreview,
} from "../api/materials"
import { approvalKeys } from "../queries"

import type { MaterialReference } from "../api/materials"
export type { MaterialReference } from "../api/materials"

/** 材料独立授权；必须匹配任务绑定的单据和提交版本。 */
export const useApprovalMaterials = (
    instanceId: string,
    enabled: boolean,
    reference?: MaterialReference,
) => {
    const { data: profile } = useAccountProfileQuery()
    const query = useQuery({
        queryKey: [
            ...approvalKeys.instance(instanceId),
            "materials",
            reference ?? null,
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
        ],
        queryFn: () => fetchApprovalMaterials(instanceId, reference),
        enabled: enabled && Boolean(instanceId) && Boolean(profile),
        retry: false,
        staleTime: 0,
    })
    const raw = query.isError ? undefined : query.data
    const { names } = useProductUnitNames(
        [],
        Boolean(raw?.purchase_lines?.length),
    )
    const data = useMemo(
        () =>
            raw?.purchase_lines
                ? {
                      ...raw,
                      purchase_lines: raw.purchase_lines.map((line) => ({
                          ...line,
                          unitName: names.get(line.unit?.trim() ?? ""),
                          source: line.source && {
                              ...line.source,
                              unitName: names.get(line.source.unit.trim()),
                          },
                      })),
                  }
                : raw,
        [raw, names],
    )
    if (query.isError) return { ...query, data: undefined }
    if (query.isSuccess) return { ...query, data: data ?? query.data }
    return query
}

export const useDownloadApprovalMaterial = (
    instanceId: string,
    sourceRevisionId?: string,
) =>
    useMutation({
        mutationFn: (file: { file_asset_id: string; file_name: string }) =>
            downloadApprovalMaterial(
                instanceId,
                file.file_asset_id,
                file.file_name,
                sourceRevisionId,
            ),
    })

export const useApprovalMaterialPreview = (
    instanceId: string,
    assetId: string,
    sourceRevisionId?: string,
) =>
    useQuery({
        queryKey: [
            ...approvalKeys.instance(instanceId),
            "material-preview",
            assetId,
            sourceRevisionId ?? null,
        ],
        queryFn: () =>
            fetchApprovalMaterialPreview(instanceId, assetId, sourceRevisionId),
        staleTime: 0,
        gcTime: 0,
    })
