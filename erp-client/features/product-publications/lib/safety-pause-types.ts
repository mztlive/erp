/**
 * W22 商品发布 · 安全暂停（供应商停供/库存/供给变动）契约类型。
 * 从 types.ts 拆出以控制文件体积；types.ts 原样再导出。
 */

export type SafetyPauseCause =
    | "SUPPLIER_STOPPED"
    | "ZERO_INVENTORY"
    | "SUPPLY_UNAVAILABLE"
    | "AVAILABILITY_STALE"
    | "COST_CHANGE_UNCONFIRMED"
    | "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED"

type SafetyPauseFollowUpWorkItemRef = {
    workItemId: string
    taskVersion: string
    workItemType: "BUSINESS_EXCEPTION"
    businessObjectType: "SUPPLIER_OFFERING"
    businessObjectId: string
    subjectVersion: string
    subjectHash: string
    handlerKey: string
}

type SafetyPauseNoTaskBlocker = {
    code: "NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY"
    message: string
    evidenceReference: string
}

type SafetyPauseReviewRegistrationBlocker = {
    code: "NORMAL_REVIEW_WORK_ITEM_TYPE_UNREGISTERED"
    message: string
    evidenceReference: string
}

type SafetyPauseAffectedPublicationView =
    | {
          publicationId: string
          pauseArtifactKind: "REVISION"
          pauseRevisionId: string
          deliveryId: string
          outboxMessageId: string
      }
    | {
          publicationId: string
          pauseArtifactKind: "ACTION"
          pauseActionId: string
          deliveryId: string
          outboxMessageId: string
      }

type KnownSafetyPauseOperationBase = {
    operationId: string
    resultStatus: "COMMITTED" | "ALREADY_SAFE"
    sourceObjectType: "SUPPLIER_OFFERING"
    sourceObjectId: string
    sourceVersion: string
    subjectHash: string
    availabilityEffect: "PAUSED"
    affectedPublications: [
        SafetyPauseAffectedPublicationView,
        ...SafetyPauseAffectedPublicationView[],
    ]
    committedAt: string
}

export type SystemSafetyPauseOperationView =
    | (KnownSafetyPauseOperationBase & {
          cause: "SUPPLIER_STOPPED"
          followUpWorkItem: SafetyPauseFollowUpWorkItemRef
          followUpBlocker?: never
      })
    | (KnownSafetyPauseOperationBase & {
          cause: "ZERO_INVENTORY" | "SUPPLY_UNAVAILABLE" | "AVAILABILITY_STALE"
          followUpWorkItem?: never
          followUpBlocker: SafetyPauseNoTaskBlocker
      })
    | (KnownSafetyPauseOperationBase & {
          cause:
              | "COST_CHANGE_UNCONFIRMED"
              | "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED"
          followUpWorkItem?: never
          followUpBlocker: SafetyPauseReviewRegistrationBlocker
      })
    | {
          operationId: string
          resultStatus: "UNKNOWN"
          cause: SafetyPauseCause
          sourceObjectType: "SUPPLIER_OFFERING"
          sourceObjectId: string
          sourceVersion: string
          subjectHash: string
          originalIdempotencyKey: string
          availabilityEffect: "FAIL_CLOSED_PENDING_RESULT"
          affectedPublications?: never
          followUpWorkItem?: never
          followUpBlocker?: never
          committedAt?: never
      }
