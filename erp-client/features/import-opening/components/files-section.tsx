"use client"

import * as React from "react"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { Fact } from "@/features/import-opening/components/batch-facts"
import type { ImportBatchView } from "@/features/import-opening/types"
import { RETENTION_LABEL } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { versionText } from "@/lib/ui-text"

function formatBytes(n: number) {
    if (n < 1024) return `${n} B`
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
    return `${(n / (1024 * 1024)).toFixed(2)} MB`
}

export function FilesSection({ batch }: { batch: ImportBatchView }) {
    const [previewAsset, setPreviewAsset] = React.useState<string | null>(null)
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>合规输入包</CardTitle>
                    <CardDescription>
                        仅展示白名单包元数据；不展示原始存储键、签名 URL
                        或文件正文。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4 text-sm">
                    {batch.inputAsset ? (
                        <>
                            <Fact
                                label="文件名"
                                value={batch.inputAsset.fileName}
                            />
                            <Fact
                                label="大小"
                                value={formatBytes(batch.inputAsset.byteSize)}
                                mono
                            />
                            <Fact
                                label="安全检查"
                                value={
                                    batch.inputAsset.securityScanStatus ===
                                    "PASSED"
                                        ? "通过"
                                        : batch.inputAsset
                                                .securityScanStatus ===
                                            "PENDING"
                                          ? "待扫描"
                                          : batch.inputAsset
                                                  .securityScanStatus ===
                                              "REJECTED"
                                            ? "拒绝"
                                            : "隔离"
                                }
                            />
                            {batch.inputAsset.contentHmacShort ? (
                                <Fact
                                    label={versionText.checksumShort}
                                    value={batch.inputAsset.contentHmacShort}
                                    mono
                                />
                            ) : null}
                            <Fact
                                label="保留策略"
                                value={
                                    RETENTION_LABEL[
                                        batch.inputAsset.retentionClass
                                    ]
                                }
                            />
                        </>
                    ) : (
                        <p className="text-muted-foreground">
                            尚未接收合规包。
                        </p>
                    )}
                    <Separator />
                    <p className="text-xs text-muted-foreground">
                        禁止内容：原始
                        SQL、数据库连接头、商城禁止字段导出。此类文件不得长期留存，也不能在本页展示。
                    </p>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>结果与诊断资产保留</CardTitle>
                    <CardDescription>
                        成功审计长期 · 失败诊断 30 天 · 导出 7
                        天；下载前重鉴权。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.resultAssets.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            尚无结果资产。
                        </p>
                    ) : (
                        batch.resultAssets.map((a) => (
                            <div
                                key={a.assetId}
                                className="rounded-lg border px-3 py-2 text-sm"
                            >
                                <div className="font-medium">{a.fileName}</div>
                                <div className="mt-1 text-xs text-muted-foreground">
                                    {RETENTION_LABEL[a.retentionClass]}
                                    {a.expiresAt
                                        ? ` · 到期 ${formatDateTime(a.expiresAt, "dateStyle", "passthrough")}`
                                        : " · 无到期"}
                                    {" · "}
                                    {formatBytes(a.byteSize)}
                                </div>
                                <Button
                                    id={`operations-import-batch-detail-files-asset-${toAutomationIdSegment(a.assetId)}-preview`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    className="mt-2 h-7 text-xs"
                                    onClick={() => setPreviewAsset(a.fileName)}
                                >
                                    查看（示例）
                                </Button>
                                {previewAsset === a.fileName ? (
                                    <p className="mt-2 text-xs text-muted-foreground">
                                        示例：文件正文不在此处展示，仅保留元数据与保留策略。
                                    </p>
                                ) : null}
                            </div>
                        ))
                    )}
                </CardContent>
            </Card>
        </div>
    )
}
