"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ImportBatchView } from "@/features/import-opening/types"
import { versionText } from "@/lib/ui-text"

export function AuditSection({ batch }: { batch: ImportBatchView }) {
    return (
        <DetailWorkspaceSection title="导入溯源信息">
            <DetailWorkspaceFacts columns={3}>
                <DetailWorkspaceFact
                    label="批次号"
                    value={batch.batchNo}
                    numeric
                />
                <DetailWorkspaceFact
                    label="规则版本"
                    value={batch.importRuleVersion}
                    numeric
                />
                <DetailWorkspaceFact
                    label="预检版本"
                    value={batch.trialVersion}
                    numeric
                />
                <DetailWorkspaceFact
                    label="批次版本"
                    value={batch.version}
                    numeric
                />
                {batch.inputAsset?.contentHmacShort ? (
                    <DetailWorkspaceFact
                        label={versionText.packageChecksum}
                        value={batch.inputAsset.contentHmacShort}
                        numeric
                    />
                ) : null}
                {batch.linkedValidationBatchNo ? (
                    <DetailWorkspaceFact
                        label="关联验证/源批次"
                        value={batch.linkedValidationBatchNo}
                        numeric
                    />
                ) : null}
            </DetailWorkspaceFacts>
            <div>
                <Button
                    id="operations-import-batch-detail-audit-view"
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
        </DetailWorkspaceSection>
    )
}
