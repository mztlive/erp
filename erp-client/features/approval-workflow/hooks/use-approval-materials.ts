"use client"

import { useMutation, useQuery } from "@tanstack/react-query"
import {
    downloadApprovalMaterial,
    fetchApprovalMaterials,
    fetchApprovalMaterialPreview,
} from "../api/materials"
import { approvalKeys } from "../queries"

export const useApprovalMaterials = (instanceId: string, enabled: boolean) =>
    useQuery({
        queryKey: [...approvalKeys.instance(instanceId), "materials"],
        queryFn: () => fetchApprovalMaterials(instanceId),
        enabled: enabled && Boolean(instanceId),
        staleTime: 0,
    })

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
