/**
 * 通用审批区的前端稳定合同。
 *
 * 页面只消费服务端投影与 `allowed_actions` / `recovery_options`，
 * 不得根据单据状态、角色名或 BPM 内部事件推导责任、下一节点或恢复动作。
 */

/** 单据或实例上由服务端授权的动作。 */
export type ApprovalAllowedAction =
    | "APPROVE"
    | "REJECT"
    | "OPEN_DOCUMENT"
    | "VIEW"
    | "SUBMIT"
    | "CANCEL"
    | "CANCEL_APPROVAL"
    | "UPGRADE_BINDING"
    | "RESUME_CURRENT_APPROVER"
    | "REASSIGN_CURRENT_APPROVER"
    | "CANCEL_BLOCKED_APPROVAL"

/** 受阻实例的唯一合法恢复动作。 */
export type RecoveryOption =
    | "RESUME_CURRENT_APPROVER"
    | "REASSIGN_CURRENT_APPROVER"
    | "CANCEL_BLOCKED"

/** 实例状态。`REJECTED` 不是合同终态。 */
export type ApprovalInstanceStatus =
    | "RUNNING"
    | "APPROVED"
    | "CANCELLED"
    | "BLOCKED"

/** 节点执行状态。 */
export type ApprovalExecutionStatus =
    | "ACTIVE"
    | "APPROVED"
    | "REJECTED"
    | "CANCELLED"
    | "BLOCKED"
    | "SUPERSEDED"

/** 决定值。 */
export type ApprovalDecision = "APPROVE" | "REJECT"

/** 命令结果类别。 */
export type ApprovalCommandOutcome = "APPLIED" | "BLOCKED" | "IDEMPOTENT_REPLAY"

/** 实例列表固定视图。 */
export type ApprovalInstanceListView =
    | "mine"
    | "started"
    | "managed"
    | "blocked"

/** 决定请求只允许这五个字段。 */
export const DECISION_REQUEST_KEYS = [
    "work_item_id",
    "decision",
    "reason",
    "expected_task_version",
    "idempotency_key",
] as const

export type DecisionRequestKey = (typeof DECISION_REQUEST_KEYS)[number]

/** 决定 HTTP 请求体。 */
export type SubmitDecisionRequest = Readonly<{
    work_item_id: string
    decision: ApprovalDecision
    reason?: string | null
    expected_task_version: string
    idempotency_key: string
}>

/** 恢复当前审批人请求。不得带目标用户或恢复动作枚举。 */
export type ResumeApproverRequest = Readonly<{
    expected_instance_version: string
    expected_execution_version: string
    expected_assignment_version: string
    expected_closed_task_version?: string | null
    idempotency_key: string
}>

/** 改派当前审批人请求。 */
export type ReassignApproverRequest = Readonly<{
    target_user_id: string
    reason: string
    expected_instance_version: string
    expected_execution_version: string
    expected_assignment_version: string
    expected_closed_task_version?: string | null
    idempotency_key: string
}>

/** 受阻取消请求。blocker 由服务端推导。 */
export type CancelBlockedRequest = Readonly<{
    reason: string
    expected_instance_version: string
    expected_execution_version: string
    expected_task_version?: string | null
    idempotency_key: string
}>

/** 升级未提交绑定请求。不得提交定义 ID。 */
export type UpgradeBindingRequest = Readonly<{
    reason: string
    expected_document_version: string
    expected_approval_binding_version: string
    idempotency_key: string
}>

/** 业务撤回请求。走单据资源接口。 */
export type CancelApprovalRequest = Readonly<{
    reason: string
    expected_instance_version: string
    expected_execution_version: string
    expected_task_version?: string | null
    idempotency_key: string
}>

/** 下一开放任务摘要。 */
export type OpenTaskSummaryDto = Readonly<{
    work_item_id: string
    task_version: string | number
    owner_user_id: string
}>

/** 启动/决定/恢复/改派/取消的统一命令响应。 */
export type ApprovalCommandViewDto = Readonly<{
    instance_id: string
    instance_status: string
    current_round_no: number
    current_node_key?: string | null
    current_node_name?: string | null
    current_assignee_participant_id?: string | null
    current_assignee_name?: string | null
    subject_status?: string | null
    latest_rejection_reason?: string | null
    next_open_task?: OpenTaskSummaryDto | null
    outcome: ApprovalCommandOutcome
}>

/** 定义节点只读摘要。 */
export type ApprovalDefinitionNodeDto = Readonly<{
    key: string
    name: string
    assignee_name?: string | null
}>

/** 绑定定义只读摘要。 */
export type ApprovalDefinitionBindingDto = Readonly<{
    id: string
    name: string
    version: number
    nodes: readonly ApprovalDefinitionNodeDto[]
    published_version?: number | null
    published_name?: string | null
    published_nodes?: readonly ApprovalDefinitionNodeDto[] | null
    binding_version?: string | number | null
    document_version?: string | number | null
}>

