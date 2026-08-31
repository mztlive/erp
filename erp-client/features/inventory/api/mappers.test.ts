import { describe, expect, it } from "vitest"

import {
    mapCommandViewDto,
    mapRuntimeInstanceDto,
} from "@/features/approval-workflow/types"
import type {
    BackendStockAdjustmentApproval,
    BackendStockAdjustmentCancelCommand,
    BackendStockAdjustmentSubmitCommand,
} from "@/features/inventory/api/dto"

import {
    mapAdjustmentApproval,
    mapStockAdjustmentCancelCommand,
    mapStockAdjustmentSubmitCommand,
} from "./mappers"

const cancelCommand: BackendStockAdjustmentCancelCommand = {
    expected_version: "9007199254740993",
    approval_process_instance_id: "instance-1",
    expected_subject_version: "4294967295",
    expected_instance_version: "9007199254740997",
    expected_execution_version: "9007199254740999",
    expected_task_version: null,
}

const submitCommand: BackendStockAdjustmentSubmitCommand = {
    expected_version: "9007199254741011",
    expected_subject_version: "4294967294",
}

const approvalDto: BackendStockAdjustmentApproval = {
    requirement: "PROCESS_REQUIRED",
    instance: {
        id: "instance-1",
        status: "BLOCKED",
        current_round_no: 1,
        subject_version: "4294967295",
        instance_version: "9007199254740997",
        current_execution_id: "execution-1",
        current_execution_version: "9007199254740999",
        current_task_id: null,
        current_task_version: null,
    },
    recent_history: [],
    history_page: { items: [], has_more: false },
    allowed_actions: ["SUBMIT", "CANCEL"],
    submit_command: submitCommand,
    cancel_command: cancelCommand,
}

