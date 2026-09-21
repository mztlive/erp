"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { AuditEventBody } from "@/features/access-audit/components/audit-event-body"
import { EffectiveAccessBody } from "@/features/access-audit/components/effective-access-body"
import {
    useAuditEventQuery,
    useEffectiveAccessQuery,
} from "@/features/access-audit/hooks/queries"
import type { RoleRow } from "@/features/access-audit/types"

const EFFECTIVE_SHEET_PREFIX = "access-preview-effective-access"
const SELLABLE_PREVIEW_WIDTH =
    "data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px]"

type AccessPreviewSheetsProps = {
    explainSubject: { type: "ROLE" | "USER"; id: string } | null
    previewRole?: RoleRow | null
    eventOpenId: string | null
    effectiveQuery: ReturnType<typeof useEffectiveAccessQuery>
    eventQuery: ReturnType<typeof useAuditEventQuery>
    closeExplain: () => void
    closeEvent: () => void
    restoreRowFocus: () => void
}

function AccessPreviewSheets({
    explainSubject,
    previewRole,
    eventOpenId,
    effectiveQuery,
    eventQuery,
    closeExplain,
    closeEvent,
    restoreRowFocus,
}: AccessPreviewSheetsProps) {
    const subjectLabel =
        previewRole?.name ??
        effectiveQuery.data?.subject.label ??
        (explainSubject?.type === "USER" ? "账号" : "角色")
    const boundAccountCount = previewRole?.boundAccountCount
    const openRoleHref =
        explainSubject?.type === "ROLE"
            ? `/system/roles/${explainSubject.id}/edit`
            : null

    return (
        <>
            <QuickPreviewSheet
                open={Boolean(explainSubject)}
                onOpenChange={(open) => {
                    if (!open) closeExplain()
                }}
                size="preview"
                contentClassName={SELLABLE_PREVIEW_WIDTH}
                onOpenChangeComplete={(open) => {
                    if (!open) restoreRowFocus()
                }}
                idPrefix={EFFECTIVE_SHEET_PREFIX}
                title={subjectLabel}
                description={previewRole?.system ? "系统内置角色" : undefined}
                summary={
                    previewRole ? (
                        <div className="flex flex-wrap items-center gap-2">
                            <BusinessStatusBadge
                                context="preview"
                                label={previewRole.statusLabel}
                                tone={previewRole.statusTone}
                            />
                            <span className="text-xs text-muted-foreground">
                                {boundAccountCount === 0 ? (
                                    "尚未绑定账号"
                                ) : (
                                    <>
                                        已绑定{" "}
                                        <span className="num">
                                            {boundAccountCount}
                                        </span>{" "}
                                        个账号
                                    </>
                                )}
                            </span>
                        </div>
                    ) : null
                }
                footer={
                    <>
                        <Button
                            id={`${EFFECTIVE_SHEET_PREFIX}-dismiss`}
                            type="button"
                            variant="outline"
                            onClick={closeExplain}
                        >
                            关闭
                        </Button>
                        {openRoleHref ? (
                            <Button
                                id={`${EFFECTIVE_SHEET_PREFIX}-open-role`}
                                type="button"
                                render={<Link href={openRoleHref} />}
                            >
                                打开角色资料
                                <ArrowUpRightIcon
                                    data-icon="inline-end"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                    </>
                }
            >
                <EffectiveAccessBody
                    query={effectiveQuery}
                    previewRole={previewRole}
                />
            </QuickPreviewSheet>

            <QuickPreviewSheet
                open={Boolean(eventOpenId)}
                onOpenChange={(open) => {
                    if (!open) closeEvent()
                }}
                size="detail"
                onOpenChangeComplete={(open) => {
                    if (!open) restoreRowFocus()
                }}
                idPrefix="access-preview-audit-event"
                footer={
                    <Button
                        id="access-preview-audit-event-dismiss"
                        variant="outline"
                        onClick={closeEvent}
                    >
                        关闭
                    </Button>
                }
                title="审计事件详情"
                description="追加式事件只读；不展示敏感旧值/新值或密钥。"
            >
                <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                    <AuditEventBody query={eventQuery} />
                </div>
            </QuickPreviewSheet>
        </>
    )
}

export { AccessPreviewSheets }