/** 运行实例只读摘要。 */
export type ApprovalRuntimeInstanceDto = Readonly<{
    id: string
    status: string
    current_round_no: number
    current_node?: string | null
    current_node_name?: string | null
    current_assignee?: string | null
    current_assignee_name?: string | null
    latest_rejection?: string | null
    latest_rejection_by?: string | null
    instance_version?: string | number | null
    execution_version?: string | number | null
    assignment_version?: string | number | null
    process_name?: string | null
    process_version?: string | number | null
    blocker_code?: string | null
    blocker_message?: string | null
    started_by?: string | null
}>

/** 有界历史项。 */
export type ApprovalHistoryItemDto = Readonly<{
    execution_id: string
    round_no: number
    execution_no?: number | null
    node_key: string
    node_name?: string | null
    result: string
    assignee_name?: string | null
    decided_by?: string | null
    decision_reason?: string | null
    decided_at?: number | null
}>

/** 历史分页。 */
export type ApprovalHistoryPageDto = Readonly<{
    items?: readonly ApprovalHistoryItemDto[]
    next_cursor?: string | null
    has_more?: boolean
}>

/** 单据详情返回的统一只读审批结构。 */
export type DocumentApprovalViewDto = Readonly<{
    requirement: string
    definition?: ApprovalDefinitionBindingDto | null
    instance?: ApprovalRuntimeInstanceDto | null
    recent_history?: readonly ApprovalHistoryItemDto[] | null
    history_page?: ApprovalHistoryPageDto | null
    allowed_actions: readonly string[]
}>

/** 实例列表行。 */
export type ApprovalInstanceListItemDto = Readonly<{
    instance_id: string
    status: string
    current_round_no: number
    current_node_key?: string | null
    current_node_name?: string | null
    current_assignee_participant_id?: string | null
    current_assignee_name?: string | null
    document_type?: string | null
    document_id?: string | null
    document_label?: string | null
    process_name?: string | null
    process_version?: string | number | null
    blocker_code?: string | null
}>

/** 实例列表页。 */
export type ApprovalInstanceListPageDto = Readonly<{
    items: readonly ApprovalInstanceListItemDto[]
    next_cursor?: string | null
    total?: number | null
}>

/** 恢复选项。 */
export type RecoveryOptionsDto = Readonly<{
    instance_id: string
    actions: readonly string[]
}>

/** 改派候选人。 */
export type ReassigneeCandidateDto = Readonly<{
    user_id: string
    name: string
}>

export type OpenTaskSummary = Readonly<{
    workItemId: string
    taskVersion: string
    ownerUserId: string
}>

export type ApprovalCommandView = Readonly<{
    instanceId: string
    instanceStatus: ApprovalInstanceStatus | string
    currentRoundNo: number
    currentNodeKey?: string
    currentNodeName?: string
    currentAssigneeId?: string
    currentAssigneeName?: string
    subjectStatus?: string
    latestRejectionReason?: string
    nextOpenTask?: OpenTaskSummary
    outcome: ApprovalCommandOutcome
}>

export type ApprovalDefinitionNode = Readonly<{
    key: string
    name: string
    assigneeName?: string
}>

export type ApprovalDefinitionBinding = Readonly<{
    id: string
    name: string
    version: number
    nodes: readonly ApprovalDefinitionNode[]
    publishedVersion?: number
    publishedName?: string
    publishedNodes: readonly ApprovalDefinitionNode[]
    bindingVersion?: string
    documentVersion?: string
}>

export type ApprovalRuntimeInstance = Readonly<{
    id: string
    status: ApprovalInstanceStatus | string
    currentRoundNo: number
    currentNode?: string
    currentNodeName?: string
    currentAssignee?: string
    currentAssigneeName?: string
    latestRejection?: string
    latestRejectionBy?: string
    instanceVersion?: string
    executionVersion?: string
    assignmentVersion?: string
    processName?: string
    processVersion?: string
    blockerCode?: string
    blockerMessage?: string
    startedBy?: string
}>

export type ApprovalHistoryItem = Readonly<{
    executionId: string
    roundNo: number
    executionNo: number
    nodeKey: string
    nodeName: string
    result: string
    assigneeName?: string
    decidedBy?: string
    decisionReason?: string
    decidedAt?: number
}>

export type DocumentApprovalView = Readonly<{
    requirement: string
    definition?: ApprovalDefinitionBinding
    instance?: ApprovalRuntimeInstance
    recentHistory: readonly ApprovalHistoryItem[]
    historyNextCursor?: string
    historyHasMore: boolean
    allowedActions: readonly ApprovalAllowedAction[]
}>

