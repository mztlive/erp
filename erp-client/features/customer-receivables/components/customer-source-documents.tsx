"use client"

import Link from "next/link"
import { useQuery } from "@tanstack/react-query"
import { ArrowUpRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    PreviewNote,
    PreviewSection,
} from "@/components/business/financial-preview"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    fetchCustomerSourceDocuments,
    type CustomerSourceScope,
} from "../api/source-documents"

/** 关联单据局部加载，不阻塞金额和审批区；无来源时明确显示未关联。 */
export function CustomerSourceDocuments({
    scope,
}: {
    scope: CustomerSourceScope
}) {
    const hasTargets = Boolean(
        scope.accountIds?.length || scope.entryIds?.length,
    )
    const query = useQuery({
        queryKey: ["customer-receivables", "source-documents", scope],
        queryFn: () => fetchCustomerSourceDocuments(scope),
        enabled: hasTargets,
    })
    return (
        <PreviewSection title="原始单据">
            {!hasTargets ? (
                <PreviewNote>尚未关联销售单</PreviewNote>
            ) : query.isPending ? (
                <PreviewNote>正在读取原单…</PreviewNote>
            ) : query.isError ? (
                <div className="space-y-2">
                    <PreviewNote>原单读取失败，请重试。</PreviewNote>
                    <Button
                        id="customer-receivables-source-retry"
                        variant="outline"
                        size="sm"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : (
                <>
                    <ul className="space-y-2">
                        {query.data.documents.map((doc) => (
                            <li key={doc.id}>
                                <Button
                                    id={`customer-receivables-source-${toAutomationIdSegment(doc.id)}`}
                                    variant="outline"
                                    size="sm"
                                    className="max-w-full justify-start"
                                    render={<Link href={doc.href} />}
                                >
                                    <span className="num min-w-0 truncate">
                                        {doc.label}
                                    </span>
                                    <span className="shrink-0">打开销售单</span>
                                    <ArrowUpRightIcon data-icon="inline-end" />
                                </Button>
                            </li>
                        ))}
                    </ul>
                    {query.data.unresolved ? (
                        <PreviewNote>
                            部分原单暂不可用或不在当前可查看范围内。
                        </PreviewNote>
                    ) : null}
                </>
            )}
        </PreviewSection>
    )
}
