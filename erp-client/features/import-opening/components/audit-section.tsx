"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Fact } from "@/features/import-opening/components/batch-facts"
import type { ImportBatchView } from "@/features/import-opening/types"
import { versionText } from "@/lib/ui-text"

export function AuditSection({ batch }: { batch: ImportBatchView }) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>可追溯谱系</CardTitle>
                <CardDescription>
                    来源身份、规则版本、manifest、成功结果与映射谱系可审计；详细事件在权限与审计中。
                </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3 pt-4 sm:grid-cols-2">
                <Fact label="批次号" value={batch.batchNo} mono />
                <Fact label="规则版本" value={batch.importRuleVersion} mono />
                <Fact label="试算版本" value={batch.trialVersion} mono />
                <Fact label="批次版本" value={batch.version} mono />
                {batch.inputAsset?.contentHmacShort ? (
                    <Fact
                        label={versionText.packageChecksum}
                        value={batch.inputAsset.contentHmacShort}
                        mono
                    />
                ) : null}
                {batch.linkedValidationBatchNo ? (
                    <Fact
                        label="关联验证/源批次"
                        value={batch.linkedValidationBatchNo}
                    />
                ) : null}
                <div className="sm:col-span-2">
                    <Button
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={`/system/access-audit?objectType=legacy_import_batch&objectId=${encodeURIComponent(batch.batchId)}`}
                            />
                        }
                    >
                        在权限与审计中查看
                        <ExternalLinkIcon className="size-4" />
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}
