"use client"

import Link from "next/link"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import {
    financialDraftEditHref,
    isFinancialDraft,
    isFinancialDraftEditor,
    type FinancialDraftKind,
} from "../types"
import { useFinancialDraftQuery } from "../hooks/queries"

/** 未提交草稿可从详情继续修改原单。 */
export function FinancialDraftEditButton({
    kind,
    documentId,
}: {
    kind: FinancialDraftKind
    documentId: string
}) {
    const profile = useAccountProfileQuery()
    const permitted = hasPermission(profile.data?.permissions, `${kind}:submit`)
    const query = useFinancialDraftQuery(kind, documentId, permitted)
    if (
        !permitted ||
        query.isError ||
        !query.data ||
        !isFinancialDraft(query.data) ||
        !isFinancialDraftEditor(kind, query.data, profile.data?.userid)
    )
        return null
    return (
        <Button
            id={`financial-draft-edit-${toAutomationIdSegment(kind)}-${toAutomationIdSegment(documentId)}-open`}
            type="button"
            size="sm"
            variant="outline"
            render={<Link href={financialDraftEditHref(kind, documentId)} />}
        >
            修改原单
        </Button>
    )
}
