"use client"

import { BusinessFailureState, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { AllocationSessionPanel } from "@/features/customer-receivables/components/allocation-session-panel"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

type AllocationSessionScreenProps = {
    isPending: boolean
    session: AllocationSessionView | null | undefined
    onBackToList: () => void
    onClose: () => void
    onPosted: () => void
}

/** 核销会话全屏态：加载骨架 / 失效提示 / 会话面板。 */
export function AllocationSessionScreen({
    isPending,
    session,
    onBackToList,
    onClose,
    onPosted,
}: AllocationSessionScreenProps) {
    if (isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }
    if (!session) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    kind="business"
                    title="本次核销无效"
                    description="本次核销已失效，请重新开始。"
                    action={
                        <Button type="button" onClick={onBackToList}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }
    return (
        <PageScaffold>
            <AllocationSessionPanel
                session={session}
                onClose={onClose}
                onPosted={onPosted}
            />
        </PageScaffold>
    )
}
