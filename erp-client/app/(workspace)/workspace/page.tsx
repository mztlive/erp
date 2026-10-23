import type { Metadata } from "next"
import { Suspense } from "react"
import { redirect } from "next/navigation"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"

import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { WorkspaceHomePage as WorkspaceHome } from "@/features/workspace/pages/workspace-home-page"

export const metadata: Metadata = {
    title: "我的工作台",
}

export default async function WorkspaceHomePage({
    searchParams,
}: {
    searchParams: Promise<Record<string, string | string[] | undefined>>
}) {
    const params = await searchParams
    const current =
        typeof params.currentWorkItemId === "string"
            ? params.currentWorkItemId
            : undefined
    const copy =
        typeof params.copyReceiptId === "string"
            ? params.copyReceiptId
            : undefined
    if (current || copy) {
        const query = new URLSearchParams()
        for (const [key, value] of Object.entries(params)) {
            if (typeof value === "string") query.set(key, value)
        }
        redirect(
            buildWorkspaceDetailHref(
                copy
                    ? "copies"
                    : params.view === "started"
                      ? "approvals"
                      : "tasks",
                (copy ?? current)!,
                {
                    returnTo: workspaceQueueHref(`/workspace?${query}`),
                },
            ),
        )
    }
    return (
        <Suspense fallback={<WorkspaceHomeSkeleton />}>
            <WorkspaceHome />
        </Suspense>
    )
}
