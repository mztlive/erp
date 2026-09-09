"use client"

import Link from "next/link"
import { useQuery } from "@tanstack/react-query"
import { ArrowUpRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    PreviewSection,
    PreviewNote,
} from "@/components/business/financial-preview"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    fetchSupplierSourceDocuments,
    type SupplierSourceScope,
} from "../api/source-documents"

export function SupplierSourceDocuments({
    scope,
}: {
    scope: SupplierSourceScope
}) {
    const hasTargets = Boolean(scope.allocations?.length || scope.entryId)
    const query = useQuery({
        queryKey: ["supplier-payables", "source-documents", scope],
        queryFn: () => fetchSupplierSourceDocuments(scope),
        enabled: hasTargets,
    })
    return (
        <PreviewSection title="原始单据">
            {!hasTargets ? (
                <PreviewNote>尚未关联采购单或结算单</PreviewNote>
            ) : query.isPending ? (
                <PreviewNote>正在读取原单…</PreviewNote>
            ) : query.isError ? (
                <div className="space-y-2">
                    <PreviewNote>原单读取失败，请重试。</PreviewNote>
                    <Button
                        id="supplier-payables-source-retry"
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
                            <li key={doc.href}>
                                <Button
                                    id={`supplier-payables-source-${toAutomationIdSegment(doc.href)}`}
                                    variant="outline"
                                    size="sm"
                                    className="max-w-full justify-start"
                                    render={<Link href={doc.href} />}
                                >
                                    <span className="num min-w-0 truncate">
                                        {doc.label}
                                    </span>
                                    <span className="shrink-0">
                                        {doc.action}
                                    </span>
                                    <ArrowUpRightIcon data-icon="inline-end" />
                                </Button>
                            </li>
                        ))}
                    </ul>
                    {query.data.unresolved ? (
                        <div className="space-y-2">
                            <PreviewNote>
                                部分原单暂不可用或不在当前可查看范围内。
                            </PreviewNote>
                            <Button
                                id="supplier-payables-source-retry-missing"
                                variant="outline"
                                size="sm"
                                onClick={() => void query.refetch()}
                            >
                                重新读取
                            </Button>
                        </div>
                    ) : null}
                </>
            )}
        </PreviewSection>
    )
}
