import { XIcon } from "lucide-react"
import {
    DetailWorkspaceTabs,
    type DetailWorkspaceTabItem,
} from "@/components/business"
import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Button } from "@/components/ui/button"
import { PortalApplicationContent } from "@/features/supplier-portal/components/application-content"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { PortalOfferingImpacts } from "./offering-impacts"
import { portalReviewOverview } from "../lib/review-overview"

export type PortalReviewEvidenceTab = "materials" | "impacts" | "history"

export function portalReviewEvidenceTabs(
    application: PortalApplication,
    fileCount: number,
): DetailWorkspaceTabItem[] {
    const tabs: DetailWorkspaceTabItem[] = []
    if (fileCount) {
        tabs.push({
            id: "materials",
            label: "附件",
            badge: <span className="num text-xs">{fileCount}</span>,
        })
    }
    if (
        (application.kind === "terms" || application.kind === "stop") &&
        portalReviewOverview(application).offeringId
    ) {
        tabs.push({ id: "impacts", label: "关联影响" })
    }
    tabs.push({ id: "history", label: "提交与处理记录" })
    return tabs
}

export function PortalReviewEvidence({
    application,
    files,
    tab,
    onTabChange,
    idPrefix,
    onClose,
}: {
    application: PortalApplication
    files: readonly MaterialFileItem[]
    tab: PortalReviewEvidenceTab
    onTabChange: (tab: PortalReviewEvidenceTab) => void
    idPrefix: string
    onClose?: () => void
}) {
    const tabs = portalReviewEvidenceTabs(application, files.length)
    const activeTab = tabs.some((entry) => entry.id === tab) ? tab : tabs[0].id
    const offeringId = portalReviewOverview(application).offeringId

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-col bg-card">
            {tabs.length > 1 ? (
                <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60">
                    <div className="min-w-0 flex-1">
                        <DetailWorkspaceTabs
                            value={activeTab}
                            onValueChange={(next) =>
                                onTabChange(next as PortalReviewEvidenceTab)
                            }
                            items={tabs}
                            ariaLabel="申请依据"
                            tabIdPrefix={idPrefix}
                        />
                    </div>
                    {onClose ? (
                        <Button
                            id={`${idPrefix}-close`}
                            type="button"
                            size="icon-sm"
                            variant="ghost"
                            aria-label="收起申请依据栏"
                            onClick={onClose}
                        >
                            <XIcon aria-hidden="true" />
                        </Button>
                    ) : null}
                </div>
            ) : (
                <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2">
                    <h2 className="truncate text-body-compact font-semibold text-foreground">
                        {tabs[0]?.label ?? "申请依据"}
                    </h2>
                    {onClose ? (
                        <Button
                            id={`${idPrefix}-close`}
                            type="button"
                            size="icon-sm"
                            variant="ghost"
                            aria-label="收起申请依据栏"
                            onClick={onClose}
                        >
                            <XIcon aria-hidden="true" />
                        </Button>
                    ) : null}
                </div>
            )}
            {activeTab === "materials" ? (
                <div className="flex min-h-80 flex-1 flex-col py-3">
                    <UnifiedMaterialViewer
                        mode="docked"
                        files={files}
                        idPrefix={`${idPrefix}-files`}
                        className="rounded-none border-0"
                    />
                </div>
            ) : (
                <div className="min-h-0 min-w-0 flex-1 overflow-y-auto py-2">
                    {activeTab === "impacts" ? (
                        <PortalOfferingImpacts
                            offeringId={offeringId}
                            enabled
                        />
                    ) : (
                        <>
                            {application.result?.summary ? (
                                <p className="pb-3 text-xs text-muted-foreground">
                                    {application.result.summary}
                                </p>
                            ) : null}
                            {application.submissions?.length ||
                            application.decisions?.length ? (
                                <PortalApplicationContent
                                    application={application}
                                    showImages={false}
                                    showDetails={false}
                                    showHistory
                                    nested
                                />
                            ) : (
                                <p className="py-3 text-xs text-muted-foreground">
                                    暂无提交或处理记录。
                                </p>
                            )}
                        </>
                    )}
                </div>
            )}
        </div>
    )
}
