import { describe, expect, it } from "vitest"

import type { EditorNode } from "./types"
import {
    assertWritePayloadSafe,
    buildReplaceNodesRequest,
    buildStableReplaceNodesCommand,
    ownKeys,
} from "./write-payload"

const nodes: EditorNode[] = [
    {
        client_id: "existing-node",
        node_id: "node-1",
        node_name: " 财务审批 ",
        assignee_user_id: "user-1",
        assignee_name: "审批人甲",
        node_purpose: null,
        unsaved_purpose_slot: false,
    },
    {
        client_id: "new-node",
        node_id: null,
        node_name: "负责人审批",
        assignee_user_id: "user-2",
        assignee_name: "审批人乙",
        node_purpose: null,
        unsaved_purpose_slot: false,
    },
]

describe("ReplaceNodes 写载荷", () => {
    it("原样携带幂等键并只输出协议白名单字段", () => {
        const request = buildReplaceNodesRequest(
            "9007199254740993",
            nodes,
            "replace-nodes:request-1",
        )

        expect(request).toEqual({
            expected_definition_lock_version: "9007199254740993",
            nodes: [
                {
                    node_id: "node-1",
                    node_name: "财务审批",
                    display_order: 1,
                    assignee_user_id: "user-1",
                },
                {
                    node_name: "负责人审批",
                    display_order: 2,
                    assignee_user_id: "user-2",
                },
            ],
            idempotency_key: "replace-nodes:request-1",
        })
        expect(ownKeys(request)).toEqual([
            "expected_definition_lock_version",
            "nodes",
            "idempotency_key",
        ])
        expect(() => assertWritePayloadSafe(request)).not.toThrow()
    })

    it("相同未决载荷复用完整命令，载荷变化才生成新键", () => {
        let generated = 0
        const createKey = () => `replace-nodes:request-${++generated}`
        const first = buildStableReplaceNodesCommand(
            "definition-1",
            "7",
            nodes,
            null,
            createKey,
        )
        const retry = buildStableReplaceNodesCommand(
            "definition-1",
            "7",
            nodes,
            first,
            createKey,
        )

        expect(retry).toBe(first)
        expect(retry.request.idempotency_key).toBe("replace-nodes:request-1")
        expect(generated).toBe(1)

        const changed = buildStableReplaceNodesCommand(
            "definition-1",
            "7",
            [{ ...nodes[0]!, node_name: "复核审批" }, nodes[1]!],
            retry,
            createKey,
        )

        expect(changed).not.toBe(retry)
        expect(changed.request.idempotency_key).toBe("replace-nodes:request-2")
        expect(generated).toBe(2)
    })
})
