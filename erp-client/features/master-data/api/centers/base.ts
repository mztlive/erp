/** 对象中心的公共骨架：从列表行构造基础详情视图，供各资源补充专属投影。 */

import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataResource,
} from "@/features/master-data/types"

export function baseCenter(
    resource: MasterDataResource,
    row: MasterDataListItem,
    extras: Partial<MasterDataCenterView> = {},
): MasterDataCenterView {
    return {
        resource,
        stableId: row.stableId,
        stableNo: row.stableNo,
        name: row.name,
        lifecycleStatus: row.lifecycleStatus,
        lifecycleStatusLabel: row.lifecycleStatusLabel,
        lifecycleTone: row.lifecycleTone,
        scheduledLifecycleStatus: row.scheduledLifecycleStatus,
        scheduledLifecycleLabel: row.scheduledLifecycleLabel,
        revisionTiming: row.revisionTiming,
        revisionTimingLabel: row.revisionTimingLabel,
        lockVersion: row.lockVersion,
        currentRevision: {
            revisionId: row.currentRevisionId,
            revisionNo: row.revisionNo,
            name: row.name,
            effectiveFrom: row.effectiveFrom,
            effectiveTo: row.effectiveTo,
            changeReason: "—",
            actor: "—",
            fields: row.keyFacts.map((f) => ({
                label: f.label,
                value: f.value,
            })),
        },
        revisionTimeline: [
            {
                id: row.currentRevisionId,
                revisionNo: row.revisionNo,
                revisionTiming:
                    row.revisionTiming === "FUTURE" ? "FUTURE" : "CURRENT",
                timingLabel: row.revisionTimingLabel,
                nameSnapshot: row.name,
                actor: "—",
                effectiveFrom: row.effectiveFrom,
                effectiveTo: row.effectiveTo,
                changeReason: "—",
                isCurrent: true,
                lifecycleAtRevision: row.lifecycleStatus,
            },
        ],
        selectorEligibility: row.selectorEligibility,
        usageSummary: {
            historicalReferenceCount: 0,
            note: "引用摘要由后端投影提供；当前接口未返回业务引用数。",
        },
        sensitiveFields: [],
        resourceFacts: [...row.keyFacts],
        allowedActions: row.allowedActions,
        actionBlockers: row.actionBlockers,
        auditEvents: [],
        sections: ["overview", "versions", "relations", "audit"],
        ...extras,
    }
}
