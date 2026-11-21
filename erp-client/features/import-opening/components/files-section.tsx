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
                    <CardTitle>导入源文件</CardTitle>
                    <CardDescription>
                        展示本次上传的期初数据文件基本信息与检查状态。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4 text-body-compact">
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
                            暂未上传导入文件。
                        </p>
                    )}
                    <Separator />
                    <p className="text-xs text-muted-foreground">
                        请按标准模板整理数据，避免包含系统无关字段。
                    </p>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>导入结果与异常报告</CardTitle>
                    <CardDescription>
                        导入成功记录长期保存；失败明细保留 30 天，导出文件保留 7
                        天。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.resultAssets.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            暂无结果文件。
                        </p>
                    ) : (
                        batch.resultAssets.map((a) => (
                            <div
                                key={a.assetId}
                                className="rounded-lg border px-3 py-2 text-body-compact"
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
                                    size="xs"
                                    variant="outline"
                                    className="mt-2"
                                    onClick={() => setPreviewAsset(a.fileName)}
                                >
                                    查看（示例）
                                </Button>
                                {previewAsset === a.fileName ? (
                                    <p className="mt-2 text-xs text-muted-foreground">
                                        当前仅展示文件基本信息与保存期限，不直接预览文件正文。
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
