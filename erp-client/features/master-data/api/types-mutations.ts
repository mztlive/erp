/** W14 基础资料 · 变更命令输入输出契约类型。 */

import type { MasterDataResource } from "@/features/master-data/api/types-core"
import type { MasterDataResourceFields } from "@/features/master-data/api/types-fields"

/** 随一次基础资料业务命令提交的本地文件。 */
export type PendingAssetUpload = Readonly<{
    /** 与业务 DTO 中临时 file_asset_id 完全一致的请求内引用。 */
    reference: string
    file: File
}>

export type CreateMasterDataInput = Readonly<{
    resource: MasterDataResource
    name: string
    effectiveFrom: string
    effectiveTo?: string
    changeReason: string
    fields: MasterDataResourceFields[MasterDataResource]
    idempotencyKey: string
    pendingAssetUploads?: readonly PendingAssetUpload[]
}>

export type CreateRevisionInput = Readonly<{
    resource: MasterDataResource
    stableId: string
    baseRevisionId: string
    expectedLockVersion: number
    /** 聚合跨 Party 修订时的独立乐观锁版本。 */
    expectedPartyVersion?: number
    name: string
    effectiveFrom: string
    effectiveTo?: string
    changeReason: string
    fields: MasterDataResourceFields[MasterDataResource]
    idempotencyKey: string
    pendingAssetUploads?: readonly PendingAssetUpload[]
}>

export type DisableMasterDataInput = Readonly<{
    resource: MasterDataResource
    stableId: string
    baseRevisionId: string
    expectedLockVersion: number
    changeReason: string
    effectiveFrom: string
    idempotencyKey: string
}>

export type MasterDataMutationResult =
    | {
          outcome: "succeeded"
          stableId: string
          stableNo: string
          revisionId: string
          revisionNo: number
          revisionState: "CURRENT" | "FUTURE"
          effectiveFrom: string
          recordedAt: string
          actor: string
          changeReason: string
          reference: string
          nextActions: readonly string[]
      }
    | {
          outcome: "blocked"
          code: string
          message: string
          detail?: string
          drillHref?: string
      }
    | {
          outcome: "conflict"
          message: string
          serverLockVersion: number
          serverRevisionNo: number
      }
    | {
          outcome: "unknown"
          message: string
          idempotencyKey: string
      }
