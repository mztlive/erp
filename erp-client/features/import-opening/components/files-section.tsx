"use client"

import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import type { ImportBatchView } from "@/features/import-opening/types"
import { RETENTION_LABEL } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"
import { versionText } from "@/lib/ui-text"

function formatBytes(n: number) {
    if (n < 1024) return `${n} B`
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
    return `${(n / (1024 * 1024)).toFixed(2)} MB`
}

export function FilesSection({ batch }: { batch: ImportBatchView }) {
    return (
        <>
            <DetailWorkspaceSection title="导入源文件">
                <div className="space-y-4">
                    {batch.inputAsset ? (
                        <DetailWorkspaceFacts columns={3}>
                            <DetailWorkspaceFact
                                label="文件名"
                                value={batch.inputAsset.fileName}
                            />
                            <DetailWorkspaceFact
                                label="大小"
                                value={formatBytes(batch.inputAsset.byteSize)}
                                numeric
                            />
                            <DetailWorkspaceFact
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
                                <DetailWorkspaceFact
                                    label={versionText.checksumShort}
                                    value={batch.inputAsset.contentHmacShort}
                                    numeric
                                />
                            ) : null}
                            <DetailWorkspaceFact
                                label="保留策略"
                                value={
                                    RETENTION_LABEL[
                                        batch.inputAsset.retentionClass
                                    ]
                                }
                            />
                        </DetailWorkspaceFacts>
                    ) : (
                        <p className="text-body-compact text-muted-foreground">
                            暂未上传导入文件。
                        </p>
                    )}
                </div>
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="导入结果与异常报告"
                subtitle="导入成功记录长期保存；失败明细保留 30 天，导出文件保留 7 天。"
            >
                {batch.resultAssets.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        暂无结果文件。
                    </p>
                ) : (
                    <ul className="divide-y divide-border/60">
                        {batch.resultAssets.map((a) => (
                            <li
                                key={a.assetId}
                                className="py-3 text-body-compact first:pt-0 last:pb-0"
                            >
                                <div className="font-medium text-foreground">
                                    {a.fileName}
                                </div>
                                <div className="mt-1 text-xs text-muted-foreground">
                                    {RETENTION_LABEL[a.retentionClass]}
                                    {a.expiresAt ? (
                                        <>
                                            {" · 到期 "}
                                            <span className="num">
                                                {formatDateTime(
                                                    a.expiresAt,
                                                    "dateStyle",
                                                    "passthrough",
                                                )}
                                            </span>
                                        </>
                                    ) : (
                                        " · 无到期"
                                    )}
                                    {" · "}
                                    <span className="num">
                                        {formatBytes(a.byteSize)}
                                    </span>
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </DetailWorkspaceSection>
        </>
    )
}
