"use client"

import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { PanelRightCloseIcon, PanelRightOpenIcon } from "lucide-react"
import {
    DetailWorkspaceMeta,
    WorkspaceProcessingPane,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    PortalReviewEvidence,
    type PortalReviewEvidenceTab,
} from "@/features/supplier-portal-admin/components/review-evidence"
import { usePortalReviewMaterials } from "@/features/supplier-portal-admin/components/review-images"
import { usePortalReviewApplication } from "@/features/supplier-portal-admin/hooks"
import { portalReviewOverview } from "@/features/supplier-portal-admin/lib/review-overview"
import { PortalAdminReviewPage } from "@/features/supplier-portal-admin/pages/review-page"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

const useIsomorphicLayoutEffect =
    typeof window !== "undefined" ? useLayoutEffect : useEffect

export function WorkspaceSupplierPortalTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const { access, canRead, application } = usePortalReviewApplication(
        item.businessObjectId,
    )
    const { materialItems } = usePortalReviewMaterials(application)
    const [evidenceOpen, setEvidenceOpen] = useState(false)
    const [evidenceTab, setEvidenceTab] =
        useState<PortalReviewEvidenceTab | null>(null)
    const [wide, setWide] = useState(false)
    const rootRef = useRef<HTMLDivElement>(null)
    const prefix = `workspace-supplier-portal-${toAutomationIdSegment(item.workItemId)}`
    const evidencePanelId = `${prefix}-evidence`
    const activeTab =
        evidenceTab ?? (materialItems.length ? "materials" : "history")

    useIsomorphicLayoutEffect(() => {
        const node = rootRef.current
        if (!node) return
        const sync = () => setWide(node.clientWidth >= 960)
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(node)
        return () => observer.disconnect()
    }, [])

    useEffect(() => {
        if (!application) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") {
                setEvidenceOpen((open) => !open)
            } else if (
                action === "history" ||
                (action === "materials" && materialItems.length)
            ) {
                setEvidenceTab(action === "history" ? "history" : "materials")
                setEvidenceOpen(true)
            }
        })
    }, [application, materialItems.length])

    useEffect(() => {
        if (!wide && evidenceOpen) {
            document
                .getElementById(evidencePanelId)
                ?.scrollIntoView({ block: "nearest" })
        }
    }, [evidenceOpen, evidencePanelId, wide])

    const forbidden = !access.profile.isPending && !canRead
    const overview = application ? portalReviewOverview(application) : undefined
    const title = forbidden
        ? "业务详情"
        : item.workItemTypeLabel || "供应商申请确认"
    const number = application
        ? application.application_no ||
          stripDocumentNumberPrefix(item.stableNumber)
        : undefined
    const supplier = overview
        ? overview.supplier || item.counterpartyName
        : undefined
    const identity = [
        ...new Set(
            [supplier, overview?.kind].filter(
                (value) => value && value !== title && value !== number,
            ),
        ),
    ]
    const evidence = application ? (
        <PortalReviewEvidence
            application={application}
            files={materialItems}
            tab={activeTab}
            onTabChange={setEvidenceTab}
            idPrefix={evidencePanelId}
            onClose={wide ? () => setEvidenceOpen(false) : undefined}
        />
    ) : null

    return (
        <div
            ref={rootRef}
            className="flex h-full min-h-0 w-full min-w-0 flex-col bg-card"
        >
            <WorkspaceProcessingPane
                archetype="action"
                idPrefix={prefix}
                aria-label="供应商申请确认任务"
                contextRail={
                    wide && evidence
                        ? {
                              allowOnFullWidth: true,
                              collapsible: true,
                              resizable: true,
                              collapsed: !evidenceOpen,
                              onCollapsedChange: (collapsed) =>
                                  setEvidenceOpen(!collapsed),
                              label: "申请依据",
                              defaultWidthPercent: 34,
                              minWidthPercent: 24,
                              maxWidthPercent: 52,
                              preferredMinWidthPercent:
                                  activeTab === "materials" &&
                                  materialItems.length
                                      ? 42
                                      : undefined,
                              storageKey:
                                  "erp.workspace.supplierPortalReviewRailWidth",
                              content: (
                                  <div
                                      id={evidencePanelId}
                                      className="h-full min-h-0"
                                  >
                                      {evidence}
                                  </div>
                              ),
                          }
                        : undefined
                }
                header={
                    <WorkspaceTaskIdentityHeader
                        item={item}
                        title={title}
                        compact
                        subtitle={
                            <DetailWorkspaceMeta
                                items={[
                                    identity[0],
                                    number && number !== title ? (
                                        <span
                                            key="number"
                                            className="num select-all"
                                            data-workspace-document-number={
                                                number
                                            }
                                        >
                                            {number}
                                        </span>
                                    ) : null,
                                    identity[1],
                                    item.dueAt && item.dueAtLabel ? (
                                        <span key="deadline" className="num">
                                            截止 {item.dueAtLabel}
                                        </span>
                                    ) : null,
                                ]}
                            />
                        }
                    >
                        {evidence && !wide ? (
                            <Button
                                id={`${prefix}-toggle-evidence`}
                                type="button"
                                size="sm"
                                variant={evidenceOpen ? "secondary" : "ghost"}
                                aria-expanded={evidenceOpen}
                                aria-controls={
                                    evidenceOpen ? evidencePanelId : undefined
                                }
                                onClick={() => setEvidenceOpen((open) => !open)}
                            >
                                {evidenceOpen ? (
                                    <PanelRightCloseIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                ) : (
                                    <PanelRightOpenIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                )}
                                {evidenceOpen ? "收起依据" : "查看依据"}
                            </Button>
                        ) : null}
                    </WorkspaceTaskIdentityHeader>
                }
            >
                <PortalAdminReviewPage
                    applicationId={item.businessObjectId}
                    workItemId={item.workItemId}
                    embedded
                    fallbackSupplier={item.counterpartyName}
                    onTaskCompleted={onTaskCompleted}
                />
                {!wide && evidence && evidenceOpen ? (
                    <section
                        id={evidencePanelId}
                        aria-label="申请依据"
                        className="min-w-0 scroll-mt-2 border-t border-border/60 py-4"
                    >
                        {evidence}
                    </section>
                ) : null}
            </WorkspaceProcessingPane>
        </div>
    )
}