export type ApprovalInstanceListItem = Readonly<{
    instanceId: string
    status: string
    currentRoundNo: number
    currentNodeKey?: string
    currentNodeName?: string
    currentAssigneeId?: string
    currentAssigneeName?: string
    documentType?: string
    documentId?: string
    documentLabel?: string
    processName?: string
    processVersion?: string
    blockerCode?: string
}>

export type ApprovalInstanceListPage = Readonly<{
    items: readonly ApprovalInstanceListItem[]
    nextCursor?: string
    total?: number
}>

export type RecoveryOptions = Readonly<{
    instanceId: string
    actions: readonly RecoveryOption[]
}>

export type ReassigneeCandidate = Readonly<{
    userId: string
    name: string
}>

const KNOWN_ALLOWED_ACTIONS = new Set<ApprovalAllowedAction>([
    "APPROVE",
    "REJECT",
    "OPEN_DOCUMENT",
    "VIEW",
    "SUBMIT",
    "CANCEL",
    "CANCEL_APPROVAL",
    "UPGRADE_BINDING",
    "RESUME_CURRENT_APPROVER",
    "REASSIGN_CURRENT_APPROVER",
    "CANCEL_BLOCKED_APPROVAL",
])

const KNOWN_RECOVERY_OPTIONS = new Set<RecoveryOption>([
    "RESUME_CURRENT_APPROVER",
    "REASSIGN_CURRENT_APPROVER",
    "CANCEL_BLOCKED",
])

/** 判断未知值是否为可安全读取的对象。 */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === "object" && value !== null

/**
 * 只保留合同白名单动作；未知码丢弃，禁止前端补默认动作。
 */
export const filterAllowedActions = (
    actions: readonly string[] | undefined,
): readonly ApprovalAllowedAction[] =>
    (actions ?? []).filter((action): action is ApprovalAllowedAction =>
        KNOWN_ALLOWED_ACTIONS.has(action as ApprovalAllowedAction),
    )

/**
 * 只保留服务端恢复选项。人员失效给恢复/改派，其它 blocker 只给受阻取消。
 */
export const filterRecoveryOptions = (
    actions: readonly string[] | undefined,
): readonly RecoveryOption[] =>
    (actions ?? []).flatMap((action) => {
        if (KNOWN_RECOVERY_OPTIONS.has(action as RecoveryOption)) {
            return [action as RecoveryOption]
        }
        if (action === "CANCEL_BLOCKED_APPROVAL") return ["CANCEL_BLOCKED"]
        return []
    })

const optionalText = (value?: string | null): string | undefined => {
    const text = value?.trim()
    return text ? text : undefined
}

const optionalVersion = (
    value?: string | number | null,
): string | undefined => {
    if (value == null) return undefined
    const text = String(value).trim()
    return text ? text : undefined
}

/**
 * 把定义节点 DTO 转成页面投影。
 */
export const mapDefinitionNodeDto = (
    dto: ApprovalDefinitionNodeDto,
): ApprovalDefinitionNode => ({
    key: dto.key,
    name: dto.name,
    assigneeName: optionalText(dto.assignee_name),
})

/**
 * 把绑定定义 DTO 转成只读投影，不含选择定义或换人控件所需字段。
 */
export const mapDefinitionBindingDto = (
    dto: ApprovalDefinitionBindingDto,
): ApprovalDefinitionBinding => ({
    id: dto.id,
    name: dto.name,
    version: dto.version,
    nodes: dto.nodes.map(mapDefinitionNodeDto),
    publishedVersion: dto.published_version ?? undefined,
    publishedName: optionalText(dto.published_name),
    publishedNodes: (dto.published_nodes ?? []).map(mapDefinitionNodeDto),
    bindingVersion: optionalVersion(dto.binding_version),
    documentVersion: optionalVersion(dto.document_version),
})

/**
 * 把运行实例 DTO 转成页面投影。
 */
export const mapRuntimeInstanceDto = (
    dto: ApprovalRuntimeInstanceDto,
): ApprovalRuntimeInstance => ({
    id: dto.id,
    status: dto.status,
    currentRoundNo: dto.current_round_no,
    currentNode: optionalText(dto.current_node ?? dto.current_node_name),
    currentNodeName: optionalText(dto.current_node_name ?? dto.current_node),
    currentAssignee: optionalText(dto.current_assignee),
    currentAssigneeName: optionalText(
        dto.current_assignee_name ?? dto.current_assignee,
    ),
    latestRejection: optionalText(dto.latest_rejection),
    latestRejectionBy: optionalText(dto.latest_rejection_by),
    instanceVersion: optionalVersion(dto.instance_version),
    executionVersion: optionalVersion(dto.execution_version),
    assignmentVersion: optionalVersion(dto.assignment_version),
    processName: optionalText(dto.process_name),
    processVersion: optionalVersion(dto.process_version),
    blockerCode: optionalText(dto.blocker_code),
    blockerMessage: optionalText(dto.blocker_message),
    startedBy: optionalText(dto.started_by),
})

