"use client"

import Image from "next/image"
import { useEffect, useMemo, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { DownloadIcon, ZoomInIcon } from "lucide-react"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Button } from "@/components/ui/button"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadBlob } from "@/lib/download-blob"
import { portalReviewFile } from "../api"
import { portalAdminKeys, usePortalReviewFile } from "../hooks"
import { portalReviewMaterialRefs } from "../lib/review-overview"

function ReviewImage({
    applicationId,
    id,
    label,
    onPreview,
}: {
    applicationId: string
    id: string
    label: string
    onPreview: (id: string) => void
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
                        onClick={() => onPreview(id)}
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

export function usePortalReviewMaterials(application?: PortalApplication) {
    const queryClient = useQueryClient()
    const applicationId = application?.id
    const unique = useMemo(
        () => portalReviewMaterialRefs(application),
        [application],
    )

    const materialItems = useMemo<readonly MaterialFileItem[]>(
        () =>
            applicationId
                ? unique.map((row) => {
                      const fetchBlob = () =>
                          queryClient.fetchQuery({
                              queryKey: [
                                  ...portalAdminKeys.all,
                                  "image",
                                  applicationId,
                                  row.id,
                              ],
                              queryFn: () =>
                                  portalReviewFile(applicationId, row.id),
                          })
                      return {
                          id: row.id,
                          name: row.label,
                          mimeType: row.mimeType,
                          loadPreview: () => fetchBlob(),
                          onDownload: async () => {
                              const blob = await fetchBlob()
                              downloadBlob(blob, row.label)
                          },
                      }
                  })
                : [],
        [applicationId, queryClient, unique],
    )

    return { unique, materialItems }
}

export function PortalReviewImages({
    application,
}: {
    application: PortalApplication
}) {
    const [activeFileId, setActiveFileId] = useState<string | null>(null)
    const { unique, materialItems } = usePortalReviewMaterials(application)

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
                        onPreview={setActiveFileId}
                    />
                ))}
            </div>
            <UnifiedMaterialViewer
                mode="dialog"
                files={materialItems}
                activeFileId={activeFileId ?? undefined}
                onActiveFileChange={setActiveFileId}
                open={activeFileId !== null}
                onOpenChange={(open) => {
                    if (!open) setActiveFileId(null)
                }}
                idPrefix="supplier-portal-review-image-dialog"
            />
        </DetailWorkspaceSection>
    )
}
