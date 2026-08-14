import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { IntegrationFormalResult } from "../../types"
import { makeItem, makeResult } from "./test-fixtures"
import { useIntegrationActionFocus } from "./use-integration-action-focus"

beforeEach(() => {
    vi.useFakeTimers()
})

afterEach(() => {
    vi.useRealTimers()
})

describe("useIntegrationActionFocus", () => {
    it("exposes the focus refs and the first-action focus helper", () => {
        const { result } = renderHook(() =>
            useIntegrationActionFocus({ item: makeItem(), lastResult: null }),
        )
        expect(result.current.resultRef.current).toBeNull()
        expect(result.current.headingRef.current).toBeNull()
        expect(result.current.actionZoneRef.current).toBeNull()

        act(() => {
            result.current.focusFirstAction()
        })
        act(() => {
            vi.advanceTimersByTime(250)
        })
        // no rendered DOM: scroll/focus attempts are safe no-ops
        expect(result.current.actionZoneRef.current).toBeNull()
    })

    it("does not throw when the result or item changes without a rendered DOM", () => {
        const { result, rerender } = renderHook(
            ({ lastResult }: { lastResult: IntegrationFormalResult | null }) =>
                useIntegrationActionFocus({ item: makeItem(), lastResult }),
            {
                initialProps: {
                    lastResult: null,
                } as { lastResult: IntegrationFormalResult | null },
            },
        )
        rerender({ lastResult: makeResult() })
        rerender({ lastResult: null })
        expect(result.current.resultRef.current).toBeNull()
    })
})
