"use client"
import { useRef, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { FileUpload } from "@/components/ui/file-upload"
import { Button } from "@/components/ui/button"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { portalFile, portalUpload } from "../api"
import { commandKey } from "../lib/presentation"
import { usePortalApplication } from "../hooks/queries"
import type { PortalUpload } from "../types"
import { PortalError } from "./surface"
import { PortalCommandConflict } from "./command-conflict"
import { PortalImage } from "./portal-image"

type UploadIntent = {
    rows: { file: File; version: number; key: string; result?: PortalUpload }[]
    originalIds: string[]
}
export function PortalAttachments({
    applicationId,
    expectedVersion,
    onVersionChange,
    onBusyChange,
    assetIds,
    onChange,
    prefix = "supplier-portal-images",
    disabled = false,
    imageOnly = true,
}: {
    applicationId: string
    expectedVersion: number
    onVersionChange: (version: number) => void
    onBusyChange: (busy: boolean) => void
    assetIds: string[]
    onChange: (ids: string[]) => void
    prefix?: string
    disabled?: boolean
    imageOnly?: boolean
}) {
    const [names, setNames] = useState<Record<string, string>>({})
    const [error, setError] = useState<unknown>(null)
    const [conflictVersion, setConflictVersion] = useState<number | null>(null)
    const application = usePortalApplication(applicationId)
    const intent = useRef<UploadIntent | null>(null)
    const upload = useMutation({
        mutationFn: async (files?: File[]) => {
            if (!intent.current) {
                if (!files?.length)
                    throw new Error(imageOnly ? "请选择图片" : "请选择商品资料")
                if (files.some((file) => file.size > 5 * 1024 * 1024))
                    throw new Error("单个文件不能超过5 MB")
                if (
                    files.some(
                        (file) =>
                            !(
                                imageOnly
                                    ? ["image/jpeg", "image/png", "image/webp"]
                                    : [
                                          "image/jpeg",
                                          "image/png",
                                          "image/webp",
                                          "application/pdf",
                                      ]
                            ).includes(file.type),
                    )
                )
                    throw new Error(
                        imageOnly
                            ? "请选择JPG、PNG或WebP图片"
                            : "请选择JPG、PNG、WebP图片或PDF资料",
                    )
                intent.current = {
                    originalIds: assetIds,
                    rows: files.map((file) => ({
                        file,
                        version: expectedVersion,
                        key: commandKey("image"),
                    })),
                }
            }
            onBusyChange(true)
            let version =
                intent.current.rows.find((row) => !row.result)?.version ??
                expectedVersion
            for (const row of intent.current.rows) {
                if (row.result) continue
                row.version = version
                row.result = await portalUpload(
                    applicationId,
                    row.file,
                    row.version,
                    row.key,
                )
                version = row.result.request_version
                onVersionChange(version)
                setNames((value) => ({
                    ...value,
                    [row.result!.id]: row.file.name,
                }))
                onChange([
                    ...new Set([
                        ...intent.current.originalIds,
                        ...intent.current.rows.flatMap((item) =>
                            item.result ? [item.result.id] : [],
                        ),
                    ]),
                ])
            }
        },
        onSuccess: () => {
            intent.current = null
            onBusyChange(false)
            setError(null)
            setConflictVersion(null)
        },
        onError: (cause) => {
            setError(cause)
            setConflictVersion(null)
            if (commandFailureDisposition(cause) === "rejected") {
                intent.current = null
                onBusyChange(false)
            }
        },
        retry: false,
    })
    const download = useMutation({
        mutationFn: async (id: string) => {
            const blob = await portalFile({ request_id: applicationId }, id)
            const url = URL.createObjectURL(blob)
            const link = document.createElement("a")
            link.href = url
            link.download =
                names[id] ??
                (blob.type === "application/pdf"
                    ? "商品资料.pdf"
                    : imageOnly
                      ? "商品图片"
                      : "商品资料")
            link.click()
            setTimeout(() => URL.revokeObjectURL(url), 1000)
        },
        retry: false,
    })
    return (
        <div className="space-y-3">
            <PortalError error={error ?? download.error} />
            <PortalCommandConflict
                error={error}
                id={`${prefix}-reload-conflict`}
                disabled={upload.isPending}
                currentSummary="当前申请仍可维护。已上传文件继续保留，请核对后重传未完成的原文件。"
                onRecheck={async () => {
                    const result = await application.refetch()
                    if (!result.data || result.isError)
                        throw (
                            result.error ??
                            new Error("申请资料暂不可用，请重新读取")
                        )
                    if (
                        !["draft", "returned", "withdrawn"].includes(
                            result.data.status,
                        )
                    )
                        throw new Error("申请已进入采购确认，请先核对当前状态")
                    setConflictVersion(result.data.version)
                }}
                onConfirmed={() => {
                    if (conflictVersion == null || !intent.current) return
                    for (const row of intent.current.rows)
                        if (!row.result) {
                            row.version = conflictVersion
                            row.key = commandKey("asset")
                        }
                    onVersionChange(conflictVersion)
                    setError(null)
                    setConflictVersion(null)
                }}
            />
            {intent.current && (
                <div className="space-y-2">
                    <p className="text-sm text-muted-foreground">
                        上传结果尚未全部确认。请重试原文件，确认完成后再保存或继续上传。
                    </p>
                    <Button
                        id={`${prefix}-retry`}
                        type="button"
                        variant="outline"
                        disabled={upload.isPending}
                        onClick={() => upload.mutate(undefined)}
                    >
                        重试原文件上传
                    </Button>
                </div>
            )}
            <FileUpload
                idPrefix={`${prefix}-upload`}
                accept={
                    imageOnly
                        ? "image/jpeg,image/png,image/webp"
                        : "image/jpeg,image/png,image/webp,application/pdf,.pdf"
                }
                multiple
                disabled={disabled || upload.isPending || !!intent.current}
                label={
                    upload.isPending
                        ? "正在上传…"
                        : imageOnly
                          ? "添加商品图片"
                          : "添加商品资料图片"
                }
                description="支持JPG、PNG、WebP，每个文件最多5 MB。资料区支持PDF；上传后需保存本次引用。"
                onFilesSelected={(files) => upload.mutate(files)}
            />
            {assetIds.map((id, index) => (
                <div
                    key={id}
                    className="flex flex-wrap items-center gap-2 text-sm"
                >
                    {imageOnly && (
                        <PortalImage
                            assetId={id}
                            source={{ request_id: applicationId }}
                            alt={names[id] ?? `商品图片 ${index + 1}`}
                        />
                    )}
                    <span>
                        {names[id] ??
                            `${imageOnly ? "图片" : "资料"} ${index + 1}`}
                    </span>
                    <Button
                        id={`${prefix}-download-${toAutomationIdSegment(id)}`}
                        size="sm"
                        variant="ghost"
                        onClick={() => download.mutate(id)}
                        disabled={download.isPending}
                    >
                        下载查看
                    </Button>
                    <Button
                        id={`${prefix}-remove-${toAutomationIdSegment(id)}`}
                        size="sm"
                        variant="ghost"
                        disabled={
                            disabled || upload.isPending || !!intent.current
                        }
                        onClick={() =>
                            onChange(assetIds.filter((asset) => asset !== id))
                        }
                    >
                        移除引用
                    </Button>
                </div>
            ))}
        </div>
    )
}