describe("stock adjustment approval detail mapping", () => {
    it("retains the server-issued cancellation token without numeric conversion", () => {
        expect(mapAdjustmentApproval(approvalDto).cancelCommand).toEqual({
            expectedVersion: "9007199254740993",
            approvalProcessInstanceId: "instance-1",
            expectedSubjectVersion: "4294967295",
            expectedInstanceVersion: "9007199254740997",
            expectedExecutionVersion: "9007199254740999",
            expectedTaskVersion: null,
        })
    })

    it("retains the server-issued submit token without deriving the subject version", () => {
        expect(mapAdjustmentApproval(approvalDto).submitCommand).toEqual({
            expectedVersion: "9007199254741011",
            expectedSubjectVersion: "4294967294",
        })
    })

    it("rejects a malformed numeric version instead of guessing a string", () => {
        const malformed = {
            ...cancelCommand,
            expected_version: 9007199254740992,
        } as unknown as BackendStockAdjustmentCancelCommand

        expect(mapStockAdjustmentCancelCommand(malformed)).toBeUndefined()
    })

    it("rejects a numeric submit token instead of converting it", () => {
        const malformed = {
            ...submitCommand,
            expected_subject_version: 4294967294,
        } as unknown as BackendStockAdjustmentSubmitCommand

        expect(mapStockAdjustmentSubmitCommand(malformed)).toBeUndefined()
    })

    it.each([
        ["subject_version", 2],
        ["subject_version", "4294967296"],
        ["instance_version", "+1"],
        ["current_execution_version", "01"],
        ["current_execution_version", "18446744073709551616"],
    ])("drops a runtime carrying an invalid %s", (field, value) => {
        const instance = {
            ...approvalDto.instance,
            [field]: value,
        }
        const mapped = mapAdjustmentApproval({
            ...approvalDto,
            instance,
        } as BackendStockAdjustmentApproval)
        expect(mapped.instance).toBeUndefined()
        expect(mapped.cancelCommand).toBeUndefined()
    })

    it("drops incomplete execution and task identity pairs", () => {
        const executionWithoutVersion = mapAdjustmentApproval({
            ...approvalDto,
            instance: {
                ...approvalDto.instance!,
                current_execution_version: null,
            },
        })
        expect(executionWithoutVersion.instance).toBeUndefined()

        const taskWithoutVersion = mapAdjustmentApproval({
            ...approvalDto,
            instance: {
                ...approvalDto.instance!,
                current_task_id: "work-item-1",
                current_task_version: null,
            },
        })
        expect(taskWithoutVersion.instance).toBeUndefined()
    })

    it.each([
        ["RUNNING", null, null],
        ["BLOCKED", "work-item-1", "10"],
        ["APPROVED", null, null],
        ["CANCELLED", null, null],
        ["UNKNOWN", null, null],
    ])(
        "drops a runtime whose status/identity shape is invalid: %s",
        (status, currentTaskId, currentTaskVersion) => {
            const mapped = mapAdjustmentApproval({
                ...approvalDto,
                instance: {
                    ...approvalDto.instance!,
                    status,
                    current_task_id: currentTaskId,
                    current_task_version: currentTaskVersion,
                },
            })
            expect(mapped.instance).toBeUndefined()
            expect(mapped.cancelCommand).toBeUndefined()
        },
    )

    it("retains a RUNNING runtime only with a complete execution/task identity", () => {
        const mapped = mapAdjustmentApproval({
            ...approvalDto,
            instance: {
                ...approvalDto.instance!,
                status: "RUNNING",
                current_task_id: "work-item-1",
                current_task_version: "10",
            },
            cancel_command: {
                ...cancelCommand,
                expected_task_version: "10",
            },
        })
        expect(mapped.instance?.status).toBe("RUNNING")
        expect(mapped.instance?.currentTaskId).toBe("work-item-1")
        expect(mapped.cancelCommand?.expectedTaskVersion).toBe("10")
    })

    it.each(["APPROVED", "CANCELLED"])(
        "retains a terminal %s runtime only without current execution/task identity",
        (status) => {
            const mapped = mapAdjustmentApproval({
                ...approvalDto,
                instance: {
                    ...approvalDto.instance!,
                    status,
                    current_execution_id: null,
                    current_execution_version: null,
                    current_task_id: null,
                    current_task_version: null,
                },
                cancel_command: null,
            })
            expect(mapped.instance?.status).toBe(status)
        },
    )

    it.each([
        ["approval_process_instance_id", "instance-2"],
        ["expected_subject_version", "2"],
        ["expected_instance_version", "8"],
        ["expected_execution_version", "9"],
        ["expected_task_version", "10"],
    ])("drops a cancellation token whose %s drifts", (field, value) => {
        expect(
            mapAdjustmentApproval({
                ...approvalDto,
                cancel_command: {
                    ...cancelCommand,
                    [field]: value,
                },
            }).cancelCommand,
        ).toBeUndefined()
    })

    it.each(["", "0", "01", "+1", "-1", " 1", "1 ", "18446744073709551616"])(
        "rejects a non-canonical or overflowing u64 cancellation version: %j",
        (value) => {
            expect(
                mapStockAdjustmentCancelCommand({
                    ...cancelCommand,
                    expected_execution_version: value,
                }),
            ).toBeUndefined()
        },
    )

    it.each(["", "0", "01", "+1", "-1", " 1", "1 ", "4294967296"])(
        "rejects a non-canonical or overflowing subject version: %j",
        (value) => {
            expect(
                mapStockAdjustmentSubmitCommand({
                    ...submitCommand,
                    expected_subject_version: value,
                }),
            ).toBeUndefined()
            expect(
                mapStockAdjustmentCancelCommand({
                    ...cancelCommand,
                    expected_subject_version: value,
                }),
            ).toBeUndefined()
        },
    )

    it("retains the complete current runtime identity without numeric conversion", () => {
        expect(
            mapRuntimeInstanceDto({
                id: "instance-1",
                status: "RUNNING",
                current_round_no: 1,
                subject_version: "4294967295",
                current_execution_id: "execution-1",
                current_execution_version: "9007199254740999",
                execution_version: "legacy-value",
                current_task_id: "work-item-1",
                current_task_version: "9007199254741001",
            }),
        ).toMatchObject({
            subjectVersion: "4294967295",
            currentExecutionId: "execution-1",
            executionVersion: "9007199254740999",
            currentTaskId: "work-item-1",
            currentTaskVersion: "9007199254741001",
        })
    })

    it("keeps next-task u64 versions as strings and rejects numeric wire values", () => {
        const base = {
            instance_id: "instance-1",
            instance_status: "RUNNING",
            current_round_no: 1,
            outcome: "APPLIED" as const,
        }
        expect(
            mapCommandViewDto({
                ...base,
                next_open_task: {
                    work_item_id: "work-item-1",
                    task_version: "18446744073709551615",
                    owner_user_id: "user-1",
                },
            }).nextOpenTask?.taskVersion,
        ).toBe("18446744073709551615")
        expect(
            mapCommandViewDto({
                ...base,
                next_open_task: {
                    work_item_id: "work-item-1",
                    task_version: 9007199254740992,
                    owner_user_id: "user-1",
                },
            } as unknown as Parameters<typeof mapCommandViewDto>[0])
                .nextOpenTask,
        ).toBeUndefined()
    })
})
