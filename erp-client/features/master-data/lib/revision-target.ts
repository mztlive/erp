import type {
    MasterDataCenterView,
    MasterDataListItem,
} from "@/features/master-data/types"

export type RevisionTarget = MasterDataListItem | MasterDataCenterView

export function revisionTargetIds(target: RevisionTarget | null): {
    stableId: string
    stableNo: string
    name: string
    baseRevisionId: string
    lockVersion: number
} {
    if (!target) {
        return {
            stableId: "",
            stableNo: "",
            name: "",
            baseRevisionId: "",
            lockVersion: 0,
        }
    }
    const baseRevisionId =
        "currentRevisionId" in target
            ? target.currentRevisionId
            : target.currentRevision.revisionId
    return {
        stableId: target.stableId,
        stableNo: target.stableNo,
        name: target.name,
        baseRevisionId,
        lockVersion: target.lockVersion,
    }
}