/**
 * 把历史项 DTO 转成按轮次分组所需投影。不得按 node_key 去重。
 */
export const mapHistoryItemDto = (
    dto: ApprovalHistoryItemDto,
): ApprovalHistoryItem => ({
    executionId: dto.execution_id,
    roundNo: dto.round_no,
    executionNo: dto.execution_no ?? 0,
    nodeKey: dto.node_key,
    nodeName: optionalText(dto.node_name) ?? dto.node_key,
    result: dto.result,
    assigneeName: optionalText(dto.assignee_name),
    decidedBy: optionalText(dto.decided_by),
    decisionReason: optionalText(dto.decision_reason),
    decidedAt: dto.decided_at ?? undefined,
})

/**
 * 把单据审批区 DTO 转成通用组件投影。
 */
export const mapDocumentApprovalViewDto = (
    dto: DocumentApprovalViewDto,
): DocumentApprovalView => ({
    requirement: dto.requirement,
    definition: dto.definition
        ? mapDefinitionBindingDto(dto.definition)
        : undefined,
    instance: dto.instance ? mapRuntimeInstanceDto(dto.instance) : undefined,
    recentHistory: (dto.recent_history ?? []).map(mapHistoryItemDto),
    historyNextCursor: optionalText(dto.history_page?.next_cursor),
    historyHasMore: Boolean(dto.history_page?.has_more),
    allowedActions: filterAllowedActions(dto.allowed_actions),
})

/**
 * 把命令响应转成页面可用的最新事实。
 */
export const mapCommandViewDto = (
    dto: ApprovalCommandViewDto,
): ApprovalCommandView => ({
    instanceId: dto.instance_id,
    instanceStatus: dto.instance_status,
    currentRoundNo: dto.current_round_no,
    currentNodeKey: optionalText(dto.current_node_key),
    currentNodeName: optionalText(dto.current_node_name),
    currentAssigneeId: optionalText(dto.current_assignee_participant_id),
    currentAssigneeName: optionalText(dto.current_assignee_name),
    subjectStatus: optionalText(dto.subject_status),
    latestRejectionReason: optionalText(dto.latest_rejection_reason),
    nextOpenTask: dto.next_open_task
        ? {
              workItemId: dto.next_open_task.work_item_id,
              taskVersion: String(dto.next_open_task.task_version),
              ownerUserId: dto.next_open_task.owner_user_id,
          }
        : undefined,
    outcome: dto.outcome,
})

/**
 * 把实例列表行转成工作台/只读摘要投影。
 */
export const mapInstanceListItemDto = (
    dto: ApprovalInstanceListItemDto,
): ApprovalInstanceListItem => ({
    instanceId: dto.instance_id,
    status: dto.status,
    currentRoundNo: dto.current_round_no,
    currentNodeKey: optionalText(dto.current_node_key),
    currentNodeName: optionalText(dto.current_node_name),
    currentAssigneeId: optionalText(dto.current_assignee_participant_id),
    currentAssigneeName: optionalText(dto.current_assignee_name),
    documentType: optionalText(dto.document_type),
    documentId: optionalText(dto.document_id),
    documentLabel: optionalText(dto.document_label),
    processName: optionalText(dto.process_name),
    processVersion: optionalVersion(dto.process_version),
    blockerCode: optionalText(dto.blocker_code),
})

/**
 * 把恢复选项 DTO 转成页面投影。
 */
export const mapRecoveryOptionsDto = (
    dto: RecoveryOptionsDto,
): RecoveryOptions => ({
    instanceId: dto.instance_id,
    actions: filterRecoveryOptions(dto.actions),
})

/**
 * 把改派候选人 DTO 转成页面投影。
 */
export const mapReassigneeCandidateDto = (
    dto: ReassigneeCandidateDto,
): ReassigneeCandidate => ({
    userId: dto.user_id,
    name: dto.name,
})

/**
 * 构造决定请求，只输出合同 §14.3 白名单字段。
 */
export const buildDecisionRequest = (input: {
    workItemId: string
    decision: ApprovalDecision
    reason?: string
    expectedTaskVersion: string
    idempotencyKey: string
}): SubmitDecisionRequest => {
    const request: SubmitDecisionRequest = {
        work_item_id: input.workItemId,
        decision: input.decision,
        expected_task_version: input.expectedTaskVersion,
        idempotency_key: input.idempotencyKey,
    }
    const reason = input.reason?.trim()
    return reason ? { ...request, reason } : request
}

/**
 * 返回对象自有键，供测试断言请求白名单。
 */
export const requestKeysOf = (value: object): readonly string[] =>
    Object.keys(value).sort()
